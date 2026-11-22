# Handoff: session starter (written 2026-11-22, after lot 4)

This file starts a **new Claude Code session** on FrameAutopsy. The original brief of the project (vision, compliance
constraints, asm leads) is kept aside in `handoff-origin.md`: read it only for context or scope doubts.

The user speaks French (talk to them in French). Code, commits, OpenSpec artifacts and technical docs are in English.

## Your task

**Initialise the OpenSpec changes of lots 5 to 9** under `openspec/changes/`, exactly as was done for lots 0 to 4 at the
start of the project. Write **no application code**. When the five changes are written and valid, report to the user in
French and wait: each change is validated by the user at the start of its own lot, before any code.

| Lot | Change id | Capabilities it introduces (sections of `specifications.md`) | Roadmap prompt |
|---|---|---|---|
| 5 | `etw-capture` | `etw-capture` **and** `lockfree-transport` (both are "Introduced by change `etw-capture`") | `roadmap.md`, "Lot 5" |
| 6 | `stutter-lab` | `stutter-lab` | "Lot 6" |
| 7 | `collector-service` | `collector-ipc` | "Lot 7" |
| 8 | `incident-store` | `incident-store` | "Lot 8" |
| 9 | `dashboard` | `dashboard` | "Lot 9" |

### Procedure (per change)

1. Check the OpenSpec command first (`openspec --help`, `openspec new change --help`); do not guess flags. Skill
   `openspec-propose` describes the workflow. `openspec/config.yaml` holds the rules each artifact must follow.
2. `openspec new change <id>` then write, in `openspec/changes/<id>/`:
   - `proposal.md`: why, what changes, **effect on `compliance.toml` and on the "never touch the game" boundary**, the
     catalog modules added or changed (`docs/asm-unsafe-catalog.md`), crates, dependencies, storage.
   - `design.md`: context, goals / non-goals, numbered decisions (D-1...), risks. Each unsafe or asm module follows
     the earn-its-place rule.
   - `tasks.md`: numbered groups; each group ends with the gate (`powershell -File scripts/gate.ps1`); every task says
     how it is verified ("verify ..."); an explanation-checkpoint task (French, user says "compris") for each new
     unsafe or asm block.
   - `specs/<capability>/spec.md`: copied from the matching section of `specifications.md`, headings raised one level,
     under `## ADDED Requirements`, the "Introduced by" line dropped. Every requirement needs a `#### Scenario:`.
3. `openspec validate <id> --strict` must pass. A requirement text over 500 characters makes `--strict` warn: keep the
   requirement short and put the rules and examples in scenarios (this was done for lot 4).
4. Work on a local branch (for example `plan/lots-5-9`), commit in small Conventional Commits, merge into `main` only
   after the user's go, then push `main`. Lot branches are never pushed.

### Cross-check each design against what lots 0 to 4 measured (this is the important part)

The lot 4 change was written before lot 3 produced its data, and three of its starting thresholds were wrong against
the measurements. Do not repeat that. Before you finalise a design, read `docs/decisions.md` section 5,
`docs/asm-unsafe-catalog.md` (results and verdicts) and section "Facts measured on the target machine" below, and
correct every number, threshold or assumption that a measurement contradicts. List these corrections in your report.
Say plainly when a lead is risky, a gain is uncertain, or a fact is unverified.

Per-lot points to look at (not exhaustive):

- **Lot 5 `etw-capture`.** The spike comes first: the target is Windows 11 build 26300 ("26H2"); do ETW kernel events give
  kernel addresses to an admin session without `SeDebugPrivilege`? ETW needs an elevated terminal. `fa-sync::spsc`
  (SYNC-01) must pass `cargo +nightly miri test -p fa-sync`, and **Miri is not installed** on the nightly toolchain
  (`cargo +nightly miri --version` fails: `rustup +nightly component add miri` is a prerequisite; ask the user, it
  changes their toolchain). Task `3.x`-style rule: no `OpenProcess`; process names come from
  Microsoft-Windows-Kernel-Process (full names), never from classic kernel events (15-byte truncation). The hot
  path is allocation-free, proven with the counting allocator (`fa-sync`, `test-support` feature).
- **Lot 6 `stutter-lab`.** D3D11 and COM in unsafe (WIN-05). Dev commands run from an admin terminal. The Lab refuses
  to run while a matrix game runs (reuse `fa_probe::plan::running_games`). ASM-06 (FMA load) exists already as the
  intrinsics module `fa_asm::fma` (`fma::burn`); ASM-07 (`clflushopt`) is still to earn its place. `cpu-burn` should
  not load the SMT sibling of a CPU that something else measures (see the SMT bias below).
- **Lot 7 `collector-service`.** Trust boundary: use the `threat-modeling` skill and show the model to the user first.
  `SYNC-02` seqlock under Miri. **The first anti-cheat test is run by the user only**; never run, trigger or simulate
  one. `health\baseline.json` (written by `fa check`) is a user-writable input: the collector must read it with
  `fa_probe::store::read_baseline` (size cap, strict schema).
- **Lot 8 `incident-store`.** The SIMD-01 codec stays only if it beats "delta + zstd" on real snapshots: measure zstd
  alone against delta+zstd first. In-game probe sampling (duty cycle at most 0.01 %) must pick CPUs without game hot
  threads and, on this machine, **avoid probing a core whose SMT sibling is busy** (the frequency probe reads about
  9 % low then).
- **Lot 9 `dashboard`.** The UI never runs elevated and only reads; every action goes through the command channel;
  text in French with decimal commas (`fa_probe::health::fr_num`).

## Current state (main = `131984e`, 2026-11-22)

Lots 0 to 4 are done, merged into `main`, archived under `openspec/changes/archive/`, and `main` is pushed to
`origin`. `openspec/specs/` holds 7 specs: `workspace-hygiene`, `unsafe-policy`, `compliance-matrix`, `tsc-clock`,
`cpu-topology`, `driverless-probes`, `health-check`. `openspec/changes/` is empty (only `archive/`). The gate is green,
172 tests pass.

Crates: `fa-core` (matrix, `TargetProcess`, clock math; no unsafe), `fa-asm` (CPUID, TSC, frequency chain, pointer chase,
bandwidth, FMA load), `fa-win` (clock, topology, pinning, timers, PDH, large pages, process list, windows, power,
console), `fa-sync` (counting allocator only so far), `fa-probe` (probes, verdicts, report, baseline; no unsafe), `fa`
(CLI). `fa-capture`, `fa-storage` and later crates do not exist yet: each is created by the lot that first needs it.

CLI (all non-elevated): `fa check` (about 70 s, refuses while a game runs; see `docs/fa-check.md`), `fa clock`,
`fa probe freq|wake|domain|curve|bw`, `fa windows [--all]`.

Earn-its-place verdicts so far: ASM-02 (frequency chain, asm mandatory) kept; ASM-01 (TSC reads), ASM-03 (pointer chase),
ASM-04 (bandwidth) and ASM-06 (FMA load) **tied with the Rust/intrinsics reference, which ships and the asm was
deleted** (4 ties out of 5 candidates: expect a tie unless a kernel is genuinely different from what LLVM emits).

### Facts measured on the target machine (i5-11400H, Windows 11 build 26300, AC power, performance mode, no overclock)

- 6 cores / 12 threads, one LLC, no hybrid cores, AVX2 + AVX-512, no WAITPKG, no MWAITX. TSC invariant, 2 688 000 000 Hz
  (CPUID `0x15`), 4.6 ppm above the QPC calibration; the 10 s TSC/QPC criterion was relaxed to 100 µs (45 µs measured).
- Single-core clock about 4.2 to 4.5 GHz on AC (3.4 on battery); all-core FMA load holds about 4.09 GHz on AC and
  2.49 GHz on battery.
- **SMT bias**: the frequency probe reads about 9 % low when the SMT sibling of its core is busy. Load other physical
  cores only (`fa_probe::plan::load_cpus`).
- Timer wake-up (`CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`, 1 ms): lateness p50 about 0.5 ms, p99 1.3 to 3.3 ms, up to 4.6 ms
  with a background build.
- Memory: 2 x 16 GiB DDR4-3200 (51.2 GB/s dual channel). Read 32 to 45 GB/s, write 26 to 35, copy 26 to 38 (noisy, +-10 %).
  Latency L1 1.25 ns, L2 4.4 ns, RAM about 110 ns. Effective last-level capacity with 4 KiB pages looks like 3 to 4 MiB
  although the topology says 12 MiB: cause not established.
- Large pages (`MEM_LARGE_PAGES`) are refused; using them needs the lock-memory privilege enabled in our own token,
  which needs `OpenProcessToken`, forbidden outside `open_target` by the source test. **Rejected by the user (2026-11-22): 4 KiB pages everywhere.**
- Minecraft Java is detected by a visible window of class `GLFW30` owned by `java.exe`/`javaw.exe` (modpack COBBLEVERSE
  reads as class `GLFW30`, title "COBBLEVERSE"); IntelliJ is `idea64.exe`/`SunAwtFrame`, VS Code is `Code.exe`.
- HWiNFO crashes on launch on this machine (kernel driver, cause not investigated); the frequency probe agrees with the
  PDH counter within 0.2 % and nothing else was compared.
- Power mode overlay GUIDs: zero GUID on AC, `{961cc777-2547-4f9d-8174-7d86181b8a7a}` on battery (names not verified).

### Open items outside the code

- Riot: the product is registered as a personal project on the Riot Developer Portal (`docs/compliance.md` 2.1); the
  user reports everything is confirmed, nothing in writing; the DevRel message was not sent. The technical question about
  ETW stack walks (O3) waits for lot 11. **Never write "approved by Riot"** anywhere.
- Blizzard: the Code of Conduct and support article could not be read (login wall); the user confirmed nothing else exists
  beyond the Anti-cheating Agreement and the EULA.
- The compliance matrix has `unverified` cells for the baseline observers until the lot 7 anti-cheat test.

## Standing rules (full text in `CLAUDE.md`, which you must read first)

- Talk to the user in French. Explain what you do and why, briefly; explain low-level reasoning (unsafe, asm); give options
  with a recommendation, not a survey. Do not re-litigate decisions already made (`docs/decisions.md`).
- Never infer a game rule from memory. A `gray` or `unverified` item is not implemented: ask the user. Only the user runs
  anti-cheat tests. No handle on any game process (`OpenProcess` only through `fa-win::process::open_target`, enforced by
  a source test that scans every crate).
- `unsafe` only in `fa-asm`, `fa-win`, `fa-sync`; every unsafe block has an uppercase `// SAFETY:` and a single unsafe
  operation. Every new unsafe or asm block gets a French explanation checkpoint; merge after the user says "compris".
- Earn its place: catalog line first, then reference, parity test, recorded comparison, runtime fallback; a win under 5 %
  is a tie and the reference ships.
- The GitHub repo is public. `profile.md` and `RIOT_DEVREL_MESSAGE.md` are gitignored and never committed.
- Conventional Commits; end commit messages with the attribution line given in the session. **Chain commands with `&&`,
  never `;`** when a gate must pass before a commit (a failed gate was committed once because of `;`).
- Publishing: lot branches stay local; merge into `main` only after the user's go, then push `main`.

## Practical notes (learned the hard way)

- The Bash tool breaks on long heredocs that contain quotes or apostrophes. For anything non-trivial write the file with the
  Write tool, or write a Python script file in the scratchpad and run it. Python edit scripts must preserve the file's line
  endings (many files are CRLF on disk); use raw strings or `–`-style escapes for non-ASCII characters in scripts.
- Benchmarks (`cargo bench`) run from the repository root (criterion writes next to the working directory); pin them with
  PowerShell `Start-Process ... ; $p.ProcessorAffinity = [IntPtr]16`; 3 runs, median and spread. The graphify rebuild that
  follows each commit disturbs timing tests: do not benchmark right after a commit.
- Timing-sensitive unit tests must be robust to a busy machine (best of several runs).
- Guard tests are verified by mutation (change the code, see the test fail, restore); a script that applies a list of
  mutants and restores the file in a `finally` is the efficient way (see the lot 4 work in the git history).
- `scripts/gate.ps1`: headers, fmt, clippy `-D warnings --all-features`, tests, Miri (once `crates/fa-sync/src/spsc.rs`
  exists), `fa lab score` (once `crates/fa/src/lab.rs` exists), graphify update.
- `docs/decisions.md` section 5 logs every recalibrated `(calibrate)` value; user decisions D1 to D12 are never rewritten.

## Definition of done for this task

1. `openspec list` shows `etw-capture`, `stutter-lab`, `collector-service`, `incident-store`, `dashboard`, each valid under
   `openspec validate <id> --strict`; `openspec validate --all` passes.
2. The gate is green; everything committed on a local branch.
3. A French report to the user: for each change, what it contains in a few lines, the corrections you made after
   cross-checking with the measurements, the risks and unverified facts, and the questions that only the user can answer.
4. No application code and no new crate written. Wait for the user's go before merging into `main`, and for their
   validation of each change at the start of its lot.
