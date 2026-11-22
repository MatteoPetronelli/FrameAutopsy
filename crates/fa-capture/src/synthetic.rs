//! A synthetic event stream for tests and benches: a fixed mix of real-looking payloads, built once (this allocates),
//! then replayed without allocating. The mix follows what the target machine produced in 20 s of normal use.

use crate::view::{guids, EventView};

struct Template {
    provider: u128,
    id: u16,
    version: u8,
    opcode: u8,
    data: Vec<u8>,
}

pub struct Synthetic {
    templates: Vec<Template>,
    /// Index into `templates` for each position of the repeating pattern.
    pattern: Vec<usize>,
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

impl Default for Synthetic {
    fn default() -> Self {
        Self::new()
    }
}

impl Synthetic {
    pub fn new() -> Self {
        let mut image = vec![0u8; 56];
        image[0..8].copy_from_slice(&0xffff_f804_9f40_0000u64.to_le_bytes());
        image.extend(utf16(r"\SystemRoot\system32\ntoskrnl.exe"));
        let mut process = vec![0u8; 48];
        process.extend([1, 1, 0, 0, 0, 0, 0, 0x10, 0x00, 0x30, 0, 0]);
        process.extend(utf16("chrome.exe"));
        let t = |provider, id, version, opcode, data: Vec<u8>| Template {
            provider,
            id,
            version,
            opcode,
            data,
        };
        let templates = vec![
            t(guids::THREAD, 0, 5, 36, vec![7; 28]), // 0: context switch
            t(guids::PERFINFO, 0, 2, 68, vec![1; 16]), // 1: DPC
            t(guids::PERFINFO, 0, 2, 67, vec![2; 20]), // 2: ISR
            t(guids::DXGI, 0x2a, 0, 1, vec![3; 24]), // 3: present start
            t(guids::DISKIO, 0, 3, 11, vec![4; 52]), // 4: disk write
            t(guids::DXGKRNL, 0xb8, 1, 0, vec![5; 44]), // 5: dxgkrnl present
            t(guids::IMAGE, 0, 3, 10, image),        // 6: image load
            t(guids::KERNEL_PROCESS, 1, 4, 0, process), // 7: process start
            t(guids::KERNEL_PROCESS, 7, 0, 0, vec![0; 10]), // 8: not decoded
            t(guids::DXGI, 0x2a, 9, 1, vec![3; 24]), // 9: unknown version
            t(guids::DXGI, 0x2a, 0, 1, vec![3; 5]),  // 10: malformed
        ];
        // 20 patterns: mostly context switches, a few of everything else.
        let pattern = vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        Self { templates, pattern }
    }

    /// Event number `i` of the stream; the timestamp is `i`.
    pub fn event(&self, i: u64) -> EventView<'_> {
        let t = &self.templates[self.pattern[(i % self.pattern.len() as u64) as usize]];
        EventView {
            provider: t.provider,
            id: t.id,
            version: t.version,
            opcode: t.opcode,
            timestamp: i,
            pid: 1000 + (i % 7) as u32,
            tid: 1,
            cpu: (i % 12) as u16,
            ptr32: false,
            data: &t.data,
        }
    }
}
