# Handoff: session starter (written 2026-11-22, after lot 5)

This file starts a **new Claude Code session** on FrameAutopsy. The original brief of the project (vision, compliance
constraints, asm leads) is kept aside in `handoff-origin.md`: read it only for context or scope doubts. Read `CLAUDE.md`
first (it is loaded automatically), then this file.

The user speaks French (talk to them in French). Code, commits, OpenSpec artifacts and technical docs are in English.

## Your task: lot 6 `stutter-lab`

The OpenSpec change already exists and is valid: `openspec/changes/stutter-lab/` (`proposal.md`, `design.md`, `tasks.md`,
`specs/stutter-lab/spec.md`, 30 tasks in 7 groups). The changes of lots 7, 8 and 9 (`collector-service`,
`incident-store`, `dashboard`) are written too.

1. **Present the change to the user** in French, in a few lines per file, and **wait for their validation**. No
   application code before it (working agreement: spec first). If they ask for changes, edit the change (skill
   `openspec-update-change`), validate with `openspec validate stutter-lab --strict`, commit.
2. Create the branch `lot-6-stutter-lab` and follow `tasks.md` group by group (skill `openspec-batch-executor`). The
   user says "go groupe N" to start each group. Tick tasks as they are verified, not before.
3. Each group ends with the gate (`powershell -File scripts/gate.ps1`). Each new `unsafe` block gets a French explanation
   checkpoint (template: `docs/asm-unsafe-catalog.md` section 7); merge only after the user says "compris".
4. At the end: archive the change (`openspec archive stutter-lab -y`, then fill the `Purpose` of the new spec: the archive
   leaves "TBD"), update the status line of `CLAUDE.md` and this file, and **ask the user's go** before merging into
   `main` and pushing.

### What the lot 6 change already settles (do not re-decide)

- `scripts/gate.ps1` runs `cargo run -p fa --release -- lab score` as soon as `crates/fa/src/lab.rs` exists, but the
  scorer is lot 10. So the Lab lives in `crates/fa/src/lab/` (`mod.rs`, no `lab.rs`) and task 1.2 moves the gate trigger to
  `crates/fa/src/lab/score.rs`.
- D3D11 and the window are unsafe code in `fa-win` (WIN-05) behind a safe API; the Lab in `fa` drives it. New catalog
  lines before any code: WIN-05, ASM-07 and a new WIN-12 (Lab system calls).
- ASM-07 (`clflushopt` for `cache-thrash`) is still to earn its place. Intrinsics reference first; asm only if the
  reference cannot produce the behaviour. Expect a tie or a loss (see "Earn-its-place so far").
- `fa lab` refuses to run while a matrix game runs (`fa_probe::plan::running_games`, same refusal as `fa check`).
  Injections are capped (30 s each; disk bursts 1 GB with 10 GB free required) and cleaned up on Ctrl+C.
- `cpu-burn` never loads the SMT sibling of a CPU something else measures (use `fa_probe::plan::load_cpus`); it may use
  the existing `fa_asm::fma::burn` (intrinsics, nothing to earn).
- No large pages anywhere (rejected by the user, `docs/decisions.md`): `tlb-thrash` works with 4 KiB pages.
- Order (design D-9): render loop and `frame-stall` first, then check that the lot 5 capture sees the exact stall, then
  the other injectors. The lot 5 PresentMon parity is to be replayed on the Lab loop (tasks of group 4).
- Dev commands (`fa lab run`, capture) run from an **elevated terminal that the user opens**; you cannot elevate.

### Lessons of lot 5 that bear on lot 6

- The timer wake-up lateness measured on this machine is p50 about 0.5 ms, p99 1.3 to 3.3 ms, up to 4.6 ms: a
  sleep-paced loop at 144 FPS (6.9 ms) jitters by up to half a frame. Pace with the swap chain and finish the wait on a
  TSC deadline; size injected stalls well above the loop's own jitter and record that jitter in each scenario.
- The machine is an **Optimus laptop** (RTX 3060 Laptop + Intel UHD). Which GPU the Lab window presents on is
  unverified: record the adapter in the journal header. Opera presents in "Hardware Composed: Independent Flip", Chrome
  in "Composed: Flip" (PresentMon 2.6.0 CSV).
- `fa capture --seconds N --frames-csv FILE` and `fa capture compare OURS.csv PRESENTMON.csv` exist: use them to
  verify the Lab's stalls and to replay the parity on the Lab loop. PresentMon console:
  `"/c/Program Files/Intel/PresentMon/PresentMonConsoleApplication/PresentMon-2.6.0-x64.exe" --process_name <exe> --output_file D:/... --timed 20 --terminate_after_timed --stop_existing_session`
  (run it in the same elevated terminal, in the background with `&`, together with `fa capture`).
- Use forward slashes in paths given to commands from Git Bash (`D:/Matteo/...`): backslashes are eaten. Give new file
  names to each measurement run, a reused name overwrites the previous run (this happened). `measurements/` (captures,
  PresentMon CSVs, Lab journals) is gitignored; `scripts/lab-verify.ps1` writes there. The old probe CSVs of lot 3 are
  in `docs/measurements/`.

## Current state (main = `4e624b8`, 2026-11-22, pushed)

Lots 0 to 5 are done, merged into `main`, archived under `openspec/changes/archive/`. `openspec/specs/` holds 9 specs
(the 7 earlier plus `etw-capture` and `lockfree-transport`). `openspec/changes/` holds `stutter-lab`,
`collector-service`, `incident-store`, `dashboard`. The gate is green and includes Miri (`cargo +nightly miri test -p
fa-sync`, installed in lot 5).

Crates: `fa-core` (matrix, `TargetProcess`, clock math), `fa-asm` (CPUID, TSC, frequency chain, pointer chase, bandwidth,
FMA load), `fa-win` (clock, topology, pinning, timers, PDH, memory, process list, windows, power, console, **`etw`**:
session and consumer, WIN-01), `fa-sync` (counting allocator, **`spsc`** ring, SYNC-01), `fa-probe` (probes, verdicts,
report, baseline), **`fa-capture`** (event view, decoders, hot and cold records, rings pipeline, frame extraction, module
map, process table; no unsafe), `fa` (CLI: `fa check`, `fa clock`, `fa probe freq|wake|domain|curve|bw`, `fa windows`,
**`fa capture`**, **`fa capture compare`**). `fa capture` and the ETW examples of `fa-win` need an ELEVATED terminal.

### Earn-its-place so far

ASM-02 (frequency chain) asm kept. ASM-01 (TSC reads), ASM-03 (pointer chase), ASM-04 (bandwidth), ASM-06 (FMA load)
tied with the Rust/intrinsics reference: the reference ships and the asm was deleted. **SYNC-01 (SPSC ring) beat `rtrb`**
(throughput +23 % to +71 % over 3 runs, latency a tie): `fa-sync::spsc` ships. WIN-01 (ETW consumer) was not compared with
`ferrisetw` (user decision: its TDH schemas make a synthetic comparison meaningless): hand-written consumer kept, recorded
in `docs/asm-unsafe-catalog.md`. Pending: SYNC-02 (lot 7), SIMD-01 (lot 8), ASM-07 (lot 6).

### Facts measured on the target machine (i5-11400H, Windows 11 build 26300, AC power, performance mode)

- 6 cores / 12 threads, one LLC, no hybrid cores, AVX2 + AVX-512, no WAITPKG, no MWAITX. TSC invariant, 2 688 000 000 Hz
  (CPUID `0x15`), 4.6 ppm above QPC; the 10 s TSC/QPC criterion is 100 us (45 us measured).
- Single-core clock about 4.2 to 4.5 GHz on AC (3.4 on battery); all-core FMA load holds about 4.09 GHz on AC, 2.49 on battery.
- **SMT bias**: the frequency probe reads about 9 % low when the SMT sibling of its core is busy. Load other physical
  cores only. SMT siblings are adjacent logical CPUs (2-3, 4-5, ...).
- Timer wake-up (`CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`, 1 ms): p50 about 0.5 ms, p99 1.3 to 3.3 ms, up to 4.6 ms.
- Memory: 2 x 16 GiB DDR4-3200 (51.2 GB/s dual channel). Read 32 to 45 GB/s, write 26 to 35, copy 26 to 38 (+-10 %).
  Latency L1 1.25 ns, L2 4.4 ns, RAM about 110 ns. Effective last-level capacity with 4 KiB pages looks like 3 to 4 MiB
  although the topology says 12 MiB (cause not established): size cache-thrash buffers well above 12 MiB.
- Large pages are refused by the system and rejected as a decision: 4 KiB pages everywhere.
- Minecraft Java is detected by a visible window of class `GLFW30` owned by `java.exe`/`javaw.exe`; IntelliJ is
  `idea64.exe`/`SunAwtFrame`, VS Code is `Code.exe`.
- HWiNFO crashes on launch here (kernel driver); the frequency probe agrees with PDH within 0.2 %.
- ETW (lot 5): non-zero kernel addresses reach an admin session without `SeDebugPrivilege`; one private system-logger
  session takes the kernel flags and the manifest providers (DXGI, DxgKrnl with an event-id filter, Kernel-Process).
  PerfInfo opcodes: 67 ISR, 68 DPC, 69 TimerDPC. The context switch (Thread class, op 36, version 5) has no TDH schema:
  only its first four fields are decoded. About 40 000 context switches and 50 000 events per second in normal use.
  Delivery is batched (median delay 300 to 570 ms, up to about 1 s); timestamps are raw TSC and never negative against
  `fa_asm::tsc::now()`. The rundown at start and stop emits about 45 000 records at once (cold ring: 65 536 slots).
- Frametime parity with PresentMon 2.6.0: Opera 1 037 of 1 037 frames within 0.1 ms, Chrome 738 of 747 (the 9
  differences are PresentMon rows out of order).
- **Not verified**: hard page fault events (none seen in 20 s; test with the Lab's `page-fault-storm`), frames of
  OpenGL/Vulkan apps (Minecraft Java) through DxgKrnl `Present_Info`, displayed time and present mode (not computed by
  us), thread to process attribution for context switches (needs a thread map; open for lots 7 and 8).

### Open items outside the code

- Riot: the product is registered as a personal project on the Riot Developer Portal (`docs/compliance.md` 2.1); the
  user reports everything is confirmed, nothing in writing; the DevRel message was not sent. The technical question about
  ETW stack walks (O3) waits for lot 11. **Never write "approved by Riot"** anywhere.
- Blizzard: the Code of Conduct and support article could not be read (login wall); the user confirmed nothing else exists
  beyond the Anti-cheating Agreement and the EULA.
- The compliance matrix has `unverified` cells for the baseline observers until the lot 7 anti-cheat test, which only the
  user runs. `fa capture` refuses to start during a matrix game unless `--allow-game`; never run it near a game yourself.

## Standing rules (full text in `CLAUDE.md`)

- Talk to the user in French. Explain what you do and why, briefly; explain low-level reasoning (unsafe, asm); give options
  with a recommendation, not a survey. Do not re-litigate decisions already made (`docs/decisions.md`). Say plainly when
  a lead is risky, a gain uncertain or a fact unverified, and never claim a result you did not measure.
- Never infer a game rule from memory. A `gray` or `unverified` item is not implemented: ask the user. Only the user runs
  anti-cheat tests. No handle on any game process (`OpenProcess` only through `fa-win::process::open_target`, which does
  not exist before lot 12; a source test scans every crate).
- `unsafe` only in `fa-asm`, `fa-win`, `fa-sync`; every unsafe block has an uppercase `// SAFETY:` and a single unsafe
  operation. Every new unsafe or asm block gets a French explanation checkpoint; merge after the user says "compris".
- Earn its place: catalog line first, then reference, parity test, recorded comparison, runtime fallback; a win under 5 %
  is a tie and the reference ships.
- The GitHub repo is public. `profile.md` and `RIOT_DEVREL_MESSAGE.md` are gitignored and never committed.
- Conventional Commits; end commit messages with the attribution line given in the session. **Chain commands with `&&`,
  never `;`** when a gate must pass before a commit.
- Publishing: lot branches stay local; merge into `main` only after the user's go, then push `main`.
- Anything that needs administrator rights (ETW, `fa capture`, the Lab, install) is run by the user in their elevated
  terminal; give them the exact command and ask them to paste the output. Do not tick a task that needs their run before
  they have done it.

## Practical notes (learned the hard way)

- The Bash tool breaks on long heredocs that contain quotes, apostrophes or backslashes (it also eats backslashes inside
  Rust string literals). For anything non-trivial use the Write tool, or a Python script file, and check the result with
  `cargo`. Python edit scripts must preserve the file's line endings (many files are CRLF on disk): the `sub()` helper
  pattern (read bytes, detect `\r\n`, assert exactly one match, write bytes) worked every time.
- Only stage the paths you mean (`git add crates docs openspec Cargo.lock`): measurement CSVs and local files sit in the
  repository root. LF to CRLF warnings on `git add` are normal here.
- Benchmarks (`cargo bench`) run from the repository root; pin threads with `fa_win::thread::pin_current_thread`; 3 runs,
  median and spread. The graphify rebuild that follows each commit disturbs timing: do not benchmark right after a commit.
- Timing-sensitive unit tests must be robust to a busy machine (best of several runs).
- Guard tests are verified by mutation (change the code, see the test fail, restore from a backup copy). Do it for any
  test that guards an allocation count, a layout offset, a bound or an ordering (lot 5 did it for the counting allocator,
  the ISR offset and the SPSC `Release` store, which Miri caught as a data race).
- `scripts/gate.ps1`: headers, fmt, clippy `-D warnings --all-targets --all-features`, tests, Miri (while
  `crates/fa-sync/src/spsc.rs` exists), `fa lab score` (once `crates/fa/src/lab.rs` exists: see lot 6 above), graphify.
  Clippy covers examples and benches, so a throwaway example must satisfy the unsafe lints too.
- Throwaway spikes go in `crates/<crate>/examples/`, are committed as `chore(...)` for the record and deleted once the
  verdict is in `docs/decisions.md` (the history keeps them). The TDH layout tool `etw_layout` and the session check
  `etw_check` remain in `crates/fa-win/examples/` (run elevated).
- `docs/decisions.md` section 5 logs every recalibrated or measured `(calibrate)` value; section 2 logs Claude's calls and
  user decisions; user decisions D1 to D12 are never rewritten.
- OpenSpec: `openspec new change`, `openspec validate <id> --strict`, `openspec archive <id> -y` (leaves `Purpose: TBD`
  in the new specs: fill it by hand). Requirements over 500 characters make `--strict` warn: put the rules in scenarios.
