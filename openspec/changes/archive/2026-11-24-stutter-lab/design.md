# Design

## Context

Lots 2 to 4 provide the TSC clock, topology, pinning, probes and the FMA load. Lot 5 provides ETW capture with
PresentMon-parity frame times. Target machine: i5-11400H (6 cores / 12 threads, one LLC of 12 MiB), RTX 3060 Laptop +
Intel UHD (Optimus), Windows 11 build 26300. See `proposal.md` for the compliance effect.

## Goals / Non-Goals

**Goals:**
- Stutters of known cause, start and end, recorded in TSC ticks, big enough to be seen above the loop's own jitter.
- Each injector acts on the Lab only and is bounded and reversible.

**Non-Goals:**
- No scoring (lot 10, `cause-engine`); no game, no action on another process.
- No large pages (rejected by the user 2026-11-22, catalog WIN-09).
- No GPU load injector in this lot.

## Decisions

**D-1 Layout and gate.** The Lab is a module directory `crates/fa/src/lab/` (`mod.rs`, then `loop`, `inject`, `journal`,
`scenario`, `guard`), safe Rust (`fa` forbids unsafe). `scripts/gate.ps1` line 36 tests `crates/fa/src/lab.rs`, which
would run `cargo run -p fa --release -- lab score` in lot 6 although the scorer arrives in lot 10. The trigger becomes
`crates/fa/src/lab/score.rs`. Verified by running the gate in lot 6 and seeing no `lab score` step.

**D-2 Render loop and pacing.** `fa-win` owns the unsafe D3D11/DXGI/window code (WIN-05); `fa` drives it through a safe
API. Measured timer lateness (high-resolution waitable timer, 1 ms): p50 about 0.5 ms, p99 1.3 to 3.3 ms, up to 4.6 ms
with a background build. A sleep-paced 144 FPS loop (6.9 ms) would jitter by up to half a frame and pollute the ground
truth. So: vsync cases pace on the flip-model swap chain (present interval, waitable swap chain); capped cases use a
coarse timer sleep to about 1 to 2 ms before the deadline, then a TSC-deadline spin; uncapped runs flat out. Injected
stalls are sized well above the loop's measured jitter, and each scenario records the loop's own baseline jitter (frame
time p50/p99/max over a stall-free lead-in) in the journal header.
The adapter the window presents on (Optimus: RTX or Intel) is unverified: it is recorded in the journal header.

**D-3 Journal.** One JSON object per line: `{cause, start_tsc, end_tsc, params}`, plus a header line (TSC frequency,
adapter, baseline jitter, scenario name). A start line is written before the injection and an end line after, so a
killed run still shows what was active. Same timebase as the capture (raw TSC).

**D-4 Scenarios.** TOML in `lab/scenarios/`: name, FPS mode, duration, `[[injection]]` entries (cause, start, duration,
intensity, cpus) and `expected_causes`. Parsed with `deny_unknown_fields`; durations and sizes are validated against the
guard caps before anything starts. nine injectors, 10 single-cause scenarios (`cpu-burn` appears twice, with and without the FMA load) and 3 two-cause.

**D-5 CPU choice.** Burners never run on the render thread's CPU unless the cause demands it (`priority-contention`).
The SMT bias is measured: the frequency probe reads about 9 % low when its core's SMT sibling is busy. `cpu-burn` never
loads the SMT sibling of a CPU that something else measures (render thread, sampler): CPUs come from
`fa_probe::plan::load_cpus`, unit tested on synthetic topologies. `cpu-burn` is safe Rust spinning to a TSC deadline;
with the FMA option it calls `fa_asm::fma::burn` (ASM-06, already shipped as intrinsics after a tie, the asm deleted:
reuse it, nothing to earn).

**D-6 ASM-07 `clflushopt` (`cache-thrash`).** Still to earn its place; 4 of 5 candidates so far tied and had their asm
deleted. Claude's call: write the intrinsics reference first (`_mm_clflushopt` / `_mm_clflush`, CPUID 7.0:EBX[23], fence
before timing) and the behavioural metric (measured latency jump after flush, and the capture seeing it); write asm only
if the reference cannot produce the behaviour. Gain uncertain. Parity: same lines flushed. Fallback: `clflush`, then
large-stride sweeps. A win under 5 % is a tie and the reference ships. Verdict recorded in the catalog. Buffers are sized
well above the 12 MiB topology (the effective capacity measured is 3 to 4 MiB with 4 KiB pages, cause unknown).

**D-7 Other injectors** (with `cpu-burn` of D-5 and `cache-thrash` of D-6, the seven below make the nine).
- `frame-stall`: burn N ms before `Present` at frame k (safe, TSC deadline).
- `tlb-thrash`: one touch per 4 KiB page over a large region, via `fa_win::mem::PageBuf` with 4 KiB pages (large pages
  are refused on the target).
- `mem-bandwidth`: non-temporal stores on N threads, reusing the ASM-04 intrinsics kernels (its asm was deleted after the tie) if one fits, else a new intrinsics loop.
- `page-fault-storm`: commit/touch/decommit and working-set trims of the Lab process (WIN-12).
- `disk-burst`: unbuffered write-through writes to a temporary file, deleted afterwards; at most 1 GB, 10 GB free needed.
- `timer-resolution`: `timeBeginPeriod` toggles; per-process since Windows 10 2004, so it changes the Lab's own pacing only.
- `priority-contention`: high-priority Lab worker on the render thread's CPU (own threads only).

**D-8 Guards.** Refusal reuses `fa_probe::plan::running_games` with Toolhelp names and window enumeration (no process
handle) and names the process. Caps: each injection at most 30 s; disk bursts at most 1 GB and 10 GB free needed.
Cleanup on Ctrl+C through `fa_win::console::install_ctrl_c`: stop flag, injectors joined, temporary files deleted,
timer period restored, journal closed. Dev commands run from an admin terminal.

**D-9 Order.** Render loop and `frame-stall` first; verify the lot 5 capture sees the exact stall (PresentMon parity
replayed on the Lab loop); then the other injectors one by one.

## Risks / Trade-offs

- Spin pacing burns a core; accepted for a dev tool, and that core is excluded from burners.
- The Lab's own loop jitter may hide small injections; hence large sizes and the recorded baseline. The jitter budget is `(calibrate)`: measured on the idle loop (task 2.3), frozen from that result, logged in `docs/decisions.md` section 5.
- ASM-07 may tie: the intrinsics ship, no asm checkpoint needed.
- COM/D3D11 unsafe is large; wrapped behind a small safe API in `fa-win` and explained to the user.
- Optimus adapter unknown: recorded, not assumed.
