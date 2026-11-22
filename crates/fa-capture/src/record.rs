//! Fixed-size `Copy` records produced by the decoders (no heap, so they can go through the lock-free rings).

use std::fmt;

/// A name stored inline as UTF-8. `truncated` is set when the source did not fit: such a name is never complete.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Name {
    len: u8,
    truncated: bool,
    bytes: [u8; Name::CAP],
}

impl Name {
    pub const CAP: usize = 62;

    pub const fn empty() -> Self {
        Self {
            len: 0,
            truncated: false,
            bytes: [0; Self::CAP],
        }
    }

    fn push(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        let len = usize::from(self.len);
        if len + s.len() > Self::CAP {
            self.truncated = true;
        } else {
            self.bytes[len..len + s.len()].copy_from_slice(s.as_bytes());
            self.len += s.len() as u8;
        }
    }

    /// From UTF-16 units, keeping only the part after the last `\` (a module path gives its file name). Unpaired
    /// surrogates become U+FFFD.
    pub fn from_utf16_basename(units: impl Iterator<Item = u16>) -> Self {
        let mut n = Self::empty();
        for c in char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)) {
            if c == '\\' {
                n = Self::empty();
            } else {
                n.push(c);
            }
        }
        n
    }

    /// From single-byte characters (the ANSI image name of a process-stop event; each byte is read as Latin-1).
    pub fn from_ansi(bytes: impl Iterator<Item = u8>) -> Self {
        let mut n = Self::empty();
        for b in bytes {
            n.push(char::from(b));
        }
        n
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or("")
    }

    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// The name only if it is whole: a truncated name must never identify a process (fail closed).
    pub fn complete(&self) -> Option<&str> {
        (!self.truncated && self.len > 0).then(|| self.as_str())
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}{}",
            self.as_str(),
            if self.truncated { " (truncated)" } else { "" }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpcKind {
    Isr,
    Dpc,
    TimerDpc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessKind {
    Start,
    Rundown,
    Stop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    Load,
    Unload,
    DcStart,
    DcEnd,
}

/// High-rate events: 32 bytes each. Timestamps are raw TSC ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotRecord {
    /// DXGI `Present_Start`: the frametime source (interval between two starts of the same process and swap chain).
    Present {
        ts: u64,
        swap_chain: u64,
        pid: u32,
        flags: u32,
        sync_interval: u8,
    },
    /// DxgKrnl `Present_Info`: the only present event of apps that bypass DXGI (OpenGL, Vulkan).
    DxgPresent {
        ts: u64,
        window: u64,
        pid: u32,
        flip_interval: u32,
        flags: u32,
    },
    /// Interrupt or DPC; `ts - initial_time` is its duration, `routine` a kernel address.
    Dpc {
        ts: u64,
        initial_time: u64,
        routine: u64,
        cpu: u16,
        vector: u16,
        kind: DpcKind,
    },
    CSwitch {
        ts: u64,
        new_tid: u32,
        old_tid: u32,
        cpu: u16,
        new_priority: i8,
        old_priority: i8,
    },
    /// Disk read or write completed; `response` in the event's own high-resolution unit.
    DiskIo {
        ts: u64,
        response: u64,
        size: u32,
        disk: u32,
        issuing_tid: u32,
        write: bool,
    },
}

/// Rare events that carry a name: they go through their own, smaller ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColdRecord {
    /// Full image name from Microsoft-Windows-Kernel-Process (the only source of process names).
    Process {
        ts: u64,
        pid: u32,
        parent_pid: u32,
        kind: ProcessKind,
        name: Name,
    },
    /// Kernel or user image from the classic image class (`name` is the file name without its directory).
    Image {
        ts: u64,
        base: u64,
        size: u64,
        pid: u32,
        kind: ImageKind,
        name: Name,
    },
}

const _: () = assert!(std::mem::size_of::<HotRecord>() == 32);
const _: () = assert!(std::mem::size_of::<ColdRecord>() <= 96);
