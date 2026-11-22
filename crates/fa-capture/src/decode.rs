//! Hand-written event layouts keyed by provider, event id (or opcode for classic kernel events) and version.
//! Every offset below comes from the TDH dump taken on the target machine (Windows 11 build 26300, 2026-11-22), except
//! the context-switch layout: TDH has no schema for it (code 1168), so only its first four fields, stable across all
//! versions of the class, are read, and only for the version seen on this build.
//!
//! No allocation, no panic: every read is bounds-checked and a short buffer gives [`Decoded::Malformed`].

use crate::record::{ColdRecord, DpcKind, HotRecord, ImageKind, Name, ProcessKind};
use crate::view::{guids, EventView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoded {
    Hot(HotRecord),
    Cold(ColdRecord),
    /// An event we do not decode (other provider, other id).
    Ignored,
    /// A known event whose version has no layout here: counted and skipped.
    UnknownVersion,
    /// A known event and version whose payload is shorter than its layout.
    Malformed,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        d.get(o..o.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        d.get(o..o.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        d.get(o..o.checked_add(8)?)?.try_into().ok()?,
    ))
}
fn ptr_at(d: &[u8], o: usize, ptr32: bool) -> Option<u64> {
    if ptr32 {
        u32_at(d, o).map(u64::from)
    } else {
        u64_at(d, o)
    }
}

/// NUL-terminated UTF-16 string starting at `o`, reduced to its file name.
fn utf16_name(d: &[u8], o: usize) -> Option<Name> {
    let units = d
        .get(o..)?
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|u| *u != 0);
    Some(Name::from_utf16_basename(units))
}

fn ansi_name(d: &[u8], o: usize) -> Option<Name> {
    Some(Name::from_ansi(
        d.get(o..)?.iter().copied().take_while(|b| *b != 0),
    ))
}

pub fn decode(v: &EventView<'_>) -> Decoded {
    let (known, version) = match (v.provider, v.id, v.opcode) {
        (guids::DXGI, 0x2a | 0x37, _) => (true, 0),
        (guids::DXGKRNL, 0xb8, _) => (true, 1),
        (guids::PERFINFO, _, 67..=69) => (true, 2),
        (guids::THREAD, _, 36) => (true, 5),
        (guids::DISKIO, _, 10 | 11) => (true, 3),
        (guids::IMAGE, _, 2 | 3 | 4 | 10) => (true, 3),
        (guids::KERNEL_PROCESS, 1, _) => (true, 4),
        (guids::KERNEL_PROCESS, 0xf | 2, _) => (true, 2),
        _ => (false, 0),
    };
    if !known {
        return Decoded::Ignored;
    }
    if v.version != version {
        return Decoded::UnknownVersion;
    }
    body(v).unwrap_or(Decoded::Malformed)
}

fn body(v: &EventView<'_>) -> Option<Decoded> {
    let d = v.data;
    let p = if v.ptr32 { 4 } else { 8 };
    let hot = |r| Some(Decoded::Hot(r));
    let cold = |r| Some(Decoded::Cold(r));
    match (v.provider, v.id, v.opcode) {
        // Present_Start / PresentMultiplaneOverlay_Start: swap chain, Flags, SyncInterval.
        (guids::DXGI, _, _) => {
            let sync = u32_at(d, p + 4)?;
            hot(HotRecord::Present {
                ts: v.timestamp,
                swap_chain: ptr_at(d, 0, v.ptr32)?,
                pid: v.pid,
                flags: u32_at(d, p)?,
                sync_interval: sync.min(255) as u8,
            })
        }
        // Present_Info: hContext u32, hWindow ptr, VidPnSourceId, FlipInterval, Flags.
        (guids::DXGKRNL, _, _) => hot(HotRecord::DxgPresent {
            ts: v.timestamp,
            window: ptr_at(d, 4, v.ptr32)?,
            pid: v.pid,
            flip_interval: u32_at(d, 8 + p)?,
            flags: u32_at(d, 12 + p)?,
        }),
        // ISR (67): InitialTime, Routine, ReturnValue u8, Vector u16. DPC (68) and TimerDPC (69): InitialTime, Routine.
        (guids::PERFINFO, _, op) => {
            let (kind, vector) = match op {
                67 => (DpcKind::Isr, u16_at(d, 17)?),
                68 => (DpcKind::Dpc, 0),
                _ => (DpcKind::TimerDpc, 0),
            };
            hot(HotRecord::Dpc {
                ts: v.timestamp,
                initial_time: u64_at(d, 0)?,
                routine: u64_at(d, 8)?,
                cpu: v.cpu,
                vector,
                kind,
            })
        }
        // CSwitch: NewThreadId, OldThreadId, NewThreadPriority i8, OldThreadPriority i8, ...
        (guids::THREAD, _, _) => hot(HotRecord::CSwitch {
            ts: v.timestamp,
            new_tid: u32_at(d, 0)?,
            old_tid: u32_at(d, 4)?,
            cpu: v.cpu,
            new_priority: *d.get(8)? as i8,
            old_priority: *d.get(9)? as i8,
        }),
        // DiskIo read (10) / write (11): DiskNumber, IrpFlags, TransferSize, Reserved, ByteOffset u64, FileObject ptr,
        // Irp ptr, HighResResponseTime u64 (at 40), IssuingThreadId (at 48). Kernel events: 64-bit layout.
        (guids::DISKIO, _, op) => hot(HotRecord::DiskIo {
            ts: v.timestamp,
            response: u64_at(d, 40)?,
            size: u32_at(d, 8)?,
            disk: u32_at(d, 0)?,
            issuing_tid: u32_at(d, 48)?,
            write: op == 11,
        }),
        // Image: ImageBase, ImageSize (pointers), ProcessId, ..., FileName UTF-16 at 56 (64-bit layout).
        (guids::IMAGE, _, op) => cold(ColdRecord::Image {
            ts: v.timestamp,
            base: u64_at(d, 0)?,
            size: u64_at(d, 8)?,
            pid: u32_at(d, 16)?,
            kind: match op {
                2 => ImageKind::Unload,
                3 => ImageKind::DcStart,
                4 => ImageKind::DcEnd,
                _ => ImageKind::Load,
            },
            name: utf16_name(d, 56)?,
        }),
        // ProcessStart (id 1) and ProcessRundown (id 0xf): ProcessID at 0, ParentProcessID at 20, a SID at 48
        // (8 + 4 * sub-authority count bytes), then ImageName as UTF-16.
        (guids::KERNEL_PROCESS, id @ (1 | 0xf), _) => {
            let sid_len = 8 + 4 * usize::from(*d.get(49)?);
            cold(ColdRecord::Process {
                ts: v.timestamp,
                pid: u32_at(d, 0)?,
                parent_pid: u32_at(d, 20)?,
                kind: if id == 1 {
                    ProcessKind::Start
                } else {
                    ProcessKind::Rundown
                },
                name: utf16_name(d, 48 + sid_len)?,
            })
        }
        // ProcessStop (id 2): ProcessID at 0, ImageName as an ANSI string at 84.
        (guids::KERNEL_PROCESS, _, _) => cold(ColdRecord::Process {
            ts: v.timestamp,
            pid: u32_at(d, 0)?,
            parent_pid: 0,
            kind: ProcessKind::Stop,
            name: ansi_name(d, 84)?,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Case<'a> = (u128, u16, u8, u8, &'a [u8], usize);

    fn view<'a>(provider: u128, id: u16, version: u8, opcode: u8, data: &'a [u8]) -> EventView<'a> {
        EventView {
            provider,
            id,
            version,
            opcode,
            timestamp: 1000,
            pid: 42,
            tid: 7,
            cpu: 3,
            ptr32: false,
            data,
        }
    }

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    // Real payload bytes copied from the TDH dump of 2026-11-22.
    const PRESENT_START: [u8; 24] = [
        0xc0, 0x67, 0x38, 0x36, 0x1e, 0x02, 0, 0, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ];
    const ISR: [u8; 20] = [
        0x97, 0xcd, 0x62, 0xd3, 0x5a, 0x18, 0, 0, 0xc0, 0x89, 0x9c, 0x31, 0x04, 0xf8, 0xff, 0xff,
        0x01, 0x71, 0, 0,
    ];
    const DPC: [u8; 16] = [
        0xbb, 0x2f, 0x63, 0xd3, 0x5a, 0x18, 0, 0, 0x40, 0x11, 0x9c, 0x31, 0x04, 0xf8, 0xff, 0xff,
    ];
    const DISK_WRITE: [u8; 52] = [
        0, 0, 0, 0, 0x43, 0, 6, 0, 0x00, 0xb0, 0x03, 0, 0, 0, 0, 0, 0x00, 0x40, 0x0b, 0xd1, 0, 0,
        0, 0, 0xa0, 0xdc, 0x80, 0x15, 0x89, 0xd7, 0xff, 0xff, 0x10, 0xb0, 0xf6, 0x98, 0x89, 0xd7,
        0xff, 0xff, 0xd9, 0x03, 0, 0, 0, 0, 0, 0, 0xab, 0x12, 0, 0,
    ];

    #[test]
    fn dxgi_present_start_from_real_bytes() {
        let r = decode(&view(guids::DXGI, 0x2a, 0, 1, &PRESENT_START));
        assert_eq!(
            r,
            Decoded::Hot(HotRecord::Present {
                ts: 1000,
                swap_chain: 0x021e_3638_67c0,
                pid: 42,
                flags: 0x100,
                sync_interval: 0
            })
        );
    }

    #[test]
    fn a_32_bit_process_uses_4_byte_pointers() {
        let mut data = vec![0xc0, 0x67, 0x38, 0x36, 0x00, 0x01, 0, 0, 1, 0, 0, 0];
        data.extend([0; 8]);
        let mut v = view(guids::DXGI, 0x2a, 0, 1, &data);
        v.ptr32 = true;
        assert_eq!(
            decode(&v),
            Decoded::Hot(HotRecord::Present {
                ts: 1000,
                swap_chain: 0x3638_67c0,
                pid: 42,
                flags: 0x100,
                sync_interval: 1
            })
        );
    }

    #[test]
    fn isr_and_dpc_from_real_bytes() {
        let r = decode(&view(guids::PERFINFO, 0, 2, 67, &ISR));
        assert_eq!(
            r,
            Decoded::Hot(HotRecord::Dpc {
                ts: 1000,
                initial_time: 0x185a_d362_cd97,
                routine: 0xffff_f804_319c_89c0,
                cpu: 3,
                vector: 0x71,
                kind: DpcKind::Isr
            })
        );
        let r = decode(&view(guids::PERFINFO, 0, 2, 68, &DPC));
        assert_eq!(
            r,
            Decoded::Hot(HotRecord::Dpc {
                ts: 1000,
                initial_time: 0x185a_d363_2fbb,
                routine: 0xffff_f804_319c_1140,
                cpu: 3,
                vector: 0,
                kind: DpcKind::Dpc
            })
        );
        assert!(matches!(
            decode(&view(guids::PERFINFO, 0, 2, 69, &DPC)),
            Decoded::Hot(HotRecord::Dpc {
                kind: DpcKind::TimerDpc,
                ..
            })
        ));
    }

    #[test]
    fn disk_write_from_real_bytes() {
        let r = decode(&view(guids::DISKIO, 0, 3, 11, &DISK_WRITE));
        assert_eq!(
            r,
            Decoded::Hot(HotRecord::DiskIo {
                ts: 1000,
                response: 0x3d9,
                size: 0x3b000,
                disk: 0,
                issuing_tid: 0x12ab,
                write: true
            })
        );
    }

    #[test]
    fn context_switch_reads_its_first_fields() {
        let mut data = vec![0u8; 28];
        data[0..4].copy_from_slice(&0x1234u32.to_le_bytes());
        data[4..8].copy_from_slice(&0x5678u32.to_le_bytes());
        data[8] = 10;
        data[9] = 0xF8; // -8
        assert_eq!(
            decode(&view(guids::THREAD, 0, 5, 36, &data)),
            Decoded::Hot(HotRecord::CSwitch {
                ts: 1000,
                new_tid: 0x1234,
                old_tid: 0x5678,
                cpu: 3,
                new_priority: 10,
                old_priority: -8
            })
        );
    }

    #[test]
    fn dxgkrnl_present_info() {
        let mut data = vec![0u8; 44];
        data[4..12].copy_from_slice(&0x4000_0000_3800u64.to_le_bytes());
        data[16..20].copy_from_slice(&2u32.to_le_bytes());
        data[20..24].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(
            decode(&view(guids::DXGKRNL, 0xb8, 1, 0, &data)),
            Decoded::Hot(HotRecord::DxgPresent {
                ts: 1000,
                window: 0x4000_0000_3800,
                pid: 42,
                flip_interval: 2,
                flags: 9
            })
        );
    }

    #[test]
    fn kernel_image_name_is_reduced_to_its_file_name() {
        let mut data = vec![0u8; 56];
        data[0..8].copy_from_slice(&0xffff_f804_9f40_0000u64.to_le_bytes());
        data[8..16].copy_from_slice(&0x0145_0000u64.to_le_bytes());
        data.extend(utf16("\\SystemRoot\\system32\\ntoskrnl.exe"));
        match decode(&view(guids::IMAGE, 0, 3, 3, &data)) {
            Decoded::Cold(ColdRecord::Image {
                base,
                size,
                kind: ImageKind::DcStart,
                name,
                ..
            }) => {
                assert_eq!((base, size), (0xffff_f804_9f40_0000, 0x0145_0000));
                assert_eq!(name.complete(), Some("ntoskrnl.exe"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn process_names_come_from_kernel_process_events() {
        let mut start = vec![0u8; 48];
        start[0..4].copy_from_slice(&0x6228u32.to_le_bytes());
        start[20..24].copy_from_slice(&0x0c60u32.to_le_bytes());
        start.extend([1, 1, 0, 0, 0, 0, 0, 0x10, 0x00, 0x30, 0, 0]); // SID S-1-16-12288, 12 bytes
        start.extend(utf16("VALORANT-Win64-Shipping.exe"));
        for (id, version, kind) in [(1, 4, ProcessKind::Start), (0xf, 2, ProcessKind::Rundown)] {
            match decode(&view(guids::KERNEL_PROCESS, id, version, 0, &start)) {
                Decoded::Cold(ColdRecord::Process {
                    pid: 0x6228,
                    parent_pid: 0x0c60,
                    kind: k,
                    name,
                    ..
                }) => {
                    assert_eq!(k, kind);
                    assert_eq!(name.complete(), Some("VALORANT-Win64-Shipping.exe"));
                }
                other => panic!("{other:?}"),
            }
        }
        let mut stop = vec![0u8; 84];
        stop[0..4].copy_from_slice(&0x6228u32.to_le_bytes());
        stop.extend(b"chrome.exe\0");
        match decode(&view(guids::KERNEL_PROCESS, 2, 2, 0, &stop)) {
            Decoded::Cold(ColdRecord::Process {
                kind: ProcessKind::Stop,
                name,
                ..
            }) => assert_eq!(name.complete(), Some("chrome.exe")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_classic_process_event_is_never_a_name_source() {
        // Classic kernel process events truncate the image name to 15 bytes; we do not decode them at all.
        let mut data = vec![0u8; 64];
        data.extend(b"VALORANT-Win64-\0");
        assert_eq!(
            decode(&view(guids::NT_PROCESS, 0, 4, 1, &data)),
            Decoded::Ignored
        );
    }

    #[test]
    fn a_name_that_does_not_fit_is_marked_truncated_and_never_complete() {
        let long = "x".repeat(Name::CAP + 10);
        let n = Name::from_utf16_basename(long.encode_utf16());
        assert!(n.is_truncated());
        assert_eq!(n.complete(), None);
        assert_eq!(
            Name::from_utf16_basename("a.exe".encode_utf16()).complete(),
            Some("a.exe")
        );
        assert_eq!(Name::empty().complete(), None);
        // An unpaired surrogate does not panic.
        assert_eq!(
            Name::from_utf16_basename([0xD800u16, u16::from(b'a')].into_iter()).as_str(),
            "\u{fffd}a"
        );
    }

    #[test]
    fn an_unknown_version_is_reported_not_decoded() {
        assert_eq!(
            decode(&view(guids::DXGI, 0x2a, 1, 1, &PRESENT_START)),
            Decoded::UnknownVersion
        );
        assert_eq!(
            decode(&view(guids::THREAD, 0, 4, 36, &[0; 28])),
            Decoded::UnknownVersion
        );
        assert_eq!(
            decode(&view(guids::PERFINFO, 0, 3, 67, &ISR)),
            Decoded::UnknownVersion
        );
    }

    #[test]
    fn unlisted_events_are_ignored() {
        assert_eq!(
            decode(&view(guids::DXGI, 0x2b, 0, 2, &[0; 4])),
            Decoded::Ignored
        );
        assert_eq!(decode(&view(0x1234, 1, 0, 0, &[1, 2, 3])), Decoded::Ignored);
        assert_eq!(
            decode(&view(guids::KERNEL_PROCESS, 7, 0, 0, &[0; 10])),
            Decoded::Ignored
        );
    }

    #[test]
    fn every_truncated_prefix_is_malformed_or_valid_never_a_panic() {
        let cases: [Case; 5] = [
            (guids::DXGI, 0x2a, 0, 1, &PRESENT_START, 16),
            (guids::PERFINFO, 0, 2, 67, &ISR, 19),
            (guids::PERFINFO, 0, 2, 68, &DPC, 16),
            (guids::DISKIO, 0, 3, 11, &DISK_WRITE, 52),
            (guids::DXGI, 0x37, 0, 1, &PRESENT_START, 16),
        ];
        for (provider, id, version, opcode, full, needed) in cases {
            for len in 0..=full.len() {
                let r = decode(&view(provider, id, version, opcode, &full[..len]));
                if len < needed {
                    assert_eq!(r, Decoded::Malformed, "{provider:x} len {len}");
                } else {
                    assert!(matches!(r, Decoded::Hot(_)), "{provider:x} len {len}");
                }
            }
        }
    }

    #[test]
    fn random_payloads_never_panic() {
        let events = [
            (guids::DXGI, 0x2a, 0, 1),
            (guids::DXGKRNL, 0xb8, 1, 0),
            (guids::PERFINFO, 0, 2, 67),
            (guids::THREAD, 0, 5, 36),
            (guids::DISKIO, 0, 3, 10),
            (guids::IMAGE, 0, 3, 3),
            (guids::KERNEL_PROCESS, 1, 4, 0),
            (guids::KERNEL_PROCESS, 0xf, 2, 0),
            (guids::KERNEL_PROCESS, 2, 2, 0),
        ];
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..20_000 {
            let (provider, id, version, opcode) = events[(next() % events.len() as u64) as usize];
            let data: Vec<u8> = (0..next() % 300).map(|_| next() as u8).collect();
            let mut v = view(provider, id, version, opcode, &data);
            v.ptr32 = next() % 2 == 0;
            let _ = decode(&v);
        }
    }
}
