# FrameAutopsy

Windows desktop tool for PC gamers that explains and fixes in-game stutters and latency spikes, then **proves** its fixes:
observe (ETW frametimes, DPC/ISR, scheduling, page faults, disk, power) → diagnose (TOML cause rules on a ring buffer,
triggered by a stutter) → act (reversible actions on *other* processes and the system, never the game) → prove (paired
A/B blocks, exact sign-flip test on 1 % lows). Rust workspace (edition 2021), elevated headless collector + non-elevated
egui UI. Target games: LoL, VALORANT, Fortnite, Overwatch 2, Minecraft Java (client); any other game is observe-only.

**Status (2026-11-21): lot 0 archived and pushed; lot 1 `compliance-matrix` coded on the local branch
`lot-1/compliance-matrix`** (matrix, `TargetProcess`, `OpenProcess` source test, tos-guard skill; gate green), pending the
user's go to merge. Riot Developer Portal registration question is with DevRel (`docs/compliance.md` 2.1). Next: lot 2
`clock-and-topology` (prompt in `roadmap.md`).

## Working agreement
- Talk to the user in **French**. Code, identifiers, commits, OpenSpec artifacts and technical docs in **English**.
  UI text is French.
- **Spec first**: one OpenSpec change per lot, validated by the user before any application code. The user directs and
  reviews everything; explain what you do and why, briefly, and explain low-level reasoning when touching unsafe/asm.
- Conventional Commits (`feat(scope): ...`), small and frequent, one branch per lot; the gate must be green to move on.
- **Publishing**: lot branches stay local and are never pushed. Merge the local branch into `main` (after the user's
  go), then push `main` to `origin`. No remote branch, no PR unless the user asks.
- Say when a lead is risky, a gain uncertain, or a fact unverified. Never infer a game rule from memory.

## Source documents (read before acting)
| File | Content | Read when |
|---|---|---|
| `handoff.md` | origin brief: vision, compliance constraints, asm leads, open questions | context, scope doubts |
| `roadmap.md` | 17 lots: order, scope, done criteria, **launch prompt per lot**, `openspec/config.yaml` draft | starting a lot |
| `specifications.md` | one `##` section per capability, OpenSpec format, `(calibrate)` / `(Claude's call)` markers | creating a change |
| `docs/architecture.md` | crates, binaries, trust boundaries, data flow, timebase, budgets, storage layout | any design work |
| `docs/asm-unsafe-catalog.md` | "earn its place" ledger (ASM-xx, SIMD-xx, SYNC-xx, WIN-xx), bench protocol, checkpoint template | any unsafe/asm |
| `docs/compliance.md` | forbidden list, sources, anti-cheats, capability matrix draft, anti-cheat test protocol | data sources, actions, UI, overlay |
| `docs/decisions.md` | user decisions D1–D12, Claude's calls, open questions, calibration log (§5) | before re-deciding anything |
| `profile.md` | personal profile of the user: **local only, gitignored, never commit or quote publicly** | rarely |

## Planned layout (crates are created by the lot that first needs them, not upfront)
| Crate | unsafe | Role | Lot |
|---|---|---|---|
| `fa-asm` | yes | asm/SIMD kernels: TSC reads, probes, FMA load, Stream VByte, cross-correlation, selection, PRNG | 0 |
| `fa-win` | yes | Windows FFI: ETW, process/power APIs, topology, shm, pipes + DACLs, D3D11, D2D/DComp | 0 |
| `fa-sync` | yes | SPSC ring, cross-process seqlock, counting test allocator (`test-support` feature) | 0 |
| `fa-core` | no | window ring, stutter trigger, TOML rules, allocation-free FSM, compliance matrix, `TargetProcess` | 0 |
| `fa-probe` / `fa-capture` / `fa-storage` | no | probe orchestration (3) / ETW decoding + frame extraction (5) / SQLite WAL + MessagePack + zstd (8) | 3–8 |
| `fa-attrib` / `fa-governor` / `fa-experiment` / `fa-jvm` | no | symbols, **only crate reaching the internet** (11) / actions + journal (12) / A/B stats (13) / JVM (15) | 11–15 |

Binaries: `fa` (CLI, non-elevated except `install`/`uninstall` and dev commands), `fa-collector` (elevated scheduled
task, headless), `fa-watchdog` (reverts the governor journal if the collector dies), `fa-ui` (egui, never elevated).
Dependencies point to `fa-core`/`fa-sync`/`fa-asm`/`fa-win`; graph in `docs/architecture.md` §2.

## Commands
```powershell
openspec list                     # verified 2026-11-20: no active changes yet
openspec validate --all           # validate changes and specs
graphify hook status              # git hooks installed
powershell -File scripts/gate.ps1 # headers -> fmt --check -> clippy -D warnings -> test -> miri (lot 5+) -> fa lab score (lot 10+) -> graphify
cargo bench -p fa-asm --bench <module>_vs_<reference>   # when a lot touches a kernel; paste result in the catalog
cargo sweep --time 14             # disk hygiene (cargo-sweep)
```
Dev commands that need an **elevated terminal** (ETW requires admin): `fa capture --seconds N`, `fa lab run <scenario>`,
`fa lab campaign`. `fa check` and `fa lab score` run without admin.

## Hard rule: never touch the game (product boundary, `docs/compliance.md`)
- Forbidden for every game: DLL injection, API/DirectX/Vulkan hooks, reading/writing game memory, **any handle on a game
  process** (`OpenProcess` only via `fa-win::process::open_target(&TargetProcess)`, enforced by a source test), modifying
  game/launcher/mod files, changing the game's priority/affinity/CPU Sets, kernel drivers, MSR/`rdpmc`,
  `SeDebugPrivilege`, input automation, anti-cheat bypass/detection, network interception, function-level symbolization
  of game modules (module name only).
- No game data shown or signalled during a match; game-event correlation only in post-match reports.
- Games are identified by full image name (Microsoft-Windows-Kernel-Process ETW or Toolhelp snapshot), Minecraft by
  window title. Classic kernel events truncate names to 15 bytes: `TargetProcess` fails closed on a truncated name.
- `compliance.toml` matrix: baseline observers (O1 O2 O4 O5) on unless `forbidden`; everything else (O3, O6, O7, actions,
  overlay) only when `allowed`; unknown games = baseline observers only. Overrides may only disable.
- A `gray` or `unverified` point is **not implemented**: ask the user. Only the user runs anti-cheat tests.
- Network: only `fa-attrib` reaches non-loopback hosts (opt-in symbol download, parsed non-elevated only); the LoL Live
  Client client is `127.0.0.1`-only. No telemetry.
- Least privilege: the UI never runs elevated; pipe commands carry collector-issued ids only (never PIDs, paths, args);
  elevated binaries run from `%ProgramFiles%\FrameAutopsy`, state in admin-only `%ProgramData%\FrameAutopsy`.

## unsafe and asm policy (D1, D11, `unsafe-policy` spec)
- `unsafe` only in `fa-asm`, `fa-win`, `fa-sync`; every other crate has `#![forbid(unsafe_code)]`. Unsafe crates deny
  `unsafe_op_in_unsafe_fn`, `clippy::undocumented_unsafe_blocks`, `clippy::multiple_unsafe_ops_per_block`; public API safe.
- Every `unsafe` block carries `// SAFETY:` (uppercase) stating the invariant.
- **Earn its place**: write the catalog line *before* the code, then (a) reference impl, (b) parity test, (c) recorded
  comparison on the declared metric, (d) runtime fallback. Loses (or wins < 5 %) → the reference ships, the module is
  deleted, the numbers stay in the catalog. It must win **on the target machine**.
- CPU features detected once at startup; `FA_FORCE_SCALAR=1` forces every fallback. Asm: `core::arch::asm!`, Intel
  syntax, precise `options`; `global_asm!` only with `.seh_proc`/`.seh_endproc`. No MASM/NASM/`build.rs` for asm.
- Every `extern "system"` callback wraps its body in `catch_unwind`. `fa-sync` passes `cargo +nightly miri test -p fa-sync`.
- **Explanation checkpoint**: each new unsafe/asm block is explained to the user in French (what it does, why it is
  sound, what breaks it, how it is verified; template in catalog §7). Merge only after the user says "compris".

## Performance invariants
- Hot path (ETW callback → SPSC ring → 10 s window → trigger) is allocation-free, proven with the counting allocator;
  diagnosis, attribution and storage run on a worker. Pre-sized reused `Vec`s, no arena.
- Everything bounded: rings, windows, retention (30 days / 2 GB), symbol cache (2 GB), probe duty cycle (≤ 0.01 %).
  Budgets in `docs/architecture.md` §6. A recalibrated `(calibrate)` value is logged in `docs/decisions.md` §5.
- One timebase: raw TSC ticks (`ClientContext = 3` + `PROCESS_TRACE_MODE_RAW_TIMESTAMP`), integer conversions.

## Gotchas
- **The GitHub repo is public.** `profile.md` stays ignored; nothing from `D:\Matteo\RiftMind` (private reference repo,
  read-only, conventions only) is copied in.
- Target machine: i5-11400H (6 identical cores, one LLC, AVX2 + AVX-512, no WAITPKG, no MWAITX, no hybrid cores),
  RTX 3060 Laptop + Intel UHD (Optimus), Windows 11 26H2 build 26300. Hybrid/dual-CCD paths use synthetic fixtures.
- Disk: `[profile.dev] debug = "line-tables-only"` and `[profile.dev.package."*"] debug = false` from lot 0 (RiftMind's
  `target/` reached 50 GB). Never commit `target/`, `*.etl`, `*.jfr`, recordings; `lab/fixtures/` ≤ 50 MB.
- Windows locks running executables: stop `fa-ui`/`fa-collector` before `cargo build`/`test`. The scheduled task runs the
  Program Files copy, so a rebuild needs `fa install` again to reach the collector.
- Timer resolution is per-process since Windows 10 2004: not an action (only a Lab injector on the Lab itself).
- Vendored community skills assume Linux/GCC: `assembly-x86` teaches System V ABI and AT&T/GCC inline asm (we use the
  Win64 ABI and Rust `asm!` Intel syntax); `rust-ffi` teaches bindgen (we use `windows`/`windows-sys`); `perf`, `sudo`,
  futex and rseq commands in `rust-profiling`, `cpu-cache-opt`, `memory-model` do not apply. Known errors: `.claude/third-party/NOTICES.md`.

## Skills
- Always: `rust-cargo`, `ponytail` (global). From lot 0: `frameautopsy-invariants`; from lot 1: `frameautopsy-tos-guard`
  (project skills, created by those lots). `threat-modeling` for lots 7 and 12; `mutation-verified-testing` for guard
  tests (matrix, `TargetProcess`, thresholds); `openspec-batch-executor` to run a lot from its `tasks.md`.
- Vendored (`.claude/skills/`): `rust-unsafe`, `rust-sanitizers-miri`, `memory-model` → `fa-sync`/`fa-win` (lots 5, 7);
  `assembly-x86`, `simd-intrinsics`, `cpu-cache-opt`, `rust-profiling` → `fa-asm` kernels and benches (lots 2–4, 8, 10,
  13); `rust-ffi` → `fa-win`; `property-based-testing` (proptest) → codecs and parsers (lots 8, 15); `sharp-edges` →
  pipe schema, overrides, `TargetProcess` API (lots 1, 7, 12).

## OpenSpec
- `openspec/config.yaml` = roadmap annex A. Workflow skills: `openspec-propose`, `-explore`, `-apply-change`,
  `-update-change`, `-sync-specs`, `-archive-change`.
- Create a change only when its lot starts: `openspec/changes/<change-id>/` with `proposal.md` (why + effect on
  compliance), `design.md` (catalog modules, choices), `tasks.md` (groups ending with the gate and explanation
  checkpoints) and `specs/<capability>/spec.md` copied from `specifications.md` with headings raised one level under
  `## ADDED Requirements` (drop the "Introduced by" line). `openspec validate` before coding; archive at the end of the lot.

## Do not touch
- OpenSpec-generated files: `.claude/skills/openspec-*`, `.agents/skills/` (regenerated by `openspec update`).
- Vendored skills in `.claude/skills/` (update by re-download, see `.claude/third-party/NOTICES.md`).
- User decisions D1–D12 in `docs/decisions.md`: propose changes, never rewrite them.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.
**Built** (2026-11-21, `graphify update .` runs in the gate). `graphify-out/` is
gitignored (local graph); the git hooks rebuild it after each commit. The search/read guard hooks live in
`.claude/settings.local.json` (machine-specific path, not committed).

Rules:
- When graphify-out/graph.json exists, run `graphify query "<question>"` before your first grep, search or source-file read for a codebase question, and put this rule in every subagent prompt that explores code. Read raw files only for lines the graph or an earlier read already located. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
- A committed graphify-out/graph.json is rewritten whole on every build, so a large diff to it in a pull request is expected. Do not flag it as churn or suggest skipping the graph rebuild or its hooks.
