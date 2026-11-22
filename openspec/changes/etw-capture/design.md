# Design

## Context

Lot 5 follows lots 0 to 4 (clock, topology, probes, `fa check`). Facts measured on the target (i5-11400H, Windows 11
build 26300, AC): TSC invariant at 2 688 000 000 Hz from CPUID `0x15`, 4.6 ppm above QPC (negligible for frametimes);
`fa_core::clock::Timebase` and `ticks_to_ns` already do the integer conversions; `fa_sync` already has `CountingAlloc`,
`assert_no_alloc` (feature `test-support`, `crates/fa-sync/src/counting.rs`). The laptop is Optimus (RTX 3060 Laptop +
Intel UHD). See `proposal.md` for motivation and compliance effect and `docs/architecture.md` sections 3 to 5 for trust
boundaries, data flow and timebase. ETW needs an elevated terminal; every elevated step is run by the user.

## Goals / Non-Goals

**Goals:**
- A real-time ETW session with raw TSC timestamps and a zero-allocation hot path (callback, decode, ring push).
- Frame records per process and swap chain, with parity against PresentMon.
- A verdict on the 24H2 kernel address question, logged before more is built on it.
- WIN-01 and SYNC-01 each earn their place against `ferrisetw` and `rtrb`, or the reference ships.

**Non-Goals:**
- The collector service, pipes, shared memory (lot 7); storage (lot 8); the cause engine (lot 10).
- Stack walks and symbolization (O3, lot 11); the D3D11 Lab render loop (lot 6).
- Any process handle (`OpenProcess` and friends); `SeDebugPrivilege`.

## Decisions

**D-1. Spike first (group 1).** An admin session on build 26300 without `SeDebugPrivilege` enables DPC/ISR, image-load
rundown and (for the spike only, a short run) a stack-walk request, then reports whether routine and stack addresses
are non-zero. The user runs the elevated terminal. The verdict goes in `docs/decisions.md` before any other group.
Fallback if addresses are zeroed: the module map keeps module names (and bases) from image-load events but resolves no
address to module + offset; DPC/ISR events are attributed by count and duration only; O3 stack walks (lot 11) and
`driver-attribution` are impacted and must be re-scoped with the user. We never enable `SeDebugPrivilege`.

**D-2. Session and consumer in `fa-win::etw` (WIN-01).** `StartTraceW` with session name `FrameAutopsy`, real-time
mode, `ClientContext = 3` when the TSC is invariant (else QPC through `Timebase::Qpc`); `EnableTraceEx2` per provider;
`OpenTraceW` with real-time, event-record and `PROCESS_TRACE_MODE_RAW_TIMESTAMP` modes (without the last flag
`ProcessTrace` converts timestamps); `ProcessTrace` on a dedicated thread. At startup a stale session of that name is
stopped. Every `extern "system"` callback wraps its body in `catch_unwind`. The callback gets an `EVENT_RECORD*` valid
only during the call: it builds a borrowed view (header fields + `&[u8]`) and never stores the pointer. Whether
Windows 11 allows kernel events in a private session instead of the NT Kernel Logger is verified in the spike
(`docs/architecture.md` section 8); if not, the NT Kernel Logger is used.

**D-3. Earn its place: WIN-01 against `ferrisetw`.** Catalog line exists; results filled at the end. Reference:
`ferrisetw`. Parity test: both consumers decode the same synthetic `EVENT_RECORD` stream to the same records. Recorded
comparison: allocations per event and throughput (events/s) on the same stream, 3 runs, median and spread. Rule: the
hand-written consumer stays for hot providers only with 0 allocations per event **and** throughput at least equal;
a throughput gap under 5 % counts as equal. Otherwise `ferrisetw` is adopted for the non-hot providers. Runtime
fallback: session code is shared; only the decode path differs, selected by a feature.

**D-4. Decoding in `fa-capture` (safe).** Layouts keyed by (provider, event id, version) into fixed-size `Copy`
records, read with checked slice indexing and `from_le_bytes`: no `unsafe`, no allocation. Unknown versions are counted
and skipped. TDH is used only by a developer tool that dumps layouts (it may allocate). Exact keywords and flags follow
PresentMon's source (MIT); the commit read is recorded in the design notes.

**D-5. Frame extraction.** Follow PresentMon's algorithm (present start, displayed time, frametime, present mode) per
process and swap chain. PresentMon itself is the parity reference. The target is Optimus, so presents may go through a
hybrid or cross-adapter copy; present-mode classification on this machine is unverified (see Risks).

**D-6. SPSC ring in `fa-sync::spsc` (SYNC-01) and Miri.** Power-of-two capacity, `T: Copy`, `MaybeUninit<T>` slots,
head and tail on separate cache lines, producer Release-stores tail and Acquire-loads head, consumer the reverse;
`push` never blocks and counts drops. Earn-its-place: reference `rtrb`; parity test (same sequence in, same sequence
out); recorded comparison by criterion on 32-byte records (throughput and latency, 3 runs); fallback: use `rtrb` behind
the same wrapper. **It may tie or lose**: 4 of 5 candidates so far tied. A win under 5 % is a tie and `rtrb` ships;
then `spsc.rs` is deleted, which also removes the Miri step from the gate (`scripts/gate.ps1` line 35 runs Miri only
while that file exists). Miri is **not installed** on nightly (`cargo +nightly miri --version` fails): the user must
approve `rustup +nightly component add miri`, which changes their toolchain, **before** `spsc.rs` is created, or the
gate fails at once. Roadmap rule: every memory ordering is explained to the user before ETW is wired to the ring.

**D-7. Timestamps.** `ClientContext = 3` and raw timestamps give TSC ticks comparable with probe samples. Conversion to
time uses `fa_core::clock::ticks_to_ns` with the lot 2 frequency; no new clock code. Without an invariant TSC the
session uses QPC and the report says so.

**D-8. Identification without handles.** PID and full image name come only from Microsoft-Windows-Kernel-Process
events (start and rundown). Classic kernel process events truncate names to 15 bytes: they are never a name source,
and `TargetProcess` fails closed on a truncated name. Lot 5 needs no process handle and adds none.

**D-9. PresentMon parity target.** The roadmap asks for parity "on a simple D3D test window before the Lab", but the
D3D11 render loop is WIN-05 (lot 6). Decision: lot 5 measures parity (at least 99 % of frames within 0.1 ms, calibrate)
on an existing non-game DXGI application chosen with the user (**open question**); lot 6 replays the same comparison on
the Lab loop. PresentMon is installed by the user (availability unverified); both tools capture at the same time. If
no suitable application is found, the parity criterion moves to lot 6 and this is logged in `docs/decisions.md` section 5.

**D-10. Loss accounting.** ETW lost-buffer and lost-event counters, unknown-version counts and ring drops are plain
counters printed by `fa capture`.

**D-11. Hot path proof.** One million synthetic `EVENT_RECORD`s run through callback, decode and push under
`assert_no_alloc`, drop-on-full path included. Ring and decoders are pre-sized at startup.

## Risks / Trade-offs

- [Kernel addresses zeroed on 26H2] -> spike first; fallback in D-1; O3 impacted.
- [Optimus cross-adapter presents, present-mode classification unverified] -> parity run on this machine, report
  mismatches by present mode, claim nothing beyond what was measured.
- [PresentMon not installed or no suitable DXGI app] -> user installs it; criterion may move to lot 6 (D-9).
- [SYNC-01 ties `rtrb`] -> `rtrb` ships, `spsc.rs` and the Miri gate step go, numbers stay in the catalog.
- [WIN-01 loses to `ferrisetw`] -> adopt it for non-hot providers; the hot decode path still must show 0 allocations.
- [Miri component missing] -> explicit task before `spsc.rs`; without approval the SPSC work waits.
- [A pointer from the callback outliving the call] -> only borrowed views; decoders copy into `Copy` records.
- [A panic crossing the C boundary] -> `catch_unwind` in every callback, tested with an injected panic.
- [Orphan session after a crash] -> stop by name at startup and at exit; test starts twice in a row.
- [Running ETW near a competitive game] -> `fa capture` is a developer command, the user decides when; O1/O2 stay unverified.
