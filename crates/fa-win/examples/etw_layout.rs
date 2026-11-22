//! Developer tool (lot 5, task 3.3): dumps the TDH layout of the events FrameAutopsy decodes, as seen on THIS machine.
//! It may allocate freely: it is never on the hot path. The decoders of `fa-capture` are written from its output.
//!
//! Run from an ELEVATED terminal: `cargo run -p fa-win --example etw_layout -- --seconds 15`.
//! Open a browser video or game-like workload meanwhile so present events flow.
//! No process handle is opened and no privilege is touched.
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::multiple_unsafe_ops_per_block)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::mem::{offset_of, size_of, zeroed};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::null;
use std::sync::Mutex;
use std::time::Duration;

use windows_sys::core::GUID;
use windows_sys::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW,
    TdhGetEventInformation, CONTROLTRACE_HANDLE, EVENT_CONTROL_CODE_CAPTURE_STATE,
    EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_HEADER_FLAG_32_BIT_HEADER, EVENT_PROPERTY_INFO,
    EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FLAG_CSWITCH, EVENT_TRACE_FLAG_DISK_IO,
    EVENT_TRACE_FLAG_DPC, EVENT_TRACE_FLAG_IMAGE_LOAD, EVENT_TRACE_FLAG_INTERRUPT,
    EVENT_TRACE_FLAG_MEMORY_HARD_FAULTS, EVENT_TRACE_FLAG_PROCESS, EVENT_TRACE_FLAG_THREAD,
    EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE,
    EVENT_TRACE_SYSTEM_LOGGER_MODE, PROCESSTRACE_HANDLE, PROCESS_TRACE_MODE_EVENT_RECORD,
    PROCESS_TRACE_MODE_RAW_TIMESTAMP, PROCESS_TRACE_MODE_REAL_TIME, TRACE_EVENT_INFO,
    WNODE_FLAG_TRACED_GUID,
};

const PERFINFO: u128 = 0xce1dbfb4_137e_4da6_87b0_3f59aa102cbc;
const IMAGE: u128 = 0x2cb15d1d_5fc1_11d2_abe1_00a0c911f518;
const THREAD: u128 = 0x3d6fa8d1_fe05_11d0_9dda_00c04fd7ba7c;
const PAGEFAULT: u128 = 0x3d6fa8d3_fe05_11d0_9dda_00c04fd7ba7c;
const DISKIO: u128 = 0x3d6fa8d4_fe05_11d0_9dda_00c04fd7ba7c;
const DXGI: u128 = 0xca11c036_0102_4a2d_a6ad_f03cfed5d3c9;
const D3D9: u128 = 0x783aca0a_790e_4d7f_8451_aa850511c6b9;
const DXGKRNL: u128 = 0x802ec45a_1e99_4b83_9920_87c98277ba9d;
const KERNEL_PROCESS: u128 = 0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716;
const SESSION_GUID: u128 = 0x6c1f3a52_0b7e_4d0a_9f21_46a1e5d00c06;

// (provider, MatchAny keyword) from PresentMon e580ac87 (docs: design notes of the etw-capture change).
const MANIFEST_PROVIDERS: [(u128, u64, &str); 4] = [
    (DXGI, 0x8000_0000_0000_0002, "Microsoft-Windows-DXGI"),
    (D3D9, u64::MAX, "Microsoft-Windows-D3D9"),
    (DXGKRNL, 0x4000_0000_0800_0001, "Microsoft-Windows-DxgKrnl"),
    (KERNEL_PROCESS, u64::MAX, "Microsoft-Windows-Kernel-Process"),
];

type Key = (u128, u16, u8, u8); // provider or class guid, event id, version, opcode

#[derive(Default)]
struct Seen {
    count: u64,
    layout: Option<String>,
}

static EVENTS: Mutex<BTreeMap<Key, Seen>> = Mutex::new(BTreeMap::new());

fn guid_u128(g: &GUID) -> u128 {
    (u128::from(g.data1) << 96)
        | (u128::from(g.data2) << 80)
        | (u128::from(g.data3) << 64)
        | u128::from_be_bytes([
            0, 0, 0, 0, 0, 0, 0, 0, g.data4[0], g.data4[1], g.data4[2], g.data4[3], g.data4[4],
            g.data4[5], g.data4[6], g.data4[7],
        ])
}

fn wanted((g, id, _ver, op): Key) -> bool {
    match g {
        PERFINFO => (66..=69).contains(&op),
        IMAGE => matches!(op, 2 | 3 | 4 | 10),
        THREAD => op == 36,
        PAGEFAULT | DISKIO => true,
        DXGI | D3D9 | KERNEL_PROCESS => true,
        DXGKRNL => matches!(
            id,
            0xab | 0xac | 0xa8 | 0x74 | 0x11 | 0xb8 | 0xa6 | 0xb2 | 0xb4
        ),
        _ => false,
    }
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    b.get(off..off + 4)
        .map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap_or([0; 4])))
}
fn u16_at(b: &[u8], off: usize) -> u16 {
    b.get(off..off + 2)
        .map_or(0, |s| u16::from_le_bytes(s.try_into().unwrap_or([0; 2])))
}

/// UTF-16 NUL-terminated string at byte offset `off` (0 = absent).
fn wstr(b: &[u8], off: usize) -> String {
    if off == 0 {
        return String::new();
    }
    let units: Vec<u16> = b
        .get(off..)
        .unwrap_or(&[])
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|u| *u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Fixed size in bytes of a TDH in-type, or None when variable.
fn in_size(in_type: u16, pointer: usize) -> Option<usize> {
    match in_type {
        3 | 4 => Some(1),
        5 | 6 => Some(2),
        7 | 8 | 11 | 13 | 20 => Some(4),
        9 | 10 | 12 | 17 | 21 => Some(8),
        15 | 18 => Some(16),
        16 | 308 => Some(pointer),
        _ => None,
    }
}

fn in_name(t: u16) -> &'static str {
    match t {
        1 => "UNICODESTRING",
        2 => "ANSISTRING",
        3 => "INT8",
        4 => "UINT8",
        5 => "INT16",
        6 => "UINT16",
        7 => "INT32",
        8 => "UINT32",
        9 => "INT64",
        10 => "UINT64",
        11 => "FLOAT",
        12 => "DOUBLE",
        13 => "BOOLEAN(4)",
        14 => "BINARY",
        15 => "GUID",
        16 => "POINTER",
        17 => "FILETIME",
        18 => "SYSTEMTIME",
        19 => "SID",
        20 => "HEXINT32",
        21 => "HEXINT64",
        308 => "SIZET",
        _ => "OTHER",
    }
}

/// Parses a TRACE_EVENT_INFO buffer with offset_of!, in safe code.
fn parse_layout(b: &[u8], pointer: usize) -> String {
    let mut out = String::new();
    let provider = wstr(
        b,
        u32_at(b, offset_of!(TRACE_EVENT_INFO, ProviderNameOffset)) as usize,
    );
    let task = wstr(
        b,
        u32_at(b, offset_of!(TRACE_EVENT_INFO, TaskNameOffset)) as usize,
    );
    let opcode = wstr(
        b,
        u32_at(b, offset_of!(TRACE_EVENT_INFO, OpcodeNameOffset)) as usize,
    );
    let _ = writeln!(
        out,
        "    provider \"{provider}\" task \"{task}\" opcode \"{opcode}\""
    );
    let count = u32_at(b, offset_of!(TRACE_EVENT_INFO, PropertyCount)) as usize;
    let top = u32_at(b, offset_of!(TRACE_EVENT_INFO, TopLevelPropertyCount)) as usize;
    let first = offset_of!(TRACE_EVENT_INFO, EventPropertyInfoArray);
    let stride = size_of::<EVENT_PROPERTY_INFO>();
    let mut offset = Some(0usize);
    for i in 0..top.min(count).min(64) {
        let p = first + i * stride;
        let flags = u32_at(b, p + offset_of!(EVENT_PROPERTY_INFO, Flags));
        let name = wstr(
            b,
            u32_at(b, p + offset_of!(EVENT_PROPERTY_INFO, NameOffset)) as usize,
        );
        let in_type = u16_at(b, p + offset_of!(EVENT_PROPERTY_INFO, Anonymous1));
        let arr = u16_at(b, p + offset_of!(EVENT_PROPERTY_INFO, Anonymous2));
        let len = u16_at(b, p + offset_of!(EVENT_PROPERTY_INFO, Anonymous3));
        let at = offset.map_or("?".to_string(), |o| o.to_string());
        let _ = writeln!(
            out,
            "      +{at:>3}  {name:<28} {:<14} flags {flags:#x} count {arr} length {len}",
            in_name(in_type)
        );
        // flags: 1 struct, 2 param length, 4 param count, 16 fixed length, 32 fixed count (windows-sys PROPERTY_FLAGS).
        offset = match (offset, in_size(in_type, pointer)) {
            (Some(o), Some(s))
                if flags & 0x1 == 0 && flags & 0x6 == 0 && (arr <= 1 || flags & 0x20 != 0) =>
            {
                Some(o + s * usize::from(if flags & 0x20 != 0 { arr.max(1) } else { 1 }))
            }
            _ => None,
        };
    }
    out
}

fn describe(rec: &EVENT_RECORD) -> String {
    let pointer = if rec.EventHeader.Flags & (EVENT_HEADER_FLAG_32_BIT_HEADER as u16) != 0 {
        4
    } else {
        8
    };
    let mut size = 0u32;
    // SAFETY: a null buffer with size 0 is the documented way to ask TDH for the required size; `rec` is a valid record.
    let rc = unsafe { TdhGetEventInformation(rec, 0, null(), std::ptr::null_mut(), &mut size) };
    let mut text = format!(
        "    userdata {} bytes, header pointer size {pointer}\n",
        rec.UserDataLength
    );
    if rc != 122 || size == 0 {
        let _ = writeln!(text, "    TdhGetEventInformation size query: code {rc}");
        return text;
    }
    let mut buf = vec![0u64; size as usize / 8 + 1];
    // SAFETY: `buf` is 8-byte aligned and at least `size` bytes, as TDH requested; `rec` is a valid record.
    let rc = unsafe {
        TdhGetEventInformation(
            rec,
            0,
            null(),
            buf.as_mut_ptr().cast::<TRACE_EVENT_INFO>(),
            &mut size,
        )
    };
    if rc != 0 {
        let _ = writeln!(text, "    TdhGetEventInformation: code {rc}");
        return text;
    }
    let bytes_len = (size as usize).min(buf.len() * 8);
    // SAFETY: `buf` owns at least `bytes_len` initialised bytes (zero-filled, then written by TDH).
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), bytes_len) };
    text.push_str(&parse_layout(bytes, pointer));
    if !rec.UserData.is_null() && rec.UserDataLength > 0 {
        let n = usize::from(rec.UserDataLength).min(48);
        // SAFETY: UserData points to UserDataLength readable bytes valid during the callback; n <= that length.
        let data = unsafe { std::slice::from_raw_parts(rec.UserData.cast::<u8>(), n) };
        let hex: Vec<String> = data.iter().map(|b| format!("{b:02x}")).collect();
        let _ = writeln!(text, "    first bytes: {}", hex.join(" "));
    }
    text
}

fn handle(rec: *mut EVENT_RECORD) {
    if rec.is_null() {
        return;
    }
    // SAFETY: ETW passes a valid EVENT_RECORD for the duration of the callback; null was rejected above.
    let r = unsafe { &*rec };
    let d = &r.EventHeader.EventDescriptor;
    let key: Key = (
        guid_u128(&r.EventHeader.ProviderId),
        d.Id,
        d.Version,
        d.Opcode,
    );
    let Ok(mut map) = EVENTS.lock() else { return };
    let entry = map.entry(key).or_default();
    entry.count += 1;
    if entry.layout.is_none() && wanted(key) {
        entry.layout = Some(describe(r));
    }
}

unsafe extern "system" fn callback(rec: *mut EVENT_RECORD) {
    let _ = catch_unwind(AssertUnwindSafe(|| handle(rec)));
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const PROPS_BYTES: usize = size_of::<EVENT_TRACE_PROPERTIES>() + 1024;

fn props(buf: &mut [u64], flags: u32) -> *mut EVENT_TRACE_PROPERTIES {
    // SAFETY: all-zero is a valid bit pattern for this plain C struct.
    let mut p: EVENT_TRACE_PROPERTIES = unsafe { zeroed() };
    p.Wnode.BufferSize = PROPS_BYTES as u32;
    p.Wnode.Guid = GUID::from_u128(SESSION_GUID);
    p.Wnode.ClientContext = 3;
    p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    p.BufferSize = 128;
    p.MinimumBuffers = 8;
    p.MaximumBuffers = 32;
    p.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE;
    p.EnableFlags = flags;
    p.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
    let ptr = buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
    // SAFETY: `buf` is zeroed, 8-byte aligned and PROPS_BYTES long, larger than the struct written.
    unsafe { ptr.write(p) };
    ptr
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seconds = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let flags = EVENT_TRACE_FLAG_CSWITCH
        | EVENT_TRACE_FLAG_DPC
        | EVENT_TRACE_FLAG_INTERRUPT
        | EVENT_TRACE_FLAG_IMAGE_LOAD
        | EVENT_TRACE_FLAG_PROCESS
        | EVENT_TRACE_FLAG_THREAD
        | EVENT_TRACE_FLAG_DISK_IO
        | EVENT_TRACE_FLAG_MEMORY_HARD_FAULTS;
    let mut name = wide("FrameAutopsyLayout");

    let mut buf = vec![0u64; PROPS_BYTES / 8 + 1];
    let p = props(&mut buf, flags);
    // SAFETY: `p` points into `buf`, which outlives the call; `name` is a NUL-terminated UTF-16 string.
    let stale = unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE { Value: 0 },
            name.as_ptr(),
            p,
            EVENT_TRACE_CONTROL_STOP,
        )
    };
    println!("stale session stop: code {stale} (4201 = none existed)");

    let mut buf = vec![0u64; PROPS_BYTES / 8 + 1];
    let p = props(&mut buf, flags);
    let mut handle = CONTROLTRACE_HANDLE { Value: 0 };
    // SAFETY: `handle` and `p` are valid for the call; `name` is a NUL-terminated UTF-16 string.
    let rc = unsafe { StartTraceW(&mut handle, name.as_ptr(), p) };
    println!("StartTraceW: code {rc} (0 = ok, 5 = access denied)");
    if rc != 0 {
        return;
    }

    for (guid, keyword, label) in MANIFEST_PROVIDERS {
        let g = GUID::from_u128(guid);
        // SAFETY: `handle` is the live session handle; `g` is a valid GUID; no enable parameters are passed.
        let rc = unsafe {
            EnableTraceEx2(
                handle,
                &g,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                5,
                keyword,
                0,
                0,
                null(),
            )
        };
        println!("EnableTraceEx2 {label}: code {rc} (0 = ok)");
    }
    let g = GUID::from_u128(KERNEL_PROCESS);
    // SAFETY: as above; CAPTURE_STATE asks the provider for its rundown (ProcessRundown events).
    let rc = unsafe {
        EnableTraceEx2(
            handle,
            &g,
            EVENT_CONTROL_CODE_CAPTURE_STATE,
            5,
            u64::MAX,
            0,
            0,
            null(),
        )
    };
    println!("EnableTraceEx2 Kernel-Process rundown: code {rc} (0 = ok)");

    // SAFETY: all-zero is a valid bit pattern for EVENT_TRACE_LOGFILEW (integers, pointers, plain structs, Option fn).
    let mut log: EVENT_TRACE_LOGFILEW = unsafe { zeroed() };
    log.LoggerName = name.as_mut_ptr();
    log.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME
        | PROCESS_TRACE_MODE_EVENT_RECORD
        | PROCESS_TRACE_MODE_RAW_TIMESTAMP;
    log.Anonymous2.EventRecordCallback = Some(callback);
    // SAFETY: `log` is fully initialised and `name` outlives the trace.
    let th = unsafe { OpenTraceW(&mut log) };
    println!("OpenTraceW: handle {:#x} (u64::MAX = invalid)", th.Value);
    let worker = (th.Value != u64::MAX).then(|| {
        let raw = th.Value;
        std::thread::spawn(move || {
            let h = PROCESSTRACE_HANDLE { Value: raw };
            // SAFETY: `h` is a handle returned by OpenTraceW; ProcessTrace blocks until the session stops.
            unsafe { ProcessTrace(&h, 1, null(), null()) }
        })
    });

    std::thread::sleep(Duration::from_secs(seconds));

    let mut buf = vec![0u64; PROPS_BYTES / 8 + 1];
    let p = props(&mut buf, flags);
    // SAFETY: `p` points into `buf`, which outlives the call; `name` is a NUL-terminated UTF-16 string.
    let rc = unsafe { ControlTraceW(handle, name.as_ptr(), p, EVENT_TRACE_CONTROL_STOP) };
    println!("ControlTraceW(stop): code {rc}");
    if let Some(w) = worker {
        println!("ProcessTrace returned {:?}", w.join());
        // SAFETY: `th` is the handle returned by OpenTraceW and is closed once.
        let _ = unsafe { CloseTrace(th) };
    }

    let Ok(map) = EVENTS.lock() else { return };
    println!("\n== Layouts (provider/class guid, id, version, opcode; count) ==");
    for ((g, id, ver, op), seen) in map.iter() {
        if let Some(layout) = &seen.layout {
            println!(
                "{g:032x} id {id:#06x} ver {ver} op {op}  x{}\n{layout}",
                seen.count
            );
        }
    }
    println!("== Every event kind seen (top 40 by count) ==");
    let mut all: Vec<_> = map.iter().collect();
    all.sort_by(|a, b| b.1.count.cmp(&a.1.count));
    for ((g, id, ver, op), seen) in all.into_iter().take(40) {
        println!(
            "  {g:032x} id {id:#06x} ver {ver} op {op:>3}: {}",
            seen.count
        );
    }
}
