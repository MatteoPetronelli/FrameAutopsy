# Tasks

Gate = `powershell -File scripts/gate.ps1`. Skills to load: `rust-cargo`, `ponytail`; `rust-unsafe` for group 4.

## 1. Workspace and skeleton crates

- [x] 1.1 Create the root `Cargo.toml` (workspace, `members = ["crates/*"]`, edition 2021, `[profile.dev] debug =
  "line-tables-only"`, `[profile.dev.package."*"] debug = false`); verify `cargo metadata --no-deps` succeeds.
- [x] 1.2 Create `crates/fa-core` with `#![forbid(unsafe_code)]`; verify `cargo build -p fa-core`.
- [x] 1.3 Create `crates/fa-asm`, `crates/fa-win`, `crates/fa-sync` with `#![deny(unsafe_op_in_unsafe_fn)]`,
  `#![deny(clippy::undocumented_unsafe_blocks)]`, `#![deny(clippy::multiple_unsafe_ops_per_block)]`; verify
  `cargo clippy --workspace --all-targets -- -D warnings` is clean and `ls crates` shows exactly these four crates.
- [x] 1.4 Verify the disk-hygiene spec: `cargo build -v` shows no debuginfo for a dependency and line tables only for
  a workspace crate.
- [x] 1.5 Run the gate (`powershell -File scripts/gate.ps1`) once group 2 provides it; record the green output.

## 2. Gate and ignore files

- [x] 2.1 Write `scripts/gate.ps1`: fmt check, clippy `-D warnings`, test, header check (lint headers present),
  guarded Miri step (only when `fa-sync` has tests), guarded Lab-scorer step (only when `fa lab score` exists), then
  `graphify update .`. Stop at the first failure with a non-zero exit code (explicit `$LASTEXITCODE` check per step).
  Verify `powershell -File scripts/gate.ps1` exits 0 on the clean workspace.
- [x] 2.2 Verify the "Clippy warning" scenario: add a deliberate clippy warning to `fa-core`, run the gate, show it
  exits non-zero **before** `cargo test` runs, revert.
- [x] 2.3 Write `.gitignore` (`target/`, `graphify-out/`, `*.etl`, `*.jfr`, local data directory, Lab temporary files,
  `profile.md`) and `.graphifyignore` (`target/`, `graphify-out/`, data directories). Verify with
  `git check-ignore -v profile.md target graphify-out` and `git status --short` showing no `target/` and no
  `profile.md`.
- [x] 2.4 Verify the "Personal profile" scenario: `git add -A --dry-run` does not list `profile.md`.
- [x] 2.5 Run the gate (`powershell -File scripts/gate.ps1`) and confirm green.

## 3. Mandatory proof: the gate fails on a deliberate `unsafe` in fa-core

- [x] 3.1 Add `pub fn probe() -> u8 { unsafe { core::mem::zeroed() } }` to `crates/fa-core/src/lib.rs`. Run
  `powershell -File scripts/gate.ps1` and **show the full output**: expected compile error on the forbidden `unsafe`
  block and a non-zero exit code (verifies "Unsafe in a safe crate").
- [x] 3.2 Add an `unsafe` block without `// SAFETY:` to a temporary function in `crates/fa-win/src/lib.rs`. Run the
  gate and show Clippy failing on `undocumented_unsafe_blocks` (verifies "Missing SAFETY comment"). Then put two
  unsafe operations in one block and show `multiple_unsafe_ops_per_block` failing.
- [x] 3.3 Revert both probes (`git diff` shows no residue in `fa-core` and `fa-win`), run the gate and **show the green
  output** with exit code 0.
- [x] 3.4 Mutation-check the header guard (skill `mutation-verified-testing`): temporarily remove
  `forbid(unsafe_code)` from `fa-core`, confirm the gate fails, restore it.
- [x] 3.5 Run the gate (`powershell -File scripts/gate.ps1`) and confirm green.

## 4. Counting allocator and bench harness (`fa-sync`, SYNC-00)

- [ ] 4.1 Re-read the SYNC-00 line of `docs/asm-unsafe-catalog.md` (written before the code) and confirm it matches
  what is built; fix the catalog line first if not. Verify the line exists with verdict "needed".
- [ ] 4.2 Implement `CountingAlloc` (`unsafe impl GlobalAlloc`, delegating to `System`, relaxed atomic counters, no
  allocation inside the allocator) behind the `test-support` feature, with a safe API (`count_allocs`,
  `assert_no_alloc`). One `unsafe` operation per block, each with an uppercase `// SAFETY:` comment. Verify the crate
  builds with and without `--features test-support` and Clippy passes with the three unsafe lints.
- [ ] 4.3 Add the self-test in a dedicated integration-test binary with `#[global_allocator]`: a `Vec::push` past
  capacity is counted, a push into a pre-sized `Vec` counts zero. Verify the "Hidden allocation" scenario with
  `cargo test -p fa-sync --features test-support`, and mutation-check by making the counter not increment on `alloc`
  (the test must fail), then restore.
- [ ] 4.4 Add `criterion` as dev-dependency of `fa-asm` and `fa-sync` with `[[bench]] harness = false` and one trivial
  bench named `<module>_vs_<reference>` that prints reference and candidate side by side. Verify
  `cargo bench -p fa-sync --no-run` and `cargo bench -p fa-asm --no-run` both link.
- [ ] 4.5 **Explanation checkpoint (French)**: explain SYNC-00 to the user using catalog section 7 (ce que ca fait,
  pourquoi c'est correct, ce qui le casserait, comment on le verifie): why delegating to `System` preserves the layout
  contract, why the allocator must not allocate, why `Relaxed` is enough for counters. Do not merge the group until the
  user answers "compris". Verify the user's "compris" is in the session.
- [ ] 4.6 Run the gate (`powershell -File scripts/gate.ps1`) and confirm green, with the `test-support` tests included
  (wire the feature into the gate's test step if it is not enabled by default).

## 5. Documentation, skill and OpenSpec changes 0 to 4

- [ ] 5.1 Write the short `README.md` (purpose, build, gate, `cargo sweep --time 14`, link to `CLAUDE.md`). Verify the
  documented commands run as written.
- [ ] 5.2 Create the project skill `.claude/skills/frameautopsy-invariants/SKILL.md` (unsafe confinement,
  earn-its-place, zero allocation, gate), modelled on `riftmind-invariants` without copying private content. Verify
  the frontmatter parses and the skill is listed.
- [ ] 5.3 Update `CLAUDE.md` status (lot 0 done, `Cargo.toml` exists, gate now runnable). Verify the "NOT runnable yet"
  note is replaced by a runnable command.
- [ ] 5.4 Verify `openspec list` shows `bootstrap-workspace`, `compliance-matrix`, `clock-and-topology`,
  `driverless-probes`, `health-check`, and `openspec validate --all --strict` passes.
- [ ] 5.5 Build the Graphify graph (`graphify update .`) and confirm `graphify-out/` is ignored by git. Run the gate
  (`powershell -File scripts/gate.ps1`) and confirm green.

## 6. Integration check (lot done criteria)

- [ ] 6.1 Gate green (`powershell -File scripts/gate.ps1`, exit 0), `git status --short` shows neither `target/` nor
  `profile.md`, `openspec list` shows changes 0 to 4, and the workspace contains only the four skeleton crates
  (verifies "Lot 0 workspace").

## Workflow follow-up

- Commit in small Conventional Commits on branch `lot-0/bootstrap-workspace` when the user asks.
- After the user's "compris" on SYNC-00 and review, archive the change with `openspec-archive-change`.
