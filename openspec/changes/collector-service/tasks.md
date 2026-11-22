# Tasks

Gate = `powershell -File scripts/gate.ps1`. Each group ends with it, green. Each new unsafe block gets a French
explanation checkpoint (template: `docs/asm-unsafe-catalog.md` section 7), merged only after the user says "compris".

## 1. Threat model (before any code)

- [ ] 1.1 Run the `threat-modeling` skill on the command channel and the trust boundary (pipe, shared memory, install location, user-writable inputs), starting from `docs/architecture.md` section 3; verify the model lists assets, actors, threats and controls.
- [ ] 1.2 Show the model to the user and fix it; verify the user approved it in writing before group 2 starts, and record it as a section of `design.md`.
- [ ] 1.3 Complete the catalog lines SYNC-02 and WIN-04 (and any extra install-side `fa-win` line) before any code; verify the lines exist with "pending".
- [ ] 1.4 Run the gate and verify it is green.

## 2. SYNC-02 seqlock (`fa-sync`)

- [ ] 2.1 Write the seqlock over a fixed-layout region with raw pointers cast to `AtomicU64`, relaxed word access and the fences of design D-5, no `volatile`, no `&T` into the region, `// SAFETY:` comments; verify with unit tests of the protocol.
- [ ] 2.2 Run `cargo +nightly miri test -p fa-sync` with a two-thread writer/reader test; verify Miri reports no data race and no UB.
- [ ] 2.3 Mutation checks: weaken each ordering and each fence in turn, and use plain access instead of atomics, in a scratch copy; verify each mutant fails a test or Miri, then restore (`mutation-verified-testing`).
- [ ] 2.4 Cross-process checksum test (writer process, reader process, 10 s, tight loop); verify zero torn snapshots and record the reader retry rate in the catalog.
- [ ] 2.5 Explanation checkpoint (French) for each memory ordering and fence of the seqlock; verify the user answered "compris".
- [ ] 2.6 Fill SYNC-02 with the justification for no reference crate and the results; verify the line is updated.
- [ ] 2.7 Run the gate and verify it is green.

## 3. Shared memory and pipe (`fa-win`, WIN-04)

- [ ] 3.1 Implement `fa-win::shm` (named mapping, SDDL DACL, RAII, `FILE_MAP_READ` reader); verify with a test that reads the DACL back and compares it to the expected entries, and that the reader view cannot write.
- [ ] 3.2 Implement the header and slot layout (magic, version, size; frametime ring, probe values, status) on top of the seqlock; verify with a test that a reader rejects a wrong magic, version or size.
- [ ] 3.3 Implement `fa-win::pipe` (SDDL DACL, first-instance flag, remote clients rejected, length checked before reading the body); verify with a test that a 100 KiB message closes the connection without parsing and that a 64 KiB message is accepted.
- [ ] 3.4 Extend the OpenProcess source test to the new modules; verify `crates/fa-core/tests/no_open_process.rs` is green and that neither module needs `fa_win::process::open_target`.
- [ ] 3.5 Explanation checkpoint (French) for each new unsafe block (SDDL conversion, mapping and view, pipe handles); verify the user answered "compris".
- [ ] 3.6 Run the gate and verify it is green.

## 4. Command schema and untrusted inputs (`fa-core`)

- [ ] 4.1 Implement the whitelist command enum, MessagePack decoding with `deny_unknown_fields` and the collector-side id table; verify with tests for each command, an unknown variant, an extra field and a PID or path in a field.
- [ ] 4.2 Add a property test (`property-based-testing`): arbitrary bytes never panic and never yield a command with an unissued id; verify it runs in the gate.
- [ ] 4.3 Forged-id scenario: `ApplyProposal` with an unissued id is rejected, logged, nothing applied; verify with an integration test over a real pipe.
- [ ] 4.4 Wire baseline reading through `fa_probe::store::read_baseline` and overrides through `Matrix::with_override`; verify with tests for an oversized file, a malformed file, an unknown field, and an override trying to enable an item.
- [ ] 4.5 Mutation checks on the size caps, the 64 KiB limit and the "may only disable" rule; verify each mutant fails a test.
- [ ] 4.6 Run the gate and verify it is green.

## 5. Collector and installer (`fa-collector`, `fa`)

- [ ] 5.1 Implement `fa-collector` (named mutex single instance, lot 5 capture in observation only, publish to shared memory, serve the pipe, own-process overhead sampling without `OpenProcess`); verify a second instance exits and a test reader sees live frametimes.
- [ ] 5.2 Implement `fa install` / `fa uninstall` (copy to Program Files, ProgramData ACL, `schtasks.exe /Create /XML` with a generated XML pointing only at Program Files, one UAC prompt each); verify in an elevated terminal that the task exists with highest privileges and the right path, and that uninstall removes it.
- [ ] 5.3 Replaced-binary scenario: verify a non-elevated process cannot overwrite the installed `fa-collector.exe` (access denied) and that the ProgramData ACL denies users write.
- [ ] 5.4 Document install, reinstall after a rebuild (running executables are locked, so `fa install` again) and uninstall in the project docs; verify the documented commands run as written.
- [ ] 5.5 Run the gate and verify it is green.

## 6. Overhead budget

- [ ] 6.1 Show the self-measured CPU and working set in the status block; verify with a test run that the values are plausible against Task Manager.
- [ ] 6.2 The user plays 30 minutes with the collector running; Claude reads the published averages; verify under 1 % of one logical CPU and under 50 MB private working set, or record the measured values for recalibration.
- [ ] 6.3 Log any recalibrated value in `docs/decisions.md` section 5 with the measurement; verify the entry exists.
- [ ] 6.4 Run the gate and verify it is green.

## 7. First anti-cheat test (user only)

- [ ] 7.1 Prepare the test: build from a known commit, install, list the enabled items (baseline observers only), give the user the protocol of `docs/compliance.md` section 6; verify the commit and items are written down. Claude does not run, trigger or simulate the test.
- [ ] 7.2 User runs the collector in observe-only mode during at least 3 unranked matches per matrix game and reports; verify the user's report is in hand for each game.
- [ ] 7.3 Claude records the user's result in the `docs/compliance.md` section 6 table and updates `compliance.toml` only on the user's word (clean result: link to the record; any warning, kick, error or notice: `forbidden` for that game); verify the record and the user's go. Riot is never described as having approved anything.
- [ ] 7.4 Run the gate and verify it is green.

## Workflow follow-up

- [ ] Run `openspec validate collector-service --strict` and the gate, then archive the change after the user's go; verify both are green.
