# Tasks

Gate = `powershell -File scripts/gate.ps1` (headers, fmt, clippy -D warnings, test --workspace, miri, graphify update).
Each group below ends with it, green.

## 1. Catalog and dependencies

- [ ] 1.1 Update SIMD-01 in `docs/asm-unsafe-catalog.md` (metric: compressed bytes, reference "delta + zstd", conditional); verify the line is present with "pending".
- [ ] 1.2 Create `fa-storage` (`#![forbid(unsafe_code)]`) and add `rusqlite` (bundled), `rmp-serde`, `zstd`, `serde`; verify `cargo build --workspace` succeeds.
- [ ] 1.3 Run the gate and verify it is green.

## 2. Codec measurement (before any codec)

- [ ] 2.1 Extract real snapshots from lot 5 captures (admin terminal, user runs the capture) as test data under the size limit; verify at least 5 snapshots of 12 s exist.
- [ ] 2.2 Measure compressed bytes and time for zstd alone, delta + zstd, delta-of-delta + zigzag + scalar varint + zstd; verify the table is pasted in SIMD-01 with date and commit.
- [ ] 2.3 Decide: a stage ships only with at least 5 % fewer bytes than delta + zstd; verify the verdict is written in the catalog and group 6 is marked skipped if not.
- [ ] 2.4 Run the gate and verify it is green.

## 3. Window, trigger, snapshot (`fa-core`)

- [ ] 3.1 Implement the const-generic window ring (frames, aggregated series, probe samples); verify with tests for wrap-around and the 10 s span.
- [ ] 3.2 Implement the trigger (2 x median and median + 4 ms, 1 s coalescing); verify with fixture tests at 144 FPS and 30 FPS and mutation checks on each term (`mutation-verified-testing`).
- [ ] 3.3 Implement the snapshot pool (copy pre-trigger 10 s, append 2 s, drop and count when exhausted); verify a test shows -10 s to +2 s content and a test for pool exhaustion.
- [ ] 3.4 Prove the hot path allocates nothing with `fa_sync::counting::assert_no_alloc` (feature `test-support`) around push, trigger and slot copy; verify the test passes.
- [ ] 3.5 Run the gate and verify it is green.

## 4. Storage (`fa-storage`)

- [ ] 4.1 Implement schema, WAL, MessagePack + zstd blobs, insert and list; verify a round-trip test on a snapshot.
- [ ] 4.2 Implement retention (30 days AND 2 GB, oldest first, incremental vacuum); verify with a cap test using synthetic incidents and one full 2 GB run, size measured db + WAL.
- [ ] 4.3 Implement the session summary (avg FPS, 1 % low, 0.1 % low, P99, stutter count per 10 min); verify against fixtures with known values.
- [ ] 4.4 Verify no raw continuous trace is written: a test lists the files in the store folder after a run.
- [ ] 4.5 Run the gate and verify it is green.

## 5. In-game probe sampling

- [ ] 5.1 Implement the CPU choice (no game hot thread, SMT sibling not busy) as a pure function; verify with unit tests on SMT, non-SMT and hybrid fixtures and mutation checks on the sibling rule.
- [ ] 5.2 Implement the schedule capping the duty cycle (at most 0.01 % per CPU, about 2 frequency samples per second); verify with a test of the budget arithmetic.
- [ ] 5.3 Measure the real duty cycle in a Lab run (admin terminal, user runs it); verify at most 0.01 % per CPU and log the value in `docs/decisions.md` section 5.
- [ ] 5.4 Run the gate and verify it is green.

## 6. Conditional codec (only if task 2.3 says it ships)

- [ ] 6.1 (conditional) Implement delta-of-delta + zigzag + scalar varint in safe Rust; verify a round-trip property test (`property-based-testing`).
- [ ] 6.2 (conditional) Implement Stream VByte `pshufb` intrinsics in `fa-asm` with `FA_FORCE_SCALAR=1` fallback; verify parity with the scalar reference and a recorded bench.
- [ ] 6.3 (conditional) Write the asm candidate only if intrinsics lose; verify parity and a bench; delete the loser if the win is under 5 %.
- [ ] 6.4 (conditional) Explanation checkpoint (French) for each new unsafe/asm block; verify the user answered "compris".
- [ ] 6.5 Run the gate and verify it is green.

## 7. Closing

- [ ] 7.1 Log calibrated values in `docs/decisions.md` section 5 and note the read-only WAL risk for lot 9 in `handoff.md`; verify the entries exist.
- [ ] 7.2 Run `openspec validate incident-store --strict` and the gate; verify both are green.

## Workflow follow-up

- Archive the change after the user validates the results and the branch is merged (`openspec-archive-change`).
