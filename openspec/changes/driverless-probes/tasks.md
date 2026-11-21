# Tasks

Each group ends with the gate (`powershell -File scripts/gate.ps1`). Each new asm/unsafe block gets an explanation
checkpoint in French (template: `docs/asm-unsafe-catalog.md` section 7), merged only after the user says "compris".

## 1. Catalog lines and CPU features

- [x] 1.1 Fill the catalog lines for ASM-02, ASM-03, ASM-04, the WIN-07 timers part and add WIN-08 (PDH reader) and WIN-09 (large pages) before any code; verify the lines exist in `docs/asm-unsafe-catalog.md`.
- [x] 1.2 Create crate `fa-probe` (`#![forbid(unsafe_code)]`) and register it in the workspace; verify `cargo build -p fa-probe`.
- [x] 1.3 Confirm by CPUID that WAITPKG and MWAITX are absent on the target machine and record it in the catalog; verify with a test that reads the feature bits and the recorded note.
- [x] 1.4 Gate: `powershell -File scripts/gate.ps1` is green.

## 2. Frequency probe and PDH comparison (first, before other probes)

- [ ] 2.1 Write the Rust `black_box` reference chain and the PDH reader (`fa-win`, WIN-08, English counter names); verify with a test that PDH returns a plausible value on this machine.
- [ ] 2.2 Implement ASM-02 in `fa-asm` (dependent `add` chain of about 50 us, TSC-bracketed) with scalar fallback and `FA_FORCE_SCALAR=1`; verify the parity test (result within 3 % of the reference) passes.
- [ ] 2.3 Implement `fa-probe` frequency orchestration: pin on own thread (WIN-07), reject samples above 1.5x median; verify with a unit test on synthetic samples including preempted ones.
- [ ] 2.4 Run the probe and PDH together for 60 s under steady load and export both curves; show them to the user before writing any other probe; verify the user accepted the curves and the measured agreement is recorded in the catalog.
- [ ] 2.5 French explanation checkpoint for ASM-02, then "compris" from the user; verify the approval is noted in the catalog line.
- [ ] 2.6 Gate: `powershell -File scripts/gate.ps1` is green.

## 3. Pinning, clock domain, wake-up latency

- [ ] 3.1 Implement the high-resolution waitable timer in `fa-win` (WIN-07, timers part) with an RAII wrapper; reuse the lot 2 pinning guard; verify in a test that a timer wait wakes within a plausible bound and that the probe thread is pinned on the requested CPU.
- [ ] 3.2 Implement the wake-up latency probe in `fa-probe` (no asm), p50/p99/max in us; verify with a test on a synthetic delay distribution and a real run with plausible values.
- [ ] 3.3 Implement the clock-domain check (probe on core A while a load runs on core B) and record the domain type of the machine in the catalog; verify with a test of the decision logic on synthetic data and the recorded result.
- [ ] 3.4 French explanation checkpoint for the new unsafe blocks of the WIN-07 timers, then "compris".
- [ ] 3.5 Gate: `powershell -File scripts/gate.ps1` is green.

## 4. Memory latency and cache curve

- [ ] 4.1 Implement the Sattolo permutation in `fa-probe` (safe); verify with a test that the result is one single cycle for several sizes and seeds.
- [ ] 4.2 Implement large-page allocation in `fa-win` (WIN-09) with 4 KiB fallback; verify the fallback path in a test and that the result reports the page size used.
- [ ] 4.3 Implement the Rust `read_volatile` chain (reference) and ASM-03 in `fa-asm` with fallback; verify the parity test (same final pointer, ns/load within 2 %) passes.
- [ ] 4.4 Run `cargo bench -p fa-asm --bench pointer_chase_vs_read_volatile` (3 runs, median and spread, catalog section 2 protocol) and paste the coefficient of variation into the catalog; verify the ship-or-delete verdict is recorded.
- [ ] 4.5 Implement the cache curve (16 KiB to 256 MiB), plateau detection and comparison with `cpu-topology`; verify with a test on synthetic curves and a real run compared with the topology.
- [ ] 4.6 French explanation checkpoint for ASM-03 (if kept) and WIN-09, then "compris".
- [ ] 4.7 Gate: `powershell -File scripts/gate.ps1` is green.

## 5. Memory bandwidth

- [ ] 5.1 Write the intrinsics reference (AVX2 loads, `_mm256_stream_si256`, copy) with SSE2 and scalar fallbacks; verify the parity tests (same bytes, same checksum).
- [ ] 5.2 Implement ASM-04 in `fa-asm` (AVX2 loads, `vmovntdq` plus `sfence`, copy); verify the parity test against the reference.
- [ ] 5.3 Implement multi-threaded orchestration across physical cores in `fa-probe`; verify with a test of the aggregation logic and a real run showing scaling then saturation.
- [ ] 5.4 Run `cargo bench -p fa-asm --bench bandwidth_vs_intrinsics` and paste GB/s into the catalog; verify the verdict (kept only with a win of 5 % or more, otherwise module deleted and numbers kept).
- [ ] 5.5 French explanation checkpoint for ASM-04 (if kept), then "compris".
- [ ] 5.6 Gate: `powershell -File scripts/gate.ps1` is green.

## 6. CLI, calibration and close

- [ ] 6.1 Add the `fa probe` subcommand (non-elevated, off-game only); verify with a command test and a manual run.
- [ ] 6.2 Write `docs/probe-calibration.md` (HWiNFO procedure, design D10); verify the user can follow it as written.
- [ ] 6.3 User runs the calibration; record probe, PDH and HWiNFO results in `docs/asm-unsafe-catalog.md`; verify the ASM-02 accuracy is noted and the keep/remove decision is explicitly deferred to lot 10.
- [ ] 6.4 Check that every probe has its reference, parity test and catalog result; verify `openspec validate driverless-probes --strict` passes.
- [ ] 6.5 Gate: `powershell -File scripts/gate.ps1` is green.

## Workflow follow-up

- Archive the change after the user approves the lot.
