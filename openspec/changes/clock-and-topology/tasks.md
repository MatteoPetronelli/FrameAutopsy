# Tasks

## 1. Catalog line and skeleton

- [ ] 1.1 Re-read and, if needed, complete the ASM-01 line in `docs/asm-unsafe-catalog.md` BEFORE any code; verify it lists reference, metric, parity test, fallback and failure mode.
- [ ] 1.2 Confirm AVX-512, WAITPKG and MWAITX status by CPUID on the target and note it in catalog §3; verify the printed flags match the note.
- [ ] 1.3 Create the `fa-asm::tsc` module skeleton with the `now()` / `now_serialized()` signatures and the `FA_FORCE_SCALAR` switch; verify `cargo build -p fa-asm` passes.
- [ ] 1.4 Run the gate: `powershell -File scripts/gate.ps1` is green.

## 2. Intrinsics reference version

- [ ] 2.1 Implement `now()` and `now_serialized()` with `_mm_lfence` + `_rdtsc` / `__rdtscp`, each `unsafe` block carrying `// SAFETY:`; verify clippy (`undocumented_unsafe_blocks`) is clean.
- [ ] 2.2 Add CPUID invariant-TSC detection (`0x80000007:EDX[8]`) with QPC fallback mode; verify a unit test covers both branches (flag injected).
- [ ] 2.3 Add tests: monotonic reads and the same-tick-within-overhead parity helper; verify `cargo test -p fa-asm` passes.
- [ ] 2.4 Run the gate: `powershell -File scripts/gate.ps1` is green.

## 3. Frequency, conversions, QPC mapping

- [ ] 3.1 Implement TSC frequency from CPUID `0x15` (non-zero ratio) and QPC calibration (5 runs >= 200 ms, median); verify tests cover both paths and the CPUID/calibration gap is logged.
- [ ] 3.2 Implement u128 integer tick-to-ns conversion; verify a property test against a reference, including boundary values.
- [ ] 3.3 Implement the QPC<->TSC mapping (offset, scale), 60 s re-estimation and recorded drift; verify a test with an injected fake clock checks re-estimation and drift value.
- [ ] 3.4 Store the frequency in the health baseline structure; verify a round-trip test.
- [ ] 3.5 Run the gate: `powershell -File scripts/gate.ps1` is green.

## 4. Topology

- [ ] 4.1 Implement the safe topology model and the `&[u8]` parser for `GetSystemCpuSetInformation` and `GetLogicalProcessorInformationEx` in `fa-win::topology`, with bounds-checked record sizes; verify truncated or malformed buffers return an error and never panic.
- [ ] 4.2 Add synthetic fixtures (hybrid P+E, dual-CCD) under `lab/fixtures/`; verify E-cores get a lower efficiency class and two LLC domains are reported.
- [ ] 4.3 Add the real-API test on the target; verify 6 cores, 12 threads, one LLC.
- [ ] 4.4 Add the CPUID cross-check test (`0x0B`/`0x1F`, `0x1A`, `0x8000001E`, `0x04`/`0x8000001D`) logging mismatches; verify it runs and skips leaves absent on the vendor.
- [ ] 4.5 Run the gate: `powershell -File scripts/gate.ps1` is green.

## 5. Dev command and accuracy criteria

- [ ] 5.1 Implement `fa clock` (non-elevated): invariance, frequency and source, TSC vs QPC gap, per-core skew, topology; verify it runs without admin and prints every field.
- [ ] 5.2 Pinned-thread cross-core test: readings monotonic and skew < 1 µs (calibrate); verify on the target and record the value.
- [ ] 5.3 10 s TSC vs QPC test: gap < 10 µs (calibrate); verify on the target and record the value.
- [ ] 5.4 Run the gate: `powershell -File scripts/gate.ps1` is green.

## 6. ASM-01 asm version and explanation checkpoint (before the bench)

- [ ] 6.1 Write the `asm!` version of `now()` / `now_serialized()` (Intel syntax, precise `options`, documented clobbers) with `// SAFETY:`; verify the parity test against the intrinsics version passes and `FA_FORCE_SCALAR=1` selects the reference.
- [ ] 6.2 French explanation checkpoint (catalog §7 template: ce que ça fait, pourquoi c'est correct, ce qui le casserait, comment on le vérifie), covering every asm line incl. `lfence`, `rdtsc` vs `rdtscp` and the EDX:EAX/ECX clobbers; verify the user answers "compris" BEFORE the bench is run.
- [ ] 6.3 Run the gate: `powershell -File scripts/gate.ps1` is green.

## 7. ASM-01 bench and verdict

- [ ] 7.1 Add `cargo bench -p fa-asm --bench tsc_vs_intrinsics` (overhead in cycles, jitter stdev) following catalog §2 (AC power, pinned core, 3 runs, median and spread, `FA_FORCE_SCALAR=1` run); verify results are produced.
- [ ] 7.2 Record numbers, date, commit and verdict (win only if > 5 % and above the spread, else tie or loss) in the ASM-01 row; verify the row status is no longer `pending`.
- [ ] 7.3 If asm lost or tied, delete the asm module and keep the intrinsics reference; verify build and tests still pass and the catalog keeps the numbers.
- [ ] 7.4 Run the gate: `powershell -File scripts/gate.ps1` is green, then `openspec validate clock-and-topology --strict`.

## Workflow follow-up

- Validate the lot with the user, then archive the change (`openspec-archive-change`).
- Log any recalibrated `(calibrate)` value in `docs/decisions.md` §5.
