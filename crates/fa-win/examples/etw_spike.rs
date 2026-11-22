//! Lot 5 spike (throwaway, deleted once the verdict is in `docs/decisions.md`).
//!
//! Question: on Windows 11 build 26300, does an elevated ETW session that never enables `SeDebugPrivilege` receive
//! non-zero kernel addresses in DPC/ISR routine fields, image-load bases and kernel stack walks? Also: does a private
//! "system logger" session work, or is the "NT Kernel Logger" needed?
//!
//! Run from an ELEVATED terminal: `cargo run -p fa-win --example etw_spike -- --seconds 10 [--mode auto|private|nt]`.
//! No process handle is opened, no privilege is touched.
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::multiple_unsafe_ops_per_block)]

use std::collections::BTreeMap;
use std::mem::{size_of, zeroed};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::null;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use windows_sys::core::GUID;
use windows_sys::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, OpenTraceW, ProcessTrace, StartTraceW, TraceSetInformation,
    CLASSIC_EVENT_ID, CONTROLTRACE_HANDLE, EVENT_RECORD, EVENT_TRACE_CONTROL_STOP,
    EVENT_TRACE_FLAG_DPC, EVENT_TRACE_FLAG_IMAGE_LOAD, EVENT_TRACE_FLAG_INTERRUPT,
    EVENT_TRACE_FLAG_PROCESS, EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, EVENT_TRACE_SYSTEM_LOGGER_MODE, PROCESSTRACE_HANDLE,
    PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_RAW_TIMESTAMP,
    PROCESS_TRACE_MODE_REAL_TIME, WNODE_FLAG_TRACED_GUID,
};

const KERNEL_FLOOR: u64 = 0xFFFF_8000_0000_0000;
const PERFINFO: u128 = 0xce1dbfb4_137e_4da6_87b0_3f59aa102cbc;
const IMAGE: u128 = 0x2cb15d1d_5fc1_11d2_abe1_00a0c911f518;
const STACKWALK: u128 = 0xdef2fe46_7bd6_4b80_bd94_f57fe20d0ce3;
const NT_KERNEL_GUID: u128 = 0x9e814aad_3204_11d2_9a82_006008a86939;
const PRIVATE_GUID: u128 = 0x6c1f3a52_0b7e_4d0a_9f21_46a1e5d00c05;

struct Bucket {
    name: &'static str,
    total: AtomicU64,
    zero: AtomicU64,
    kernel: AtomicU64,
    other: AtomicU64,
}

impl Bucket {
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            total: AtomicU64::new(0),
            zero: AtomicU64::new(0),
            kernel: AtomicU64::new(0),
            other: AtomicU64::new(0),
        }
    }
}

// Index = bucket id used by `classify`.
static BUCKETS: [Bucket; 8] = [
    Bucket::new("DPC (PerfInfo 66) Routine"),
    Bucket::new("ISR (PerfInfo 67) Routine"),
    Bucket::new("TimerDPC (PerfInfo 68) Routine"),
    Bucket::new("ThreadedDPC (PerfInfo 69) Routine"),
    Bucket::new("Image Load (10) ImageBase"),
    Bucket::new("Image DCStart (3) ImageBase"),
    Bucket::new("Image DCEnd (4) ImageBase"),
    Bucket::new("StackWalk (32) every stack word"),
];

static SAMPLES: Mutex<Vec<(usize, u64)>> = Mutex::new(Vec::new());
static SEEN: Mutex<BTreeMap<(u128, u8), u64>> = Mutex::new(BTreeMap::new());
static FIRST_TS: AtomicU64 = AtomicU64::new(0);

fn guid_u128(g: &GUID) -> u128 {
    (u128::from(g.data1) << 96)
        | (u128::from(g.data2) << 80)
        | (u128::from(g.data3) << 64)
        | u128::from_be_bytes([
            0, 0, 0, 0, 0, 0, 0, 0, g.data4[0], g.data4[1], g.data4[2], g.data4[3], g.data4[4],
            g.data4[5], g.data4[6], g.data4[7],
        ])
}

fn record(bucket: usize, value: u64) {
    let b = &BUCKETS[bucket];
    b.total.fetch_add(1, Ordering::Relaxed);
    if value == 0 {
        b.zero.fetch_add(1, Ordering::Relaxed);
    } else if value >= KERNEL_FLOOR {
        b.kernel.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut s) = SAMPLES.lock() {
            if s.iter().filter(|(i, _)| *i == bucket).count() < 3 {
                s.push((bucket, value));
            }
        }
    } else {
        b.other.fetch_add(1, Ordering::Relaxed);
    }
}

fn word(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset + 8)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

fn classify(provider: u128, opcode: u8, data: &[u8]) {
    match (provider, opcode) {
        (PERFINFO, 66) => word(data, 8).map_or((), |v| record(0, v)),
        (PERFINFO, 67) => word(data, 8).map_or((), |v| record(1, v)),
        (PERFINFO, 68) => word(data, 8).map_or((), |v| record(2, v)),
        (PERFINFO, 69) => word(data, 8).map_or((), |v| record(3, v)),
        (IMAGE, 10) => word(data, 0).map_or((), |v| record(4, v)),
        (IMAGE, 3) => word(data, 0).map_or((), |v| record(5, v)),
        (IMAGE, 4) => word(data, 0).map_or((), |v| record(6, v)),
        (STACKWALK, 32) => {
            // EventTimeStamp u64, StackProcess u32, StackThread u32, then one u64 per frame.
            let mut off = 16;
            while let Some(v) = word(data, off) {
                record(7, v);
                off += 8;
            }
        }
        _ => {}
    }
}

fn handle(rec: *mut EVENT_RECORD) {
    if rec.is_null() {
        return;
    }
    // SAFETY: ETW passes a valid EVENT_RECORD for the duration of the callback; null was rejected above.
    let r = unsafe { &*rec };
    let provider = guid_u128(&r.EventHeader.ProviderId);
    let opcode = r.EventHeader.EventDescriptor.Opcode;
    let data: &[u8] = if r.UserData.is_null() || r.UserDataLength == 0 {
        &[]
    } else {
        // SAFETY: UserData points to UserDataLength readable bytes owned by the record, valid during the callback.
        unsafe {
            std::slice::from_raw_parts(r.UserData.cast::<u8>(), usize::from(r.UserDataLength))
        }
    };
    let _ = FIRST_TS.compare_exchange(
        0,
        r.EventHeader.TimeStamp as u64,
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
    if let Ok(mut seen) = SEEN.lock() {
        *seen.entry((provider, opcode)).or_insert(0) += 1;
    }
    classify(provider, opcode, data);
}

unsafe extern "system" fn callback(rec: *mut EVENT_RECORD) {
    let _ = catch_unwind(AssertUnwindSafe(|| handle(rec)));
}

fn to_guid(v: u128) -> GUID {
    GUID::from_u128(v)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const PROPS_BYTES: usize = size_of::<EVENT_TRACE_PROPERTIES>() + 1024;

/// Zeroed, 8-aligned buffer holding one filled `EVENT_TRACE_PROPERTIES` followed by room for the names.
fn props(buf: &mut [u64], guid: u128, mode: u32, flags: u32) -> *mut EVENT_TRACE_PROPERTIES {
    // SAFETY: all-zero is a valid bit pattern for this plain C struct (integers, pointers, a GUID).
    let mut p: EVENT_TRACE_PROPERTIES = unsafe { zeroed() };
    p.Wnode.BufferSize = PROPS_BYTES as u32;
    p.Wnode.Guid = to_guid(guid);
    p.Wnode.ClientContext = 3; // raw TSC timestamps
    p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    p.BufferSize = 64;
    p.MinimumBuffers = 4;
    p.MaximumBuffers = 16;
    p.LogFileMode = mode;
    p.EnableFlags = flags;
    p.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
    let ptr = buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
    // SAFETY: `buf` is zeroed, 8-byte aligned and PROPS_BYTES long, which is larger than the struct written.
    unsafe { ptr.write(p) };
    ptr
}

struct Session {
    name: Vec<u16>,
    guid: u128,
    mode: u32,
    flags: u32,
    label: &'static str,
}

fn sessions(which: &str) -> Vec<Session> {
    let flags = EVENT_TRACE_FLAG_DPC
        | EVENT_TRACE_FLAG_INTERRUPT
        | EVENT_TRACE_FLAG_IMAGE_LOAD
        | EVENT_TRACE_FLAG_PROCESS;
    let private = Session {
        name: wide("FrameAutopsySpike"),
        guid: PRIVATE_GUID,
        mode: EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE,
        flags,
        label: "private system logger (own GUID)",
    };
    let nt = Session {
        name: wide("NT Kernel Logger"),
        guid: NT_KERNEL_GUID,
        mode: EVENT_TRACE_REAL_TIME_MODE,
        flags,
        label: "NT Kernel Logger",
    };
    match which {
        "private" => vec![private],
        "nt" => vec![nt],
        _ => vec![private, nt],
    }
}

fn run_session(s: &mut Session, seconds: u64) -> bool {
    // Only OUR session name is cleaned up. The NT Kernel Logger belongs to whoever runs it: never stop it blindly.
    if s.guid == PRIVATE_GUID {
        let mut buf = vec![0u64; PROPS_BYTES / 8 + 1];
        let p = props(&mut buf, s.guid, s.mode, s.flags);
        // SAFETY: `p` points into `buf`, which outlives the call; `s.name` is a NUL-terminated UTF-16 string.
        let stale = unsafe {
            ControlTraceW(
                CONTROLTRACE_HANDLE { Value: 0 },
                s.name.as_ptr(),
                p,
                EVENT_TRACE_CONTROL_STOP,
            )
        };
        println!(
            "[{}] stop of a stale session: code {stale} (4201 = none existed)",
            s.label
        );
    }

    let mut buf = vec![0u64; PROPS_BYTES / 8 + 1];
    let p = props(&mut buf, s.guid, s.mode, s.flags);
    let mut handle = CONTROLTRACE_HANDLE { Value: 0 };
    // SAFETY: `handle` and `p` are valid for the call; `s.name` is a NUL-terminated UTF-16 string.
    let rc = unsafe { StartTraceW(&mut handle, s.name.as_ptr(), p) };
    println!("[{}] StartTraceW: code {rc} (0 = ok, 5 = access denied, 183 = already exists, 87 = invalid parameter)", s.label);
    if rc != 0 {
        return false;
    }

    // Stack walk on DPC/ISR (spike only): PerfInfo types 66 DPC, 67 ISR, 68 TimerDPC, 69 ThreadedDPC.
    let perf = to_guid(PERFINFO);
    let ids: Vec<CLASSIC_EVENT_ID> = [66u8, 67, 68, 69]
        .iter()
        .map(|t| CLASSIC_EVENT_ID {
            EventGuid: perf,
            Type: *t,
            Reserved: [0; 7],
        })
        .collect();
    // SAFETY: `ids` is a valid array of CLASSIC_EVENT_ID of the stated byte length; 3 = TraceStackTracingInfo.
    let rc = unsafe {
        TraceSetInformation(
            handle,
            3,
            ids.as_ptr().cast(),
            (ids.len() * size_of::<CLASSIC_EVENT_ID>()) as u32,
        )
    };
    println!(
        "[{}] TraceSetInformation(stack tracing): code {rc} (0 = ok)",
        s.label
    );

    // SAFETY: all-zero is a valid bit pattern for EVENT_TRACE_LOGFILEW (integers, pointers, plain structs, Option fn).
    let mut log: EVENT_TRACE_LOGFILEW = unsafe { zeroed() };
    log.LoggerName = s.name.as_mut_ptr();
    log.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME
        | PROCESS_TRACE_MODE_EVENT_RECORD
        | PROCESS_TRACE_MODE_RAW_TIMESTAMP;
    log.Anonymous2.EventRecordCallback = Some(callback);
    // SAFETY: `log` is fully initialised and `s.name` outlives the trace.
    let th = unsafe { OpenTraceW(&mut log) };
    println!(
        "[{}] OpenTraceW: handle {:#x} (u64::MAX = invalid)",
        s.label, th.Value
    );
    let ok = th.Value != u64::MAX;
    let worker = ok.then(|| {
        let raw = th.Value;
        std::thread::spawn(move || {
            let h = PROCESSTRACE_HANDLE { Value: raw };
            // SAFETY: `h` is a handle returned by OpenTraceW; ProcessTrace blocks until the session stops.
            unsafe { ProcessTrace(&h, 1, null(), null()) }
        })
    });

    std::thread::sleep(Duration::from_secs(seconds));

    let mut buf = vec![0u64; PROPS_BYTES / 8 + 1];
    let p = props(&mut buf, s.guid, s.mode, s.flags);
    // SAFETY: `p` points into `buf`, which outlives the call; `s.name` is a NUL-terminated UTF-16 string.
    let rc = unsafe { ControlTraceW(handle, s.name.as_ptr(), p, EVENT_TRACE_CONTROL_STOP) };
    println!("[{}] ControlTraceW(stop): code {rc}", s.label);
    if let Some(w) = worker {
        println!("[{}] ProcessTrace returned {:?}", s.label, w.join());
        // SAFETY: `th` is the handle returned by OpenTraceW and is closed once.
        let rc = unsafe { CloseTrace(th) };
        println!(
            "[{}] CloseTrace: code {rc} (6 = already closed by the stop, harmless)",
            s.label
        );
    }
    ok
}

fn report() {
    println!("\n== Result (kernel = value >= 0xFFFF800000000000) ==");
    println!(
        "{:<36} {:>9} {:>9} {:>9} {:>9}",
        "field", "total", "zero", "kernel", "other"
    );
    for b in &BUCKETS {
        println!(
            "{:<36} {:>9} {:>9} {:>9} {:>9}",
            b.name,
            b.total.load(Ordering::Relaxed),
            b.zero.load(Ordering::Relaxed),
            b.kernel.load(Ordering::Relaxed),
            b.other.load(Ordering::Relaxed)
        );
    }
    println!(
        "first event timestamp (raw): {}",
        FIRST_TS.load(Ordering::Relaxed)
    );
    if let Ok(s) = SAMPLES.lock() {
        for (i, v) in s.iter() {
            println!("sample kernel value, {}: {v:#018x}", BUCKETS[*i].name);
        }
    }
    println!("\nEvents seen (provider guid, opcode -> count), top 20:");
    if let Ok(seen) = SEEN.lock() {
        let mut v: Vec<_> = seen.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for ((g, op), n) in v.into_iter().take(20) {
            println!("  {g:032x} op {op:>3}: {n}");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let seconds = arg("--seconds").and_then(|s| s.parse().ok()).unwrap_or(10);
    let mode = arg("--mode").unwrap_or_else(|| "auto".into());
    let mut candidates = sessions(&mode);
    let mut started = false;
    for s in &mut candidates {
        if run_session(s, seconds) {
            println!("session used: {}", s.label);
            started = true;
            break;
        }
    }
    if !started {
        println!("no session could be started (run from an elevated terminal)");
        return;
    }
    report();
}
