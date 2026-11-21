# Design

## Context

Lot 0 provides the workspace, `fa-asm`/`fa-win`/`fa-core` skeletons, the gate and the catalog. Target machine:
i5-11400H, 6 identical cores, one LLC, no hybrid cores, so hybrid and dual-CCD topologies can only be tested with
synthetic fixtures. The timebase is raw TSC ticks everywhere (`docs/architecture.md` §5); ETW (lot 5) will hand us
raw TSC timestamps that must compare directly with probe samples. See proposal.md for motivation, specs for the
requirements.

## Goals / Non-Goals

**Goals:**
- One `Clock` type: `now()` ticks, tick-to-ns, QPC-to-TSC mapping, degraded QPC mode, usable from any crate that may
  depend on `fa-win` (layout in D-12).
- ASM-01 follows earn-its-place with a recorded verdict, whichever way it goes.
- A topology model usable by lots 3 and 12 without touching raw Windows structures.

**Non-Goals:**
- Probes, effective frequency, cache and wake-up measurement (lot 3).
- ETW timestamp handling (lot 5), JFR tick mapping (lot 15), wall-clock mapping beyond a session-start record.
- Any CPU Set or affinity change on other processes (lot 12); this lot only reads topology and pins its own test threads.

## Decisions

**D-1 ASM-01 earns its place.** The catalog line exists (ASM-01: reference `_rdtsc`/`__rdtscp` + `_mm_lfence`, metric
read overhead in cycles and jitter stdev, parity: same tick within overhead window and monotonic, fallback QPC, breaks
if `lfence` is missing). Order of work: (1) confirm/complete the catalog line before any code; (2) intrinsics version
in `fa-asm::tsc` (`core::arch::x86_64::{_rdtsc, __rdtscp, _mm_lfence}`) with `// SAFETY:` comments, which is the
reference and ships by default; (3) `asm!` version (Intel syntax, precise `options`; `rdtsc` clobbers EDX:EAX,
`rdtscp` also ECX) behind the same signature, followed by the French explanation checkpoint; (4) parity test;
(5) criterion bench `tsc_vs_intrinsics`, pinned to one core per catalog §2; (6) verdict: the asm wins only if it beats
intrinsics by more than 5 % and the run-to-run spread, else the asm module is deleted and the numbers stay in the
catalog. Runtime fallback: `FA_FORCE_SCALAR=1` forces the reference path. Alternative rejected: shipping asm "to
learn" without a win (violates D1). Honest expectation: intrinsics compile to nearly the same instructions, so a tie
is likely; that is a valid outcome.

**D-2 Invariant TSC check.** CPUID `0x80000007:EDX[8]` read once at startup via `__cpuid`. If clear, the `Clock` runs
in QPC mode: same API, ticks are QPC counts, frequency from `QueryPerformanceFrequency`, and the health check reports
the degraded mode. Alternative rejected: trusting the TSC without the flag (unproven across cores and sleep states).

**D-3 TSC frequency.** CPUID `0x15` when the crystal ratio (EBX) and the crystal Hz (ECX) are non-zero:
`freq = ecx * ebx / eax`. Many Intel parts report ECX = 0; then calibrate against QPC: 5 runs of at least 200 ms,
median kept (`freq = dTSC * qpc_freq / dQPC` in u128). The value is stored in the health baseline. If both are
available and disagree beyond a threshold, the calibrated value wins and the gap is logged (the CPUID value can be
nominal rather than actual). Whether the i5-11400H reports ECX is verified at implementation time, not assumed.

**D-4 Integer conversions.** `ns = ticks * 1_000_000_000 / freq` in u128 (no float, no drift, no overflow for any u64
tick count). Frequency stored as u64 Hz. Property test against a reference and boundary values.

**D-5 QPC<->TSC mapping.** A pair `(tsc, qpc)` taken back-to-back (QPC read bracketed by two TSC reads, midpoint used)
plus the scale `freq_tsc / freq_qpc`. Re-estimated every 60 s; the drift between the previous prediction and the new
pair is recorded in ns. Alternative rejected: single calibration at startup (frequency error accumulates, spec
requires drift tracking).

**D-6 Topology from Windows APIs.** `GetSystemCpuSetInformation` gives CPU Set ids, `EfficiencyClass`, core and LLC
indexes; `GetLogicalProcessorInformationEx` gives caches, groups and NUMA. `unsafe` is limited to calling the API in
`fa-win::topology`; the parser takes `&[u8]` and returns a safe model (logical CPU -> core -> LLC domain -> efficiency
class), so it is testable on synthetic buffers. Buffer size comes from the first call and every record's `Size` field
is bounds-checked before reading (fail closed, no panic). Processor groups are modelled but only exercised
synthetically.

**D-7 CPUID cross-check is a test, not a source.** A test compares the API model with leaves `0x0B`/`0x1F`, `0x1A`,
`0x8000001E`, `0x04`/`0x8000001D` through `__cpuid_count`, pinned per logical CPU, and logs mismatches (leaves absent
on the vendor are skipped, not failed). Documented as a validation and learning device.

**D-8 Synthetic topologies.** Hybrid (P+E) and dual-CCD buffers are hand-built byte fixtures under `lab/fixtures/`
(tiny, far below the 50 MB cap) and feed the same parser as the real call.

**D-9 Dev command `fa clock`.** Non-elevated and read-only: prints invariance, frequency and its source, the 10 s
TSC vs QPC gap, per-core skew and a topology summary. No input crosses into the elevated collector in this lot, so no
whitelist applies here.

**D-10 Cross-core skew test.** A test thread pinned successively (affinity on our own thread only) to each logical CPU
reads the TSC with `now_serialized()`; monotonicity and skew < 1 µs are checked, with samples preempted between reads
rejected.

**D-11 Own-thread pinning (WIN-07, moved from lot 3).** `fa-win::thread::pin_current_thread(logical_cpu) ->
PinGuard`: `SetThreadAffinityMask` on the current-thread pseudo-handle (`GetCurrentThread`), mask restored when the
guard drops. It touches only our own thread, opens no handle (no `OpenThread`, the source test stays green) and
changes no other process. Limit: a single processor group (at most 64 logical CPUs), enough for the target; a
`shortcut:` comment marks it. Lot 3 reuses it and adds only the timers.

**D-12 Crate layout for the clock (Claude's call).** `fa-core` cannot depend on `fa-win` (`fa-win` will depend on
`fa-core` for `TargetProcess`), so: pure math (u128 tick-to-ns, median, QPC<->TSC map from injected readings) in
`fa-core::clock`; TSC and CPUID reads in `fa-asm::tsc`; QPC read and the `Clock` type that ties them together in
`fa-win::clock`, which now depends on `fa-asm` and `fa-core`. `docs/architecture.md` section 2 is updated. Rejected: a
new `fa-clock` crate (a fifth crate for a few hundred lines) and putting `Clock` in `fa-probe` (lots 5 and 8 need it
without probes).

**User decision (2026-11-21): explanation checkpoints.** The rule is a theoretical baseline: this lot keeps the
single French checkpoint for the asm block (task 6.2) and adds none per `unsafe` group.

## Risks / Trade-offs

- [asm and intrinsics tie] -> expected and acceptable; the reference ships, the asm is deleted, numbers are kept.
- [CPUID `0x15` missing or ECX = 0 on the target] -> the QPC calibration path is then the main path and is tested on its own.
- [QPC calibration noise on a loaded laptop] -> median of 5 runs, run at idle, thresholds marked `(calibrate)`.
- [10 µs / 1 µs thresholds too tight on battery or in a VM] -> thresholds are `(calibrate)`; a recalibration is logged
  in `docs/decisions.md` §5, never silently loosened.
- [Hybrid and dual-CCD paths unvalidated on real hardware] -> synthetic fixtures only, as the spec states; real
  validation waits for another machine.
- [Wrong fence placement gives a plausible but biased tick] -> parity and monotonic tests plus the explanation checkpoint.

## Migration Plan

New code only, nothing to migrate. Rollback: revert the lot's branch. If ASM-01 loses, the asm module is deleted and
the intrinsics reference stays.
