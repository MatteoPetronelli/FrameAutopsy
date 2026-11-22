# Tasks

Gate = `powershell -File scripts/gate.ps1`. Each group ends with the gate. Each new unsafe block gets an explanation
checkpoint in French (template: `docs/asm-unsafe-catalog.md` section 7), merged only after the user says "compris".
ETW steps need an elevated terminal, run by the user.

## 1. Spike: kernel addresses on build 26300

- [x] 1.1 Write a throwaway spike (no new crate, not committed as product code) that starts an admin session without `SeDebugPrivilege`, enables DPC/ISR, image-load rundown and a short stack-walk request, and counts non-zero kernel addresses; verify it never calls `OpenProcess` or enables `SeDebugPrivilege` (`no_open_process` source test green).
- [x] 1.2 The user runs it in an elevated terminal; verify the raw output (counts of zero and non-zero addresses per event type) is pasted in the session.
- [x] 1.3 Record the verdict, and whether kernel events work in a private session or need the NT Kernel Logger, in `docs/decisions.md`; verify the entry states the fallback of design D-1 if addresses are zeroed, and the user has read it.
- [x] 1.4 Run the gate and verify it is green.

## 2. SPSC ring and Miri (before any ETW wiring)

- [x] 2.1 Ask the user to approve `rustup +nightly component add miri` (it changes their toolchain); verify `cargo +nightly miri --version` succeeds. Do not create `crates/fa-sync/src/spsc.rs` before this passes: the gate runs Miri as soon as the file exists.
- [x] 2.2 Update the SYNC-01 catalog line (metric, reference `rtrb`, expected outcome: may tie) before code; verify the line in `docs/asm-unsafe-catalog.md`.
- [x] 2.3 Implement `fa-sync::spsc` (power-of-two capacity, `T: Copy`, `MaybeUninit<T>`, padded head and tail, Acquire/Release, non-blocking `push` with drop counter), each unsafe block with an uppercase `// SAFETY:` and one unsafe operation; verify with unit tests (order, full ring returns error and counts a drop, wrap-around).
- [x] 2.4 Add the two-thread stress test; verify `cargo +nightly miri test -p fa-sync` passes.
- [x] 2.5 French explanation checkpoint for every memory ordering and unsafe block of the ring, before ETW is wired to it; verify the user answered "compris".
- [x] 2.6 Parity test against `rtrb` (same sequence in and out) and criterion bench on 32-byte records (3 runs, median and spread, catalog section 2 protocol); verify results are pasted in the catalog.
- [x] 2.7 Decide: ship `spsc.rs` only if throughput is at least equal and the win is 5 % or more or latency is better by the same rule; otherwise adopt `rtrb`, delete `spsc.rs` (this removes the gate Miri step), keep the numbers; verify the verdict in the catalog and that the gate matches the file state.
- [x] 2.8 Run the gate and verify it is green.

## 3. Crate `fa-capture`, decoders, synthetic stream

- [x] 3.1 Create crate `fa-capture` with `#![forbid(unsafe_code)]` and register it in the workspace; verify `cargo build -p fa-capture` and the gate headers step.
- [x] 3.2 Read PresentMon's source (MIT) for keywords, flags, event ids and versions; note the commit and the list in the design notes; verify the list covers every provider of spec `etw-capture` "Providers".
- [x] 3.3 Write the TDH layout-dump developer tool (may allocate, not on the hot path); verify it prints the layout of one known event on the user's machine.
- [x] 3.4 Implement hand-written decoders keyed by provider, event id and version into fixed-size `Copy` records, with an unknown-version counter; verify with tests on synthetic byte buffers including truncated buffers and unknown versions (no panic, counted and skipped).
- [x] 3.5 Build the synthetic `EVENT_RECORD` generator; verify the zero-allocation test passes for one million events through callback, decode and push under `assert_no_alloc`, including the full-ring drop path.
- [x] 3.6 Run the gate and verify it is green.

## 4. ETW session in `fa-win::etw` (WIN-01)

- [x] 4.1 Update the WIN-01 catalog line with the comparison protocol before code; verify the line in the catalog.
- [x] 4.2 Implement the session (`StartTraceW`, `EnableTraceEx2`, `OpenTraceW`, `ProcessTrace`), `ClientContext = 3`, `PROCESS_TRACE_MODE_RAW_TIMESTAMP`, QPC fallback through `Timebase`, RAII stop, orphan-session stop at startup; verify (elevated, by the user) that starting twice in a row works and that timestamps are comparable with `fa_asm::tsc` reads (delta within a bound recorded in the test).
- [x] 4.3 Implement the callback with a borrowed view and `catch_unwind`; verify with a test that an injected panic does not cross the C boundary and that the view is never stored.
- [x] 4.4 Implement the module map (rundown + live image loads, address to module + offset) per the spike verdict; verify with synthetic loads and, if addresses are zeroed, that only names are kept and no address is resolved.
- [x] 4.5 Implement Microsoft-Windows-Kernel-Process identification (PID, full image name); verify with a test that a 15-byte truncated name from a classic kernel event is never accepted as a name, and that `grep` finds no `OpenProcess` outside the allowed file (`no_open_process` green).
- [x] 4.6 French explanation checkpoint for each new unsafe block of `fa-win::etw`; verify the user answered "compris".
- [x] 4.7 Compare with `ferrisetw` on the same synthetic stream (allocations per event, events/s, 3 runs); verify parity of decoded records and record the verdict in the catalog (hand-written for hot providers only with 0 allocations and throughput at least equal, else `ferrisetw` for non-hot providers).
- [x] 4.8 Run the gate and verify it is green.

## 5. Frames, losses, CLI and parity

- [x] 5.1 Implement frame extraction per process and swap chain following PresentMon (present start, displayed time, frametime, present mode); verify with synthetic present sequences including dropped and composed presents.
- [x] 5.2 Implement loss counters (lost buffers, lost events, unknown versions, ring drops); verify with a stalled-consumer test that drops grow while the callback still returns at once.
- [ ] 5.3 Add `fa capture --seconds N` (elevated terminal, refuses to start while a matrix game runs unless the user says otherwise); verify with a command test and a manual run by the user printing frames, counters and the timebase mode.
- [ ] 5.4 Parity targets are Opera and Chrome (WebGL page or 60 fps video, user choice); ask the user to install PresentMon (unverified); verify PresentMon runs and both browsers present through DXGI.
- [ ] 5.5 Run FrameAutopsy and PresentMon in parallel on that application and compare frametimes; verify at least 99 % of frames match within 0.1 ms (calibrate), report the present modes seen on this Optimus laptop, and record the result and any recalibration in `docs/decisions.md` section 5. If it cannot be met or no application exists, log it and move the criterion to lot 6.
- [ ] 5.6 Run the gate and verify it is green.

## 6. Close

- [ ] 6.1 Check that WIN-01 and SYNC-01 each have reference, parity test, recorded comparison and runtime fallback or deletion; verify `openspec validate etw-capture --strict` passes.
- [ ] 6.2 Update `docs/architecture.md` and `CLAUDE.md` status lines if the SPSC or consumer verdicts changed the plan; verify by reading the diff with the user.
- [ ] 6.3 Run the gate and verify it is green.

## Workflow follow-up

- Archive the change after the user approves the lot.
