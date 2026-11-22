//! WIN-01: the ETW real-time session and its consumer.
//!
//! One private "system logger" session named `FrameAutopsy` receives the kernel events (context switch, DPC, ISR, image
//! loads, disk, hard faults, process and thread) and the manifest providers we need (DXGI, DxgKrnl, Kernel-Process).
//! Timestamps are raw: TSC ticks when the TSC is invariant (`ClientContext = 3`), else QPC ticks (`ClientContext = 1`).
//!
//! The callback hands the handler a borrowed [`EventView`] that is valid only during the call: the handler's type
//! (`for<'a> FnMut(&EventView<'a>)`) makes it impossible to keep one. No process handle is ever opened here.
//!
//! ```
//! use fa_win::etw::EventView;
//! // A handler may copy plain values out of the view.
//! let mut stamps: Vec<u64> = Vec::new();
//! let mut handler = move |v: &EventView<'_>| stamps.push(v.timestamp);
//! fn takes(_: impl FnMut(&EventView<'_>) + Send + 'static) {}
//! takes(handler);
//! ```
//!
//! ```compile_fail
//! use fa_win::etw::EventView;
//! // ...but not the view itself: it does not outlive the callback.
//! let mut kept: Vec<EventView<'static>> = Vec::new();
//! let handler = move |v: &EventView<'_>| kept.push(*v);
//! fn takes(_: impl FnMut(&EventView<'_>) + Send + 'static) {}
//! takes(handler);
//! ```

use std::ffi::c_void;
use std::io;
use std::mem::{size_of, zeroed};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::null;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use fa_core::clock::Timebase;
use windows_sys::core::GUID;
use windows_sys::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW,
    CONTROLTRACE_HANDLE, ENABLE_TRACE_PARAMETERS, ENABLE_TRACE_PARAMETERS_VERSION_2,
    EVENT_CONTROL_CODE_CAPTURE_STATE, EVENT_CONTROL_CODE_ENABLE_PROVIDER,
    EVENT_ENABLE_PROPERTY_IGNORE_KEYWORD_0, EVENT_FILTER_DESCRIPTOR, EVENT_FILTER_TYPE_EVENT_ID,
    EVENT_HEADER_FLAG_32_BIT_HEADER, EVENT_RECORD, EVENT_TRACE_CONTROL_STOP,
    EVENT_TRACE_FLAG_CSWITCH, EVENT_TRACE_FLAG_DISK_IO, EVENT_TRACE_FLAG_DPC,
    EVENT_TRACE_FLAG_IMAGE_LOAD, EVENT_TRACE_FLAG_INTERRUPT, EVENT_TRACE_FLAG_MEMORY_HARD_FAULTS,
    EVENT_TRACE_FLAG_PROCESS, EVENT_TRACE_FLAG_THREAD, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, EVENT_TRACE_SYSTEM_LOGGER_MODE,
    PROCESSTRACE_HANDLE, PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_RAW_TIMESTAMP,
    PROCESS_TRACE_MODE_REAL_TIME, WNODE_FLAG_TRACED_GUID,
};

/// Name of the session; a session of this name left behind by a crash is stopped at startup.
pub const SESSION_NAME: &str = "FrameAutopsy";

const SESSION_GUID: u128 = 0x6c1f3a52_0b7e_4d0a_9f21_46a1e5d00c01;
const DXGI: u128 = 0xca11c036_0102_4a2d_a6ad_f03cfed5d3c9;
const DXGKRNL: u128 = 0x802ec45a_1e99_4b83_9920_87c98277ba9d;
const KERNEL_PROCESS: u128 = 0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716;

/// DxgKrnl events we ask for (event-id filter): `Present_Info`. Extend when displayed time is implemented.
const DXGKRNL_EVENT_IDS: [u16; 1] = [0xb8];

const KERNEL_FLAGS: u32 = EVENT_TRACE_FLAG_CSWITCH
    | EVENT_TRACE_FLAG_DPC
    | EVENT_TRACE_FLAG_INTERRUPT
    | EVENT_TRACE_FLAG_IMAGE_LOAD
    | EVENT_TRACE_FLAG_PROCESS
    | EVENT_TRACE_FLAG_THREAD
    | EVENT_TRACE_FLAG_DISK_IO
    | EVENT_TRACE_FLAG_MEMORY_HARD_FAULTS;

/// One event, borrowed from the ETW buffer: valid only during the callback. Copy out what you need.
#[derive(Clone, Copy, Debug)]
pub struct EventView<'a> {
    /// Provider or classic class GUID as a `u128`: `data1:data2:data3:data4` read as hex.
    pub provider: u128,
    pub id: u16,
    pub version: u8,
    pub opcode: u8,
    /// Raw ticks (see [`Session::timebase`]).
    pub timestamp: u64,
    pub pid: u32,
    pub tid: u32,
    pub cpu: u16,
    /// Written by a 32-bit process: pointers in `data` are 4 bytes.
    pub ptr32: bool,
    pub data: &'a [u8],
}

/// Counters shared between the callback and the owner.
#[derive(Debug, Default)]
struct Counters {
    events: AtomicU64,
    panics: AtomicU64,
}

type Handler = Box<dyn FnMut(&EventView<'_>) + Send>;

/// Passed to the callback through `EVENT_TRACE_LOGFILEW.Context`; touched only by the ProcessTrace thread.
struct Ctx {
    handler: Handler,
    counters: Arc<Counters>,
}

fn guid_u128(g: &GUID) -> u128 {
    u128::from(g.data1) << 96
        | u128::from(g.data2) << 80
        | u128::from(g.data3) << 64
        | u128::from_be_bytes([
            0, 0, 0, 0, 0, 0, 0, 0, g.data4[0], g.data4[1], g.data4[2], g.data4[3], g.data4[4],
            g.data4[5], g.data4[6], g.data4[7],
        ])
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn win_err(what: &str, code: u32) -> io::Error {
    io::Error::other(format!("{what}: Windows error {code}"))
}

/// The callback body: builds the view, calls the handler, never lets a panic escape.
fn dispatch(rec: &EVENT_RECORD) {
    let ctx = rec.UserContext.cast::<Ctx>();
    if ctx.is_null() {
        return;
    }
    // SAFETY: `UserContext` is the `Ctx` pointer given to OpenTraceW; the owner frees it only after ProcessTrace has
    // returned, and callbacks of one session are serial, so this is the only live reference.
    let ctx = unsafe { &mut *ctx };
    let data: &[u8] = if rec.UserData.is_null() || rec.UserDataLength == 0 {
        &[]
    } else {
        // SAFETY: `UserData` points to `UserDataLength` readable bytes that ETW keeps valid during the callback.
        unsafe {
            std::slice::from_raw_parts(rec.UserData.cast::<u8>(), usize::from(rec.UserDataLength))
        }
    };
    // SAFETY: reading the `ProcessorIndex` view of a plain-integer union is always valid.
    let cpu = unsafe { rec.BufferContext.Anonymous.ProcessorIndex };
    let h = &rec.EventHeader;
    let view = EventView {
        provider: guid_u128(&h.ProviderId),
        id: h.EventDescriptor.Id,
        version: h.EventDescriptor.Version,
        opcode: h.EventDescriptor.Opcode,
        timestamp: h.TimeStamp as u64,
        pid: h.ProcessId,
        tid: h.ThreadId,
        cpu,
        ptr32: u32::from(h.Flags) & EVENT_HEADER_FLAG_32_BIT_HEADER != 0,
        data,
    };
    ctx.counters.events.fetch_add(1, Ordering::Relaxed);
    if catch_unwind(AssertUnwindSafe(|| (ctx.handler)(&view))).is_err() {
        ctx.counters.panics.fetch_add(1, Ordering::Relaxed);
    }
}

/// The C callback: a panic must never unwind into ETW.
unsafe extern "system" fn callback(rec: *mut EVENT_RECORD) {
    if rec.is_null() {
        return;
    }
    // SAFETY: ETW passes a valid EVENT_RECORD for the duration of the callback; null was rejected above.
    let rec = unsafe { &*rec };
    let _ = catch_unwind(AssertUnwindSafe(|| dispatch(rec)));
}

const PROPS_BYTES: usize = size_of::<EVENT_TRACE_PROPERTIES>() + 1024;

/// A zeroed, 8-aligned buffer holding a filled `EVENT_TRACE_PROPERTIES` followed by room for the names.
fn props_buf(timebase: Timebase) -> Vec<u64> {
    let mut buf = vec![0u64; PROPS_BYTES / 8 + 1];
    // SAFETY: all-zero is a valid bit pattern for this plain C struct (integers, pointers, a GUID).
    let mut p: EVENT_TRACE_PROPERTIES = unsafe { zeroed() };
    p.Wnode.BufferSize = PROPS_BYTES as u32;
    p.Wnode.Guid = GUID::from_u128(SESSION_GUID);
    p.Wnode.ClientContext = if timebase == Timebase::Tsc { 3 } else { 1 };
    p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    p.BufferSize = 128;
    p.MinimumBuffers = 8;
    p.MaximumBuffers = 64;
    p.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE;
    p.EnableFlags = KERNEL_FLAGS;
    p.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
    let ptr = buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
    // SAFETY: `buf` is zeroed, 8-byte aligned and PROPS_BYTES long, larger than the struct written.
    unsafe { ptr.write(p) };
    buf
}

fn stop_by_name(name: &[u16], handle: CONTROLTRACE_HANDLE, timebase: Timebase) -> u32 {
    let mut buf = props_buf(timebase);
    // SAFETY: `buf` holds a valid EVENT_TRACE_PROPERTIES and outlives the call; `name` is NUL-terminated UTF-16.
    unsafe {
        ControlTraceW(
            handle,
            name.as_ptr(),
            buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>(),
            EVENT_TRACE_CONTROL_STOP,
        )
    }
}

/// Enables a manifest provider; `event_ids` (when not empty) restricts it to those event ids.
fn enable(
    handle: CONTROLTRACE_HANDLE,
    provider: u128,
    keyword: u64,
    event_ids: &[u16],
    capture_state: bool,
) -> u32 {
    let guid = GUID::from_u128(provider);
    // EVENT_FILTER_EVENT_ID: FilterIn (1 byte) + Reserved (1 byte), Count (u16), then the ids.
    let mut filter: Vec<u16> = vec![1, event_ids.len() as u16];
    filter.extend_from_slice(event_ids);
    let desc = EVENT_FILTER_DESCRIPTOR {
        Ptr: filter.as_ptr() as u64,
        Size: (filter.len() * 2) as u32,
        Type: EVENT_FILTER_TYPE_EVENT_ID,
    };
    // SAFETY: all-zero is a valid bit pattern for this plain C struct.
    let mut params: ENABLE_TRACE_PARAMETERS = unsafe { zeroed() };
    params.Version = ENABLE_TRACE_PARAMETERS_VERSION_2;
    let filtered = !event_ids.is_empty();
    if filtered {
        params.EnableProperty = EVENT_ENABLE_PROPERTY_IGNORE_KEYWORD_0;
        params.EnableFilterDesc =
            &desc as *const EVENT_FILTER_DESCRIPTOR as *mut EVENT_FILTER_DESCRIPTOR;
        params.FilterDescCount = 1;
    }
    let code = if capture_state {
        EVENT_CONTROL_CODE_CAPTURE_STATE
    } else {
        EVENT_CONTROL_CODE_ENABLE_PROVIDER
    };
    // SAFETY: `handle` is a live session; `guid`, `params`, `desc` and `filter` outlive the call.
    unsafe { EnableTraceEx2(handle, &guid, code, 0xFF, keyword, 0, 0, &params) }
}

/// A running capture session. Dropping it stops the session.
pub struct Session {
    control: CONTROLTRACE_HANDLE,
    trace: PROCESSTRACE_HANDLE,
    name: Vec<u16>,
    timebase: Timebase,
    ctx: *mut Ctx,
    counters: Arc<Counters>,
    worker: Option<JoinHandle<u32>>,
}

// SAFETY: `ctx` is only dereferenced by the ProcessTrace thread while it runs and by `Drop` after that thread was
// joined; its handler is `Send`; the handles are plain integers.
unsafe impl Send for Session {}

impl Session {
    /// Starts the session (stopping a stale one of the same name first). Needs an elevated process.
    pub fn start(handler: impl FnMut(&EventView<'_>) + Send + 'static) -> io::Result<Session> {
        let timebase = Timebase::from_invariant(fa_asm::tsc::invariant_tsc());
        let mut name = wide(SESSION_NAME);
        // A crashed run may have left a session behind; 4201 means there was none.
        let _ = stop_by_name(&name, CONTROLTRACE_HANDLE { Value: 0 }, timebase);

        let mut buf = props_buf(timebase);
        let mut control = CONTROLTRACE_HANDLE { Value: 0 };
        // SAFETY: `control` and `buf` are valid for the call; `name` is NUL-terminated UTF-16.
        let rc = unsafe {
            StartTraceW(
                &mut control,
                name.as_ptr(),
                buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>(),
            )
        };
        if rc != 0 {
            return Err(win_err("StartTraceW (is the process elevated?)", rc));
        }
        let fail = |what: &str, code: u32| {
            let _ = stop_by_name(&wide(SESSION_NAME), control, timebase);
            win_err(what, code)
        };
        for (provider, keyword, ids, rundown) in [
            (DXGI, 0x8000_0000_0000_0002u64, &[][..], false),
            (
                DXGKRNL,
                0x4000_0000_0800_0001,
                &DXGKRNL_EVENT_IDS[..],
                false,
            ),
            (KERNEL_PROCESS, 0x10, &[][..], false),
            // Rundown: ask Kernel-Process for the processes that already run.
            (KERNEL_PROCESS, 0x10, &[][..], true),
        ] {
            let rc = enable(control, provider, keyword, ids, rundown);
            if rc != 0 {
                return Err(fail("EnableTraceEx2", rc));
            }
        }

        let counters = Arc::new(Counters::default());
        let ctx = Box::into_raw(Box::new(Ctx {
            handler: Box::new(handler),
            counters: Arc::clone(&counters),
        }));
        // SAFETY: all-zero is a valid bit pattern for EVENT_TRACE_LOGFILEW (integers, pointers, plain structs, Option fn).
        let mut log: EVENT_TRACE_LOGFILEW = unsafe { zeroed() };
        log.LoggerName = name.as_mut_ptr();
        log.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME
            | PROCESS_TRACE_MODE_EVENT_RECORD
            | PROCESS_TRACE_MODE_RAW_TIMESTAMP;
        log.Anonymous2.EventRecordCallback = Some(callback);
        log.Context = ctx.cast::<c_void>();
        // SAFETY: `log` is fully initialised; `name` and `ctx` outlive the trace (freed in Drop after the join).
        let trace = unsafe { OpenTraceW(&mut log) };
        if trace.Value == u64::MAX {
            let err = fail("OpenTraceW", 0);
            // SAFETY: `ctx` came from Box::into_raw above and no trace uses it.
            drop(unsafe { Box::from_raw(ctx) });
            return Err(err);
        }
        let raw = trace.Value;
        let worker = std::thread::spawn(move || {
            let h = PROCESSTRACE_HANDLE { Value: raw };
            // SAFETY: `h` was returned by OpenTraceW; ProcessTrace blocks until the session is stopped.
            unsafe { ProcessTrace(&h, 1, null(), null()) }
        });
        Ok(Session {
            control,
            trace,
            name,
            timebase,
            ctx,
            counters,
            worker: Some(worker),
        })
    }

    /// Whether event timestamps are raw TSC ticks or (degraded mode) QPC ticks.
    pub fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Events delivered to the handler so far.
    pub fn events(&self) -> u64 {
        self.counters.events.load(Ordering::Relaxed)
    }

    /// Handler panics caught at the C boundary so far.
    pub fn handler_panics(&self) -> u64 {
        self.counters.panics.load(Ordering::Relaxed)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = stop_by_name(&self.name, self.control, self.timebase);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        // SAFETY: `trace` was returned by OpenTraceW and is closed once, after ProcessTrace returned.
        let _ = unsafe { CloseTrace(self.trace) };
        // SAFETY: `ctx` came from Box::into_raw and the only thread that used it has been joined.
        drop(unsafe { Box::from_raw(self.ctx) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;
    use std::sync::Mutex;

    type Seen = (u128, u16, u64, u32, u32, u16, bool, Vec<u8>);

    fn record_with(ctx: *mut Ctx, data: &[u8]) -> EVENT_RECORD {
        // SAFETY: all-zero is a valid bit pattern for EVENT_RECORD (integers, pointers, plain structs).
        let mut rec: EVENT_RECORD = unsafe { zeroed() };
        rec.UserContext = ctx.cast::<c_void>();
        rec.UserData = data.as_ptr() as *mut c_void;
        rec.UserDataLength = data.len() as u16;
        rec.EventHeader.ProviderId = GUID::from_u128(DXGI);
        rec.EventHeader.EventDescriptor.Id = 0x2a;
        rec.EventHeader.EventDescriptor.Version = 0;
        rec.EventHeader.EventDescriptor.Opcode = 1;
        rec.EventHeader.TimeStamp = 123_456;
        rec.EventHeader.ProcessId = 77;
        rec.EventHeader.ThreadId = 88;
        rec.BufferContext.Anonymous.ProcessorIndex = 5;
        rec
    }

    #[test]
    fn the_view_carries_the_header_fields_and_the_payload() {
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let sink = Arc::clone(&seen);
        let counters = Arc::new(Counters::default());
        let mut ctx = Ctx {
            handler: Box::new(move |v| {
                sink.lock().unwrap().push((
                    v.provider,
                    v.id,
                    v.timestamp,
                    v.pid,
                    v.tid,
                    v.cpu,
                    v.ptr32,
                    v.data.to_vec(),
                ));
            }),
            counters: Arc::clone(&counters),
        };
        let mut rec = record_with(&mut ctx, &[1, 2, 3]);
        // SAFETY: `rec` is a valid EVENT_RECORD and its context points to `ctx`, both alive for the call.
        unsafe { callback(&mut rec) };
        rec.EventHeader.Flags = EVENT_HEADER_FLAG_32_BIT_HEADER as u16;
        // SAFETY: as above.
        unsafe { callback(&mut rec) };
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0],
            (DXGI, 0x2a, 123_456, 77, 88, 5, false, vec![1, 2, 3])
        );
        assert!(seen[1].6, "the 32-bit header flag is carried");
        assert_eq!(counters.events.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_panicking_handler_does_not_cross_the_c_boundary() {
        let counters = Arc::new(Counters::default());
        let mut ctx = Ctx {
            handler: Box::new(|_| panic!("injected")),
            counters: Arc::clone(&counters),
        };
        let mut rec = record_with(&mut ctx, &[]);
        for _ in 0..3 {
            // SAFETY: `rec` is a valid EVENT_RECORD and its context points to `ctx`, both alive for the call.
            unsafe { callback(&mut rec) };
        }
        assert_eq!(
            counters.panics.load(Ordering::Relaxed),
            3,
            "every panic was caught and counted, none escaped"
        );
    }

    #[test]
    fn a_null_record_or_context_is_ignored() {
        // SAFETY: a null record is the documented-safe case the callback checks first.
        unsafe { callback(null_mut()) };
        let mut rec = record_with(null_mut(), &[]);
        // SAFETY: `rec` is a valid EVENT_RECORD; a null context is ignored.
        unsafe { callback(&mut rec) };
    }
}
