# Handoff: session starter (written 2026-11-22, after lot 5)

This file starts a **new Claude Code session** on FrameAutopsy. The original brief of the project (vision, compliance
constraints, asm leads) is kept aside in `handoff-origin.md`: read it only for context or scope doubts.

The user speaks French (talk to them in French). Code, commits, OpenSpec artifacts and technical docs are in English.

## Your task

**Start lot 6 `stutter-lab`.** Its OpenSpec change is already written and valid (`openspec/changes/stutter-lab/`); the
other changes (`collector-service`, `incident-store`, `dashboard`) are written too. Read the change, show the user what
it contains, **wait for their validation, and write no application code before it**. Then follow `tasks.md`
(skill `openspec-batch-executor`). Before the first task, re-read the lot 5 facts below: several change the Lab's design.

Things the lot 6 change already says and you must not forget: `gate.ps1` runs `fa lab score` as soon as
`crates/fa/src/lab.rs` exists (the scorer is lot 10), so the Lab lives in `crates/fa/src/lab/` and task 1.2 moves the
gate trigger to `lab/score.rs`; the Lab refuses to run while a matrix game runs (`fa_probe::plan::running_games`);
`cpu-burn` never loads the SMT sibling of a measured CPU; large pages are rejected (4 KiB everywhere).

## Current state (main, 2026-11-22, after lot 5)

Lots 0 to 5 are done. Lot 5 `etw-capture` is archived once merged (check `openspec list`). `openspec/changes/` holds
`stutter-lab`, `collector-service`, `incident-store` and `dashboard`. The gate is green and includes Miri for `fa-sync`.

Crates: `fa-core`, `fa-asm`, `fa-win` (adds `etw`: session and consumer, WIN-01), `fa-sync` (counting allocator and the
SPSC ring `spsc`, SYNC-01), `fa-probe`, `fa-capture` (event view, decoders, hot and cold records, frame extraction,
module map, process table; no unsafe), `fa` (CLI: `fa check`, `fa clock`, `fa probe ...`, `fa windows`, `fa capture`,
`fa capture compare`). `fa capture` and the ETW examples of `fa-win` need an ELEVATED terminal.

### Facts measured in lot 5 (build 26300, 2026-11-22)

- ETW gives non-zero kernel addresses to an admin session without `SeDebugPrivilege`; one private system-logger session
  takes the kernel flags and the manifest providers (DXGI, DxgKrnl with an event-id filter, Kernel-Process).
- Opcodes (PerfInfo): 67 ISR, 68 DPC, 69 TimerDPC. Context switch (Thread class, op 36, version 5) has NO TDH schema;
  only its first four fields are decoded. About 40 000 context switches and 50 000 events per second in normal use.
- ETW delivery is batched: median delay 300 to 570 ms, up to about 1 s (flush timer). Timestamps are raw TSC and never
  negative against `fa_asm::tsc::now()`.
- The rundown at start and stop emits about 45 000 image and process records in an instant: the cold ring needs 65 536 slots.
- Frametime parity with PresentMon 2.6.0: Opera 1 037 of 1 037 frames within 0.1 ms, Chrome 738 of 747 (the 9 differences
  are PresentMon rows out of order). Modes seen: Composed Flip, Hardware Composed Independent Flip.
- SYNC-01 beat `rtrb` (throughput +23 % to +71 %, latency tie), so `fa-sync::spsc` ships. WIN-01 was not compared with
  `ferrisetw` (user decision, `docs/asm-unsafe-catalog.md`).
- Not verified: hard page fault events (none seen), frames of OpenGL or Vulkan apps (Minecraft Java) through DxgKrnl
  `Present_Info`, displayed time and present mode (not computed), thread to process attribution for context switches.
- Git Bash eats backslashes in paths: use `D:/Matteo/...` with `fa capture --frames-csv`. PresentMon console:
  `"/c/Program Files/Intel/PresentMon/PresentMonConsoleApplication/PresentMon-2.6.0-x64.exe"`.

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
