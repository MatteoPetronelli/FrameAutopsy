//! Process identification without handles: PID to full image name, from Microsoft-Windows-Kernel-Process events only.
//! A truncated name is never stored, and classic kernel process events (15-byte names) are never decoded at all.

use crate::record::{ColdRecord, ProcessKind};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ProcessTable {
    names: HashMap<u32, String>,
    /// Start or rundown events whose name did not fit: the process exists, its name is unknown.
    refused: u64,
}

impl ProcessTable {
    pub fn apply(&mut self, record: &ColdRecord) {
        let ColdRecord::Process {
            pid, kind, name, ..
        } = record
        else {
            return;
        };
        match kind {
            ProcessKind::Start | ProcessKind::Rundown => match name.complete() {
                Some(n) => {
                    self.names.insert(*pid, n.to_owned());
                }
                None => {
                    self.names.remove(pid);
                    self.refused += 1;
                }
            },
            ProcessKind::Stop => {
                self.names.remove(pid);
            }
        }
    }

    /// The full image name of `pid`, if a Kernel-Process event gave a complete one.
    pub fn name_of(&self, pid: u32) -> Option<&str> {
        self.names.get(&pid).map(String::as_str)
    }

    /// How many start or rundown events were refused because the name was truncated.
    pub fn refused_names(&self) -> u64 {
        self.refused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{decode, Decoded};
    use crate::record::Name;
    use crate::view::{guids, EventView};

    fn process(pid: u32, kind: ProcessKind, name: &str) -> ColdRecord {
        ColdRecord::Process {
            ts: 0,
            pid,
            parent_pid: 0,
            kind,
            name: Name::from_utf16_basename(name.encode_utf16()),
        }
    }

    #[test]
    fn start_rundown_and_stop_maintain_the_table() {
        let mut t = ProcessTable::default();
        t.apply(&process(10, ProcessKind::Rundown, "explorer.exe"));
        t.apply(&process(
            11,
            ProcessKind::Start,
            "VALORANT-Win64-Shipping.exe",
        ));
        assert_eq!(t.name_of(11), Some("VALORANT-Win64-Shipping.exe"));
        t.apply(&process(
            11,
            ProcessKind::Stop,
            "VALORANT-Win64-Shipping.exe",
        ));
        assert_eq!(t.name_of(11), None);
        assert_eq!(t.name_of(10), Some("explorer.exe"));
    }

    #[test]
    fn a_name_too_long_to_store_is_refused_not_guessed() {
        let mut t = ProcessTable::default();
        t.apply(&process(5, ProcessKind::Start, &"a".repeat(80)));
        assert_eq!(t.name_of(5), None);
        assert_eq!(t.refused_names(), 1);
    }

    #[test]
    fn a_classic_kernel_event_with_a_15_byte_name_never_reaches_the_table() {
        let mut data = vec![0u8; 64];
        data.extend(b"VALORANT-Win64-\0"); // 15 bytes: the classic truncation of VALORANT-Win64-Shipping.exe
        let v = EventView {
            provider: guids::NT_PROCESS,
            id: 0,
            version: 4,
            opcode: 1,
            timestamp: 0,
            pid: 1,
            tid: 1,
            cpu: 0,
            ptr32: false,
            data: &data,
        };
        let mut t = ProcessTable::default();
        match decode(&v) {
            Decoded::Cold(r) => t.apply(&r),
            other => assert_eq!(other, Decoded::Ignored),
        }
        assert_eq!(t.name_of(1), None);
        assert_eq!(t.refused_names(), 0);
    }
}
