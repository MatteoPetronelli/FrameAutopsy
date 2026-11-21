# Proposal

## Why

Monitoring tools read effective clock, memory behaviour and wake-up jitter through kernel drivers, which anti-cheats
(Vanguard) block. FrameAutopsy needs the same facts from user mode, with no driver, to explain stutters (throttling,
slow memory, timer jitter) and to feed the `fa check` verdict (lot 4) and the Lab (lot 6).

Purpose of the capability: measure from user mode, with no driver, what monitoring tools usually read through drivers
(which Vanguard blocks): effective clock, memory latency and bandwidth, cache sizes, thread wake-up latency. Probes run
in FrameAutopsy's own threads only.

Lot 3 follows `tsc-clock` and `cpu-topology` (lot 2): probes timestamp in TSC ticks and pin on CPUs chosen from the
topology.

## What Changes

- New `fa-asm` kernels, each with a reference, a parity test, a bench and a runtime fallback:
  - ASM-02: effective frequency probe (chain of dependent `add`, asm-mandatory because the compiler folds such a chain);
  - ASM-03: pointer chase for memory latency (asm-candidate vs Rust `read_volatile` chain);
  - ASM-04: bandwidth kernels, AVX2 loads, `vmovntdq` non-temporal stores, copy (asm-candidate vs intrinsics).
- New `fa-win` pieces: high-resolution waitable timer (WIN-07 timers part; pinning already exists from lot 2); large-page allocation (WIN-09) and
  PDH reader (WIN-08), both new catalog lines added before their code (see design D7, D8).
- New safe crate `fa-probe` (`#![forbid(unsafe_code)]`): orchestration, pinning, rejection of preempted samples,
  cache curve, wake-up latency probe (no asm), PDH comparison, clock-domain check.
- Dev command `fa probe` (non-elevated) to run each probe and print the PDH comparison.
- Manual calibration procedure against HWiNFO (off-game), results recorded in `docs/asm-unsafe-catalog.md`.
- Lot 3 measures ASM-02 accuracy only. The keep/remove decision on the frequency probe is taken at lot 10.

## Capabilities

### New Capabilities
- `driverless-probes`: pinned, driverless probes for effective frequency, memory latency and bandwidth, cache curve and
  wake-up latency, plus the calibration record.

### Modified Capabilities

## Impact

- **Compliance**: no change to `compliance.toml`. Probes run only in our own threads and off-game only; they open no
  handle on a game process and read no game data. In-game sampling is out of scope (lot 8, `incident-store`). The
  "never touch the game" boundary is unchanged.
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): ASM-02, ASM-03, ASM-04 (`fa-asm`); WIN-07 timers part (`fa-win`, pinning done in lot 2); new lines
  WIN-08 (PDH) and WIN-09 (large pages). Results and verdicts are filled at the end of the lot.
- **Crates**: new `fa-probe` (safe); touches `fa-asm` and `fa-win`; `fa` gains a `probe` subcommand.
- **Dependencies**: no new crate expected beyond `windows`/`windows-sys` features (PDH, memory, threading) and
  `criterion` (dev) already planned.
