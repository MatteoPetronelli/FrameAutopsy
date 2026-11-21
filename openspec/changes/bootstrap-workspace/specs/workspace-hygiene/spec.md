

## ADDED Requirements

### Requirement: Workspace layout
The repository SHALL be a Cargo workspace (edition 2021) with crates under `crates/`. A crate SHALL be created by the
first change that needs it, not upfront.

#### Scenario: Lot 0 workspace
- **WHEN** lot 0 is merged
- **THEN** the workspace contains only `fa-core`, `fa-asm`, `fa-win` and `fa-sync` skeletons with their lint headers

### Requirement: Disk hygiene
The workspace `Cargo.toml` SHALL set `[profile.dev] debug = "line-tables-only"` and
`[profile.dev.package."*"] debug = false`. The README SHALL document `cargo sweep --time 14`.

#### Scenario: Debug info of dependencies
- **WHEN** a dev build completes
- **THEN** dependencies are compiled without debug info and workspace crates with line tables only

### Requirement: Ignored files
`.gitignore` SHALL exclude `target/`, `graphify-out/`, `*.etl`, `*.jfr`, the local data directory, Lab temporary
files and `profile.md`.

#### Scenario: Personal profile
- **WHEN** `git add -A` runs in a working copy that contains `profile.md`
- **THEN** `profile.md` is not staged

### Requirement: Local gate
`scripts/gate.ps1` SHALL run, in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, then `cargo +nightly miri test -p fa-sync` once `fa-sync` has tests, then the Lab scorer once
`cause-engine` exists. It SHALL stop at the first failure with a non-zero exit code.

#### Scenario: Clippy warning
- **WHEN** a change introduces a clippy warning
- **THEN** the gate exits non-zero before running the tests

### Requirement: Graphify configuration
`.graphifyignore` SHALL exclude `target/`, `graphify-out/` and the data directories, mirroring RiftMind's setup.

#### Scenario: Ignore list
- **WHEN** `.graphifyignore` is read
- **THEN** it lists `target/`, `graphify-out/` and the data directories

### Requirement: Allocation test harness
`fa-sync` SHALL provide, behind a `test-support` feature, a counting global allocator (RiftMind pattern) so any crate
can assert in its tests that a code path performs zero heap allocations. It lives in `fa-sync` because
`unsafe impl GlobalAlloc` is not possible under `#![forbid(unsafe_code)]`.

#### Scenario: Hidden allocation
- **WHEN** a hot-path test calls a function that pushes into a `Vec` without spare capacity
- **THEN** the allocation-count assertion fails

### Requirement: Benchmark harness
`fa-asm` and `fa-sync` SHALL use `criterion` benches named `<module>_vs_<reference>`; each bench prints the reference
and the candidate side by side.

#### Scenario: Bench naming
- **WHEN** a criterion bench of `fa-asm` or `fa-sync` is added
- **THEN** it is named `<module>_vs_<reference>` and prints the reference


