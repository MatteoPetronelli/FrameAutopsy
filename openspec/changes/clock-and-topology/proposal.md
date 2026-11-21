# Proposal

## Why

One high-resolution timebase is needed for probes, ETW events and reports. QPC runs at 10 MHz (100 ns resolution) on
current Windows; probes need sub-microsecond precision over ~50 µs windows, so the TSC (~0.3 ns) is the base.
Probes, the collector and the CPU Set governor also need to know cores, SMT siblings, last-level-cache domains and
efficiency classes, to pin probes and to place background processes. Lot 2 delivers both, and the first asm blocks
of the project (ASM-01), which must earn their place against an intrinsics reference.

## What Changes

- `fa-asm::tsc`: `now()` (`lfence; rdtsc; lfence`) and `now_serialized()` (`rdtscp; lfence`), first as an intrinsics
  reference, then as an `asm!` candidate; the asm ships only if it wins the ASM-01 bench.
- Clock layer: invariant TSC detection (CPUID `0x80000007:EDX[8]`) with QPC fallback, TSC frequency (CPUID `0x15` or
  QPC calibration), integer tick-to-ns conversion, QPC<->TSC mapping re-estimated every 60 s.
- `fa-win::topology`: logical CPU -> physical core -> LLC domain -> efficiency class model built from
  `GetSystemCpuSetInformation` and `GetLogicalProcessorInformationEx`; CPUID cross-check as a test only; synthetic
  buffers for hybrid and dual-CCD cases.
- Dev command `fa clock` (non-elevated) printing invariance, frequency, TSC/QPC gap and topology.
- Catalog: ASM-01 line confirmed before the code, bench verdict recorded at the end of the lot.

## Capabilities

### New Capabilities
- `tsc-clock`: invariant-TSC timebase, read primitives, frequency, integer conversions, QPC mapping.
- `cpu-topology`: CPU model from Windows APIs (cores, SMT, LLC domains, efficiency classes), CPUID cross-check test,
  synthetic topologies.

### Modified Capabilities

None.

## Impact

- Effect on `compliance.toml` and the "never touch the game" boundary: none. The change reads the TSC, CPUID and
  system topology from our own threads; it opens no handle on any process, adds no data source and no action, and
  needs no matrix row.
- Catalog modules (`docs/asm-unsafe-catalog.md`): ASM-01 (`fa-asm::tsc`, asm-candidate, status pending -> verdict) and
  `fa-win::topology` (safe model over FFI; a WIN-xx ledger line is added if the buffer walking needs `unsafe`).
- Code: `fa-asm`, `fa-win`, `fa-core` (clock and topology types), `fa` (`fa clock`); `lab/fixtures/` synthetic buffers.
- Dependencies: none beyond what lot 0 already provides (`windows`/`windows-sys`, `criterion` as dev-dependency).
- Consumed later by lot 3 (probes), lot 5 (ETW raw TSC timestamps), lot 12 (CPU Sets).
