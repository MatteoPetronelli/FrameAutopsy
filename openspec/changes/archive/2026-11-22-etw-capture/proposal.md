# Proposal

## Why

Everything so far (lots 0 to 4) ran without admin, ETW or a game. Diagnosis needs what only ETW gives: the frametimes
of a running game (present events) and the system events around a stutter (context switches, DPC/ISR, page faults, disk,
process and image events). The Lab (lot 6), the collector (lot 7) and the incident store (lot 8) all consume this
stream. Existing consumers either allocate per event or convert timestamps; the project needs raw TSC timestamps and an
allocation-free hot path, proven by a counting allocator.

Capability purpose (from `specifications.md`): a hand-written real-time ETW consumer, zero-copy and allocation-free on
the hot path, that produces frametimes and the system events needed for diagnosis; plus a lock-free single-producer
single-consumer transport from the ETW callback thread to the analysis thread.

## What Changes

- **Spike first** (group 1): on build 26300 (later than 24H2), does an elevated session without `SeDebugPrivilege`
  receive non-zero kernel addresses (DPC/ISR routines, stack walks, image-load rundown)? Verdict logged in
  `docs/decisions.md`. The user runs the elevated terminal.
- New `fa-win::etw` (WIN-01): session `FrameAutopsy` (`StartTraceW`, `EnableTraceEx2`, `OpenTraceW`, `ProcessTrace`),
  `ClientContext = 3` + `PROCESS_TRACE_MODE_RAW_TIMESTAMP`, stop of an orphan session at startup, `catch_unwind` in every
  callback. Compared with `ferrisetw` under the earn-its-place rule.
- New `fa-sync::spsc` (SYNC-01): SPSC ring, compared with `rtrb`, verified under Miri.
- New safe crate `fa-capture` (`#![forbid(unsafe_code)]`): hand-written decoders keyed by provider, event id and version
  into fixed-size `Copy` records, kernel module map, frame extraction following PresentMon's algorithm (MIT), loss
  counters, and a developer tool that dumps layouts through TDH.
- Dev command `fa capture --seconds N` (elevated terminal).
- Parity with PresentMon measured on a non-game DXGI application (design D-9); replayed on the Lab loop in lot 6.

## Capabilities

### New Capabilities
- `etw-capture`: the real-time ETW session, raw TSC timestamps, providers, the 24H2 kernel address spike, module map,
  zero-copy decoding, zero allocation, loss accounting, frametime extraction and parity, identification without handles.
- `lockfree-transport`: the SPSC ring, its Miri verification and its comparison with `rtrb`.

### Modified Capabilities

None. `tsc-clock` (`Timebase`, `ticks_to_ns`) is consumed, not changed.

## Impact

- **Compliance.** `compliance.toml` is not changed and no matrix row is added. This lot implements the baseline
  observers O1 (present events) and O2 (kernel events); both stay `unverified` until the lot 7 anti-cheat test, which
  only the user runs. O3 (stack walks) is **not** enabled: the spike only checks whether addresses are available, and
  the technical question to Riot waits for lot 11. The "never touch the game" boundary is unchanged: **no `OpenProcess`,
  no handle on any process** (`fa_win::process::open_target` does not exist before lot 12; the source test
  `crates/fa-core/tests/no_open_process.rs` scans every crate, including `fa-win::etw` and `fa-capture`); the game is
  seen only through ETW events. `SeDebugPrivilege` is never enabled. Process names come from
  Microsoft-Windows-Kernel-Process (full names); classic kernel events truncate to 15 bytes and `TargetProcess`
  (`fa-core`) fails closed on a truncated name. `fa capture` is a developer command and is not run during a match of a
  matrix game without the user's go.
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): **WIN-01** ETW session and consumer (comparison with
  `ferrisetw`); **SYNC-01** SPSC ring (comparison with `rtrb`). Results and verdicts are filled at the end of the lot.
  Expect a possible tie: 4 of 5 earn-its-place candidates so far tied with their reference. If SYNC-01 ties or loses,
  `rtrb` ships and `spsc.rs` is deleted.
- **Crates:** new `fa-capture` (safe); touches `fa-win` (`etw`), `fa-sync` (`spsc`), `fa` (`capture`).
- **Dependencies:** `windows`/`windows-sys` features for ETW; dev-only `ferrisetw`, `rtrb`, `criterion` for the
  comparisons (removed if the hand-written code ships and the comparison is archived in the catalog). Miri is a
  toolchain component, not a dependency (design D-6).
- **Gate:** `scripts/gate.ps1` runs Miri as soon as `crates/fa-sync/src/spsc.rs` exists; the Miri component is not
  installed on the nightly toolchain and must be added with the user's approval first.
- **Storage:** none persistent; `fa capture` prints a text summary to the console only. No `.etl` is committed.
