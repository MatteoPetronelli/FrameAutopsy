# Proposal

## Why

FrameAutopsy has docs, OpenSpec and a Claude setup but no code. Before any business logic, the repository needs a
workspace that stays small on disk, a single local gate, and the `unsafe`/asm policy enforced by the compiler and Clippy
rather than by discipline. Doing it first means every later lot is checked from its first line.

- **workspace-hygiene** (Purpose): A Cargo workspace that stays small on disk, runs one local gate, and never commits
  traces or personal files.
- **unsafe-policy** (Purpose): Make `unsafe` and assembly earn their place, confine them to three crates, and keep every
  block explainable.

## What Changes

- Add a Cargo workspace (edition 2021) with four skeleton crates under `crates/`: `fa-core` (`#![forbid(unsafe_code)]`),
  and `fa-asm`, `fa-win`, `fa-sync` (unsafe lint headers, safe public API). No other crate is created.
- Add the disk-hygiene profile settings (`debug = "line-tables-only"`, dependencies without debug info).
- Add `.gitignore` (including `profile.md`) and `.graphifyignore`.
- Add `scripts/gate.ps1` (fmt, clippy `-D warnings`, test, then miri/Lab scorer once their prerequisites exist).
- Add the counting global allocator in `fa-sync` behind the `test-support` feature, and the `criterion` bench harness
  in `fa-asm` and `fa-sync`.
- Add a short `README.md` (documents `cargo sweep --time 14`), update `CLAUDE.md` status, and create the project skill
  `frameautopsy-invariants`.
- OpenSpec: `openspec init` is already done (`openspec/` exists, CLI 2.0.0) and `openspec/config.yaml` is in place.
  This lot creates the changes for lots 0 to 4 from `specifications.md`.
- No breaking change: nothing exists yet to break.

## Capabilities

### New Capabilities
- `workspace-hygiene`: workspace layout, disk hygiene, ignored files, the local gate, Graphify configuration, the
  allocation test harness and the benchmark harness.
- `unsafe-policy`: confinement of `unsafe`, lints and `// SAFETY:` comments, safe public API, earn-its-place rule,
  runtime dispatch, assembly form, no unwinding across FFI, Miri coverage, explanation checkpoint.

### Modified Capabilities
<!-- None: no spec exists yet under openspec/specs/. -->

## Impact

- **Compliance**: no effect on `compliance.toml` (it does not exist before lot 1) and none on the "never touch the
  game" boundary. Lot 0 adds no data source, no action, no process handle, no network access.
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): no asm or SIMD module. One test-only unsafe module is added in
  `fa-sync`: `SYNC-00` (counting global allocator, `unsafe impl GlobalAlloc`, `test-support` feature), already listed in
  the ledger with verdict "needed". `fa-asm` and `fa-win` stay skeletons (no `unsafe`, no catalog entry).
- **Code**: new `Cargo.toml`, `crates/*`, `scripts/gate.ps1`, `.gitignore`, `.graphifyignore`, `README.md`,
  `.claude/skills/frameautopsy-invariants/`; `CLAUDE.md` status line updated.
- **Dependencies**: `criterion` (dev-dependency, `fa-asm` and `fa-sync`) only.
- **Tooling**: requires stable Rust, nightly with Miri (used from lot 5), `cargo-sweep`.
