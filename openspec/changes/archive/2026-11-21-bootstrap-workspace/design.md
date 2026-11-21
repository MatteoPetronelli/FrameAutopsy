# Design

## Context

Repository state: docs, OpenSpec (initialised, CLI 2.0.0, `openspec/config.yaml` from roadmap annex A) and Claude setup;
no `Cargo.toml`. Constraints come from `CLAUDE.md` (unsafe policy D1/D11, disk hygiene) and `docs/architecture.md`
(crate roles, dependency graph). See proposal.md for motivation. Requirements are in `specs/workspace-hygiene` and
`specs/unsafe-policy`; this document only records how they are met.

## Goals / Non-Goals

**Goals:**
- A gate that is green on an empty workspace and provably red on an `unsafe` block in `fa-core`.
- Lint headers in place so later lots inherit the policy without re-deciding it.
- Everything in this lot compiles in seconds and stays small on disk.

**Non-Goals:**
- No asm, SIMD, FFI, ring buffer or ETW code (lots 2 to 5).
- No `fa-probe`, `fa-capture`, `fa-storage`, binaries or other crates: each is created by the lot that first needs it.
- No `compliance.toml`, no `frameautopsy-tos-guard` skill (lot 1).
- No Miri step yet: `fa-sync` has no concurrent code before lot 5.

## Decisions

1. **Workspace.** Root `Cargo.toml` with `[workspace] resolver = "2"`, `members = ["crates/*"]`, shared
   `[workspace.package]` (edition 2021) and `[workspace.lints]`. Alternative: explicit member list. The glob is shorter
   and later lots add crates without touching the root.
2. **Four skeleton crates, lint headers.** `fa-core`: `#![forbid(unsafe_code)]`. `fa-asm`, `fa-win`, `fa-sync`:
   `#![deny(unsafe_op_in_unsafe_fn)]`, `#![deny(clippy::undocumented_unsafe_blocks)]`,
   `#![deny(clippy::multiple_unsafe_ops_per_block)]`, plus `#![warn(missing_docs)]` and a `# Safety` rule for any
   exported `unsafe fn`. Headers go in each `lib.rs` (not only `[workspace.lints]`) so the policy is visible where the
   code is and a crate cannot silently opt out. Alternative: workspace lints only; rejected because `forbid` per crate
   is the clearest signal to a reader. Dependency edges follow `docs/architecture.md` §2 (`fa-core` may depend on
   `fa-sync`; none needed yet).
3. **Profiles.** `[profile.dev] debug = "line-tables-only"` and `[profile.dev.package."*"] debug = false`. Nothing else
   is tuned now (release profile settings are chosen with the first measurement that needs them). `cargo sweep --time 14`
   is documented in the README.
4. **Ignore files.** `.gitignore`: `target/`, `graphify-out/`, `*.etl`, `*.jfr`, local data directory, Lab temporary
   files, `profile.md`. `.graphifyignore`: `target/`, `graphify-out/`, data directories (mirrors RiftMind). Verified by
   `git check-ignore` and `git add -A --dry-run`, not by eye.
5. **`scripts/gate.ps1`.** Sequential, fail-fast (`$ErrorActionPreference = 'Stop'` plus an explicit `$LASTEXITCODE`
   check after every native command, since PowerShell does not stop on native exit codes). Steps: `cargo fmt --all --
   --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. The Miri and Lab-scorer
   steps are written as guarded blocks (run only when `fa-sync` has tests / the `lab score` command exists) so later
   lots turn them on without rewriting the script. `graphify update .` runs last as the roadmap gate lists it, but a
   graphify failure must not hide a failing cargo step (it is after them anyway). Alternative: `cargo xtask`; rejected
   as an extra crate for no gain.
6. **Counting allocator (SYNC-00).** In `fa-sync` behind the `test-support` feature: a `CountingAlloc` type
   implementing `GlobalAlloc` by delegating to `System` and incrementing `AtomicUsize` counters (alloc, dealloc,
   realloc, bytes) with `Relaxed` ordering. It must not allocate itself. Safe API on top: `count_allocs(|| ...) ->
   AllocStats` and an `assert_no_alloc(|| ...)` helper. Counting is per measurement window (a scope guard resets and
   reads the counters); tests using it run in their own integration-test binary with `#[global_allocator]` so other
   tests are unaffected. It is the earn-its-place exception "needed by the zero-allocation tests": no reference, no
   bench; verification is a self-test (a known `Vec::push` past capacity is counted, a preallocated push is not).
   Alternatives: `assert_no_alloc` crate or `stats_alloc` (reference crates). Considered and the catalog already
   decided hand-written (RiftMind pattern, ~40 lines, no dependency); recorded here so it can be revisited.
   Each `unsafe` block carries `// SAFETY:` stating that the delegation preserves the layout contract of `GlobalAlloc`.
7. **Bench harness.** `criterion` as dev-dependency in `fa-asm` and `fa-sync`, `[[bench]] harness = false`. Bench files
   are named `<module>_vs_<reference>` and print both sides. Lot 0 ships the dependency and one trivial bench that
   proves `cargo bench --no-run` links; real benches arrive with the modules they judge.
8. **Docs and skill.** Short `README.md` (what, build, gate, `cargo sweep`). `CLAUDE.md` status updated at the end of
   the lot. Project skill `.claude/skills/frameautopsy-invariants/SKILL.md`, modelled on `riftmind-invariants` (read
   only, conventions only, nothing private copied): unsafe confinement, earn-its-place, zero allocation, gate before
   commit.
9. **OpenSpec changes 0 to 4.** `openspec init` is already done (`openspec/` exists, CLI 2.0.0), so this lot does not
   run it. The changes for lots 0 to 4 are created by this planning step from `specifications.md` (headings raised one
   level under `## ADDED Requirements`). Later changes are created only when their lot starts.
10. **Failing-gate proof.** The policy is only trusted once seen failing. The apply step adds a deliberate `unsafe`
    block to `fa-core`, runs the gate, shows the compile error and non-zero exit, reverts, and shows the green run. A
    second probe adds an `unsafe` block without `// SAFETY:` to a temporary `fa-win` function and shows Clippy
    failing, covering the "Missing SAFETY comment" scenario.

## Risks / Trade-offs

- [PowerShell ignores native exit codes] -> explicit `$LASTEXITCODE` check after each step; the failing-gate task
  proves the exit code is non-zero.
- [Lint headers drift between crates] -> a cheap header check in the gate (the three unsafe crates contain the three
  `deny` lines, `fa-core` contains `forbid(unsafe_code)`), mutation-checked.
- [Counting allocator observes other threads] -> counters are global; the self-test lives in a dedicated test binary
  and measures deltas around a closure, documented as single-threaded.
- [`graphify update .` slow or failing without a graph] -> kept last in the gate; revisit if it slows the gate.
- [Nightly toolchain drift for Miri] -> pinned when the Miri step is switched on (lot 5).

## Migration Plan

No data or deployed state. Work on branch `lot-0/bootstrap-workspace` with small Conventional Commits. Rollback is
reverting the branch. Archive the change at the end of the lot, after the user's "compris" on SYNC-00.
