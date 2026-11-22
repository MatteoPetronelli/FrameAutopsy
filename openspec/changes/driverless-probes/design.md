# Design

## Context

Lot 3 builds on `tsc-clock` (raw TSC ticks, integer conversions) and `cpu-topology` (cores, LLC size, CPU Sets), both
from lot 2. See proposal.md for motivation and specs/driverless-probes/spec.md for requirements. Target machine:
i5-11400H, 6 identical cores, one LLC, AVX2 + AVX-512, no WAITPKG, no MWAITX (to be confirmed by CPUID in this lot).
Probes run in our own threads only and off-game only. Every asm/unsafe module follows earn-its-place: catalog line
first, reference, parity test, recorded comparison, runtime fallback (`FA_FORCE_SCALAR=1`).

## Goals / Non-Goals

**Goals:**
- Each probe has its reference, parity test and catalog result.
- ASM-02 accuracy against PDH measured and recorded; clock-domain type of the machine recorded.

**Non-Goals:**
- Keep/remove decision for the frequency probe (lot 10).
- In-game sampling and probe duty cycle (lot 8); any `fa check` verdict (lot 4).
- MSR, `rdpmc`, APERF/MPERF (kernel driver, forbidden); `tpause`/`umwait`/`mwaitx` waits.

## Decisions

**D1. Frequency probe: dependent-add chain (ASM-02).** `core::arch::asm!`, Intel syntax: N `add reg, 1` on the same
register (1-cycle latency each, so cycles = N), unrolled to last about 50 us, bracketed by two TSC reads from lot 2.
Effective frequency = N / elapsed TSC time. Asm is mandatory because rustc folds N repeated `x + 1` into one add.
Samples above 1.5x the recent median are dropped (preemption). References: a Rust chain of dependent squarings (same GHz within
3 %; a `black_box` add chain and `x*3` and `x*(x|3)` chains were tried and rejected, see `fa-asm::freq`) and PDH. Alternative rejected: APERF/MPERF (driver).

**D2. Pointer chase (ASM-03).** Buffer of at least 4x the LLC (from topology), arranged as one random cyclic
permutation built with Sattolo's algorithm. A plain shuffle can leave short cycles whose data fits in cache; Sattolo
yields a single cycle, and the unpredictable order defeats the prefetchers. Each load depends on the previous
(`mov rax, [rax]`). The permutation is built by a small scalar PRNG in `fa-probe` (safe; SIMD-04 is lot 13). Reference:
Rust `read_volatile` chain. Asm ships only if its coefficient of variation is lower; otherwise the module is deleted and
the numbers stay in the catalog. 4 KiB pages, and 2 MiB large pages when `SeLockMemoryPrivilege` is held (D8).

**D3. Bandwidth (ASM-04).** Read with AVX2 loads, write with `vmovntdq` (non-temporal stores bypass the cache and need
an `sfence` before the data counts as written), copy; multi-threaded across physical cores, GB/s. Reference:
`core::arch` intrinsics (`_mm256_stream_si256`), then SSE2, then scalar. Parity: same bytes, same checksum. Asm kept
only if it wins by 5 % or more on the target machine.

**D4. Cache curve.** The pointer chase over working sets from 16 KiB to 256 MiB; plateau detection on the ns/load
ratio; L1/L2/L3 sizes compared with `cpu-topology`. A mismatch is reported, not fatal.

**D5. Wake-up latency, no asm.** A thread waits on a high-resolution waitable timer
(`CreateWaitableTimerExW` with `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`) and records TSC at wake minus TSC at due time;
p50/p99/max in us. `tpause`/`umwait` need WAITPKG and `mwaitx` needs MWAITX; the target has neither, so such waits
could never be validated here. CPUID is checked once in this lot to confirm and record it. Timer resolution is
per-process since Windows 10 2004: the result describes our own process only. The timer call is `fa-win`
(WIN-07, timers part; pinning is already provided by lot 2).

**D6. Orchestration in `fa-probe` (safe).** Pin the probe thread (with the lot 2 `fa-win::thread` guard) on the chosen logical CPU (lot 2 uses `SetThreadAffinityMask`
on the current thread, one processor group; CPU Sets only if a lot needs them), warm up, sample, reject preempted samples (above 1.5x median),
report median and spread, timestamps in TSC ticks. Probes run in the non-elevated `fa` process, so no input crosses
into the elevated collector in this lot.

**D7. PDH reference (WIN-08).** `\Processor Information(*)\% Processor Performance` x nominal frequency, sampled at
1 Hz next to the probe for 60 s under load. Counter names are localized, so use `PdhAddEnglishCounterW`. The catalog
currently says PDH needs no unsafe, but the calls are FFI, so a `fa-win` line is added before the code (catalog
correction). Alternative rejected: `typeperf.exe` (localized names, text parsing).

**D8. Large pages (WIN-09).** `VirtualAlloc` with `MEM_LARGE_PAGES` needs `SeLockMemoryPrivilege`, which only the user
grants by policy; we never grant it ourselves. The probe falls back to 4 KiB pages and reports which size was used.

**D9. Clock-domain check.** Pin the frequency probe on core A while our own load thread runs on core B. If A's
frequency follows B's load, cores share a clock domain; otherwise clocks are per core. The result goes in the catalog.
On per-core-clock CPUs a probe on a core the game does not use only shows package-level throttling; this informs
lots 8 and 10.

**D10. Calibration against HWiNFO (manual, off-game).** `docs/probe-calibration.md`: AC power, best-performance
overlay, no game, background apps closed; 60 s all-core load, idle, then single-core load; note HWiNFO effective clock,
PDH and probe side by side; target agreement 3 % (calibrate). Only the user runs HWiNFO.

**D11. Order of work.** Frequency probe and PDH comparison first (60 s, both curves shown to the user) before any other
probe is written: ASM-02 is the only asm-mandatory module and its accuracy gates the value of the lot.

## Risks / Trade-offs

- [PDH already shows everything the probe shows] -> accuracy only in lot 3; lot 10 decides with Lab soaks.
- [Turbo or thermal change during a 50 us sample] -> median rule, many samples, record variance.
- [PDH smooths over its own window] -> compare means over 1 s windows, not instants.
- [4 KiB pointer chase mixes TLB misses with cache latency] -> large-page variant when available; report both.
- [Missing `sfence` after non-temporal stores] -> `sfence` in the kernel; parity test reads the data after the kernel.
- [asm loses to the reference] -> reference ships, module deleted, numbers kept in the catalog.
- [Windows power throttling of our own process] -> record the power plan; opt our own process out of EcoQoS only.
- [Large-page privilege unavailable] -> 4 KiB fallback, flagged in the result.
