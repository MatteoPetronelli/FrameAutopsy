# Tasks

Gate = `powershell -File scripts/gate.ps1` (fmt, clippy -D warnings, test --workspace, graphify update). Each group
below ends with it, green.

## 1. Catalog and dependencies

- [ ] 1.1 Update ASM-06 in `docs/asm-unsafe-catalog.md` (chain count, metric, reference) and add WIN-10 (Toolhelp snapshot, power reads) before any code; verify both lines are present with "pending".
- [ ] 1.2 Add dependencies `wmi`, `nvml-wrapper`, `serde`, `serde_json` to the crates that use them and verify `cargo build --workspace` succeeds.
- [ ] 1.3 Run the gate and verify it is green.

## 2. ASM-06 FMA power load (`fa-asm`)

- [ ] 2.1 Write the intrinsics reference (AVX2 and AVX-512 behind runtime dispatch, scalar fallback with `FA_FORCE_SCALAR=1`) and verify the dispatch test selects the expected variant on this machine.
- [ ] 2.2 Write the parity test (same FLOP count, same final accumulators) and verify it passes for AVX2, AVX-512 and scalar.
- [ ] 2.3 Write the `asm!` candidate (Intel syntax, 8 independent chains, `// SAFETY:` comments, precise `options`) and verify the parity test passes against the reference.
- [ ] 2.4 Write `cargo bench -p fa-asm --bench fma_load_vs_intrinsics` (FLOP per TSC tick, 3 runs, pinned thread, AC power); paste results, verdict and date into ASM-06; verify the loser is deleted if the win is under 5 %.
- [ ] 2.5 Explanation checkpoint (French): present the asm block to the user (ce que ca fait, pourquoi c'est correct, ce qui le casserait, comment on le verifie, template catalog section 7); verify the user answered "compris" before merge. Skipped with the block if intrinsics win.
- [ ] 2.6 Run the gate and verify it is green.

## 3. Windows queries (`fa-win`, WIN-10)

- [ ] 3.1 Implement the Toolhelp process-name listing with an RAII snapshot guard and no `OpenProcess`; verify with a test against the running test process name and the extended source test forbidding `OpenProcess` outside `open_target`.
- [ ] 3.2 Implement read-only power state (AC or battery, active scheme name, effective overlay); verify with a test on the type conversions and a manual run printing the three values.
- [ ] 3.2b Minecraft detection (see design Decisions): FIRST ask the user to launch Minecraft and read its real window title (and a `javaw.exe` IDE/other Java app as negative case); only then implement `EnumWindows` + `GetWindowThreadProcessId` matching. Verify with a pure title-matching unit test (Minecraft titles match, IDE/other titles do not) and a manual run: `javaw.exe` without a Minecraft window does not block `fa check`.
- [ ] 3.3 Explanation checkpoint (French) for each new `unsafe` block; verify the user answered "compris".
- [ ] 3.4 Run the gate and verify it is green.

## 4. Safe probes: WMI and NVML

- [ ] 4.1 Implement the `Win32_PhysicalMemory` query on a COM thread with a 3 s timeout and "inconnu" on failure; verify with a unit test on result parsing and a manual run on the laptop.
- [ ] 4.2 Implement the optional NVML read (name, P-state, power limit) with absent-DLL handling; verify with a unit test for the absent case and a manual run on the laptop.
- [ ] 4.3 Run the gate and verify it is green.

## 5. Verdicts and report (`fa-probe`)

- [ ] 5.1 Implement the verdict rules (throttling, single-channel RAM likely, scheduling interference likely) with evidence and confidence; verify with fixture tests for "bridage apres 25 s, -26 %" and for the healthy desktop.
- [ ] 5.2 Add mutation checks on each threshold (10 %, 60 %, 500 us) and the first-5 s window, and verify each mutant fails a test (`mutation-verified-testing`).
- [ ] 5.3 Implement the `HealthReport` struct, the French text renderer (including the honest "nothing found" listing of checks) and the JSON writer; verify with golden-file tests for both outputs.
- [ ] 5.4 Implement the AC baseline (`health\baseline.json`, atomic rename, never from a battery run) and the strict reader (size cap, `deny_unknown_fields`); verify with tests for AC, battery and an oversized or malformed file.
- [ ] 5.5 Run the gate and verify it is green.

## 6. `fa check` command (`fa`)

- [ ] 6.1 Wire `fa check`: refusal when a matrix game is running (message asking to close it, exit non-zero), battery warning, phase sequencing under an 85 s deadline, Ctrl-C handling; verify with a test using a fake process list for `League of Legends.exe` and a manual run.
- [ ] 6.2 Verify `fa check` runs non-elevated, changes no setting (before/after power scheme and overlay compared), and finishes within 90 s (timed manual run and a test of the deadline logic).
- [ ] 6.3 Document `fa check` usage and report format in the project docs and verify the documented command runs as written.
- [ ] 6.4 Run the gate and verify it is green.

## 7. Real runs and threshold freeze

- [ ] 7.1 Run `fa check` on the user's laptop on AC power and keep the French report and JSON; verify the run completed in 90 s or less.
- [ ] 7.2 Run `fa check` on the user's laptop on battery and keep the report and JSON; verify the battery warning appeared and the baseline file was not overwritten.
- [ ] 7.3 Show both reports to the user BEFORE freezing any threshold; verify the user's feedback is recorded and apply the agreed threshold changes.
- [ ] 7.4 Log the final thresholds, the phase order choice and the HWiNFO comparison result (if done) in `docs/decisions.md` section 5, and paste both reports in the PR description; verify the entries exist.
- [ ] 7.5 Run `openspec validate health-check --strict` and the gate; verify both are green.

## Workflow follow-up

- Archive the change after the user validates the reports and the branch is merged (`openspec-archive-change`).
