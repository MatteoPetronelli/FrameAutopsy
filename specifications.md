# FrameAutopsy — Specifications

> Source document for OpenSpec. Each `##` section below is one capability. When a change is created,
> copy the capability section into `openspec/changes/<change-id>/specs/<capability>/spec.md`
> (drop the "Introduced by" line). Order and dependencies live in `roadmap.md`.
> Headings are one level deeper here than in OpenSpec: when copying, raise each by one level
> (`### Purpose` → `## Purpose`, `#### Requirement:` → `### Requirement:`, `##### Scenario:` → `#### Scenario:`),
> and wrap the requirements under `## ADDED Requirements` in a change delta.
>
> Conventions
> - **SHALL** = mandatory. Values marked **(calibrate)** are starting values; the Stutter Lab (lot 6) and real
>   sessions recalibrate them, and the change that recalibrates records the new value.
> - Values marked **(Claude's call)** were chosen by Claude during the brainstorming; the user may override them
>   (list in `docs/decisions.md`).
> - "Matrix game" = a game listed in `compliance.toml` (LoL, VALORANT, Fortnite, Overwatch 2, Minecraft Java).
> - Every timestamp is in TSC ticks unless stated otherwise (see `tsc-clock`).
> - Related documents: `docs/architecture.md`, `docs/asm-unsafe-catalog.md`, `docs/compliance.md`, `docs/decisions.md`.

## Glossary

| Term | Meaning |
|---|---|
| Frametime | Time between two consecutive presents of the game's swap chain, in ms. |
| 1 % low | Average FPS of the slowest 1 % of frames of a session (CapFrameX convention). P99 frametime is reported alongside. |
| 0.1 % low | Same over the slowest 0.1 %. |
| Stutter / incident | A frame that meets the trigger of `incident-store`; the incident is the captured window around it. |
| DPC / ISR | Deferred Procedure Call / Interrupt Service Routine: kernel work run by drivers, measured by ETW. |
| EcoQoS | Windows power-throttling mode for a process (`ProcessPowerThrottling`). |
| CPU Set | Windows soft-affinity mechanism (`SetProcessDefaultCpuSets`). |
| Reference implementation | The safe baseline (safe Rust, intrinsics, existing crate or OS counter) that an asm/unsafe module must beat. |
| TargetProcess | The only type through which the governor can act on a process (see `compliance-matrix`). |

---

## workspace-hygiene

Introduced by change `bootstrap-workspace` (lot 0).

### Purpose
A Cargo workspace that stays small on disk, runs one local gate, and never commits traces or personal files.

### Requirements

#### Requirement: Workspace layout
The repository SHALL be a Cargo workspace (edition 2021) with crates under `crates/`. A crate SHALL be created by the
first change that needs it, not upfront.

##### Scenario: Lot 0 workspace
- **WHEN** lot 0 is merged
- **THEN** the workspace contains only `fa-core`, `fa-asm`, `fa-win` and `fa-sync` skeletons with their lint headers

#### Requirement: Disk hygiene
The workspace `Cargo.toml` SHALL set `[profile.dev] debug = "line-tables-only"` and
`[profile.dev.package."*"] debug = false`. The README SHALL document `cargo sweep --time 14`.

##### Scenario: Debug info of dependencies
- **WHEN** a dev build completes
- **THEN** dependencies are compiled without debug info and workspace crates with line tables only

#### Requirement: Ignored files
`.gitignore` SHALL exclude `target/`, `graphify-out/`, `*.etl`, `*.jfr`, the local data directory, Lab temporary
files and `profile.md`.

##### Scenario: Personal profile
- **WHEN** `git add -A` runs in a working copy that contains `profile.md`
- **THEN** `profile.md` is not staged

#### Requirement: Local gate
`scripts/gate.ps1` SHALL run, in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, then `cargo +nightly miri test -p fa-sync` once `fa-sync` has tests, then the Lab scorer once
`cause-engine` exists. It SHALL stop at the first failure with a non-zero exit code.

##### Scenario: Clippy warning
- **WHEN** a change introduces a clippy warning
- **THEN** the gate exits non-zero before running the tests

#### Requirement: Graphify configuration
`.graphifyignore` SHALL exclude `target/`, `graphify-out/` and the data directories, mirroring RiftMind's setup.

##### Scenario: Ignore list
- **WHEN** `.graphifyignore` is read
- **THEN** it lists `target/`, `graphify-out/` and the data directories

#### Requirement: Allocation test harness
`fa-sync` SHALL provide, behind a `test-support` feature, a counting global allocator (RiftMind pattern) so any crate
can assert in its tests that a code path performs zero heap allocations. It lives in `fa-sync` because
`unsafe impl GlobalAlloc` is not possible under `#![forbid(unsafe_code)]`.

##### Scenario: Hidden allocation
- **WHEN** a hot-path test calls a function that pushes into a `Vec` without spare capacity
- **THEN** the allocation-count assertion fails

#### Requirement: Benchmark harness
`fa-asm` and `fa-sync` SHALL use `criterion` benches named `<module>_vs_<reference>`; each bench prints the reference
and the candidate side by side.

##### Scenario: Bench naming
- **WHEN** a criterion bench of `fa-asm` or `fa-sync` is added
- **THEN** it is named `<module>_vs_<reference>` and prints the reference

---

## unsafe-policy

Introduced by change `bootstrap-workspace` (lot 0).

### Purpose
Make `unsafe` and assembly earn their place, confine them to three crates, and keep every block explainable.

### Requirements

#### Requirement: Unsafe confinement
Only `fa-asm`, `fa-win` and `fa-sync` MAY contain `unsafe`. Every other crate SHALL declare `#![forbid(unsafe_code)]`.

##### Scenario: Unsafe in a safe crate
- **WHEN** an `unsafe` block is added to `fa-core`
- **THEN** compilation fails

#### Requirement: Lints in unsafe crates
The three unsafe crates SHALL declare `#![deny(unsafe_op_in_unsafe_fn)]` and deny the Clippy lints
`undocumented_unsafe_blocks` and `multiple_unsafe_ops_per_block`. Every `unsafe` block SHALL carry a `// SAFETY:`
comment that states the invariant making it sound.

##### Scenario: Missing SAFETY comment
- **WHEN** an `unsafe` block without `// SAFETY:` is added to `fa-win`
- **THEN** the gate fails on Clippy

#### Requirement: Safe public API
The public API of the three unsafe crates SHALL be safe. An exported `unsafe fn` is allowed only with a `# Safety`
doc section and a written reason in `docs/asm-unsafe-catalog.md`.

##### Scenario: Exported unsafe fn
- **WHEN** an `unsafe fn` is exported by `fa-asm`, `fa-win` or `fa-sync`
- **THEN** it has a `# Safety` doc section and a written reason in `docs/asm-unsafe-catalog.md`

#### Requirement: Earn-its-place rule
Every asm or unsafe module SHALL have (a) a reference implementation, (b) a parity test against it, (c) a recorded
comparison on its declared metric (criterion bench or comparative test) in `docs/asm-unsafe-catalog.md`, and (d) a
runtime fallback. If the module does not beat the reference on its declared metric, the reference SHALL ship and the
module SHALL be removed. FFI with no safe alternative is exempt from (c), but an existing wrapper crate that meets the
requirements counts as the reference.

##### Scenario: Kernel loses its benchmark
- **WHEN** the asm Stream VByte decoder is slower than the intrinsics version on the user's machine
- **THEN** the intrinsics version ships, the asm version is deleted, and the catalog records both numbers

##### Scenario: Probe brings no extra information
- **WHEN** the frequency probe shows nothing that the PDH counter does not already show (see `driverless-probes`)
- **THEN** the probe is removed and the PDH counter is used

#### Requirement: Runtime dispatch
CPU features SHALL be detected once at startup (`is_x86_feature_detected!`, plus CPUID for WAITPKG and MWAITX) and
the implementation selected through a function pointer or enum. Setting `FA_FORCE_SCALAR=1` SHALL force every
fallback path.

##### Scenario: CPU without AVX2
- **WHEN** the test suite runs with `FA_FORCE_SCALAR=1`
- **THEN** all tests pass on the scalar paths and no AVX2 instruction executes

#### Requirement: Assembly form
Assembly SHALL use `core::arch::asm!` with Intel syntax and precise options (`nomem`, `nostack`, `preserves_flags`
only when true). `global_asm!` SHALL be used only when a whole function is required, and then with
`.seh_proc`/`.seh_endproc` unwind directives.

##### Scenario: Asm syntax
- **WHEN** assembly is added to an unsafe crate
- **THEN** it uses `core::arch::asm!` with Intel syntax, or `global_asm!` with `.seh_proc`/`.seh_endproc` only when a whole function is required

#### Requirement: No unwinding across FFI
Every `extern "system"` callback SHALL wrap its body in `std::panic::catch_unwind`; a caught panic stops the related
session and is recorded.

##### Scenario: Callback panic
- **WHEN** an `extern "system"` callback panics
- **THEN** `std::panic::catch_unwind` catches it, the related session stops and the panic is recorded

#### Requirement: Miri coverage
`fa-sync` tests SHALL pass under `cargo +nightly miri test -p fa-sync`, with iteration counts reduced under
`cfg(miri)`.

##### Scenario: Miri run
- **WHEN** `cargo +nightly miri test -p fa-sync` is run
- **THEN** the `fa-sync` tests pass, with iteration counts reduced under `cfg(miri)`

#### Requirement: Explanation checkpoint
Each change that adds or modifies an `unsafe` or asm block SHALL include, in its `tasks.md`, a checkpoint where the
block is explained to the user in French (what it does, why it is sound, what would break it). The block is merged only
after the user confirms understanding.

##### Scenario: Checkpoint in tasks
- **WHEN** a change adds or modifies an `unsafe` or asm block
- **THEN** its `tasks.md` has a checkpoint explaining the block to the user in French, and the block is merged only after the user confirms understanding

---

## compliance-matrix

Introduced by change `compliance-matrix` (lot 1).

### Purpose
Encode the games' rules in the product: per game, which observers and actions may run, and make it impossible for the
governor to act on a game, an anti-cheat or a critical process.

### Requirements

#### Requirement: Matrix file
`compliance.toml` SHALL list, per game: id, display name, detection rule (process image names; window-title prefix for
Minecraft), anti-cheat, and a verdict for every observer and action (`allowed`, `gray`, `forbidden`, `unverified`),
each verdict with source URLs, consultation date and anti-cheat test record when one exists. Game ids: `lol`,
`valorant`, `fortnite`, `overwatch2`, `minecraft-java`, `fa-lab`, plus the implicit `unknown`. The file is embedded at
build time; a user override MAY only disable items, never enable them.

##### Scenario: Override tries to enable an action
- **WHEN** a user override sets `governor.ecoqos = "allowed"` for `valorant` while the embedded verdict is `unverified`
- **THEN** the override is rejected with an error naming the item

#### Requirement: Enablement semantics
Observers in the baseline observe-only set (present events, kernel scheduling/DPC/page-fault/disk events, in-game
probes, system counters) SHALL be enabled for every game unless the verdict is `forbidden`. Every other observer, every
action and the overlay SHALL be enabled only when the verdict is `allowed`. (Claude's call, reconciling handoff §2
"observe-only on any game" with §4.2 "to verify"; the anti-cheat smoke test at the end of lot 7 confirms the baseline
per game.)

##### Scenario: Unverified action
- **WHEN** an EcoQoS proposal targets a background process during a VALORANT match and `valorant.governor.ecoqos` is `unverified`
- **THEN** nothing is applied and the UI shows "en attente de vérification de conformité"

##### Scenario: Unknown game
- **WHEN** an unlisted game is detected through its presents
- **THEN** only the baseline observers run; no action and no overlay are available

#### Requirement: Compliance tests
A test SHALL fail if any action is enabled for a game without a matrix entry, or if an entry marked `allowed` lacks a
source URL and a date.

##### Scenario: Matrix entry check
- **WHEN** an action is enabled for a game without a matrix entry, or an entry marked `allowed` lacks a source URL and a date
- **THEN** the test fails

#### Requirement: TargetProcess guard
The governor SHALL act only through `TargetProcess`, constructible only by `TargetProcess::try_new(pid, image_name,
creation_time, &Matrix)`. Construction SHALL fail for matrix games, anti-cheat processes listed in the matrix (for
example `vgc.exe`, `vgtray.exe`, `EasyAntiCheat.exe`, `EasyAntiCheat_EOS.exe`, `BEService.exe`), critical processes
(`System`, `smss.exe`, `csrss.exe`, `wininit.exe`, `winlogon.exe`, `services.exe`, `lsass.exe`, `dwm.exe`,
`audiodg.exe`) and FrameAutopsy's own processes.

##### Scenario: Governor aims at the game
- **WHEN** code tries to build a `TargetProcess` for `VALORANT-Win64-Shipping.exe`
- **THEN** it gets an error and no handle is opened

#### Requirement: No handle on game processes
FrameAutopsy SHALL never call `OpenProcess` (or open any handle) on a matrix game's process. Games are detected by
their full image name, from Microsoft-Windows-Kernel-Process ETW events (collector) or a Toolhelp snapshot (CLI), and
Minecraft by window class (a visible `GLFW30` window owned by `java.exe` or `javaw.exe`, the title varies per modpack) through `EnumWindows`/`GetWindowThreadProcessId`. Classic kernel process events truncate the
image name to 15 bytes and SHALL NOT be used for identification. `TargetProcess` construction SHALL fail closed when
only a truncated or unknown name is available. A source test SHALL fail if `OpenProcess` is called anywhere except
`fa-win::process::open_target(&TargetProcess)`.

##### Scenario: Truncated name
- **WHEN** the only name known for a PID is the 15-byte kernel name `VALORANT-Win64-`
- **THEN** `TargetProcess::try_new` refuses it

#### Requirement: No game data during play
No game-state data SHALL be displayed or signalled while a match is in progress, in any game. Only system metrics MAY
be shown live. Game-event correlation appears only in post-match reports.

##### Scenario: Match in progress
- **WHEN** a match is in progress
- **THEN** no game-state data is displayed or signalled, only system metrics MAY be shown live, and game-event correlation appears only in post-match reports

---

## tsc-clock

Introduced by change `clock-and-topology` (lot 2).

### Purpose
One high-resolution timebase for probes, ETW events and reports. QPC runs at 10 MHz (100 ns resolution) on current
Windows; probes need sub-microsecond precision over ~50 µs windows, so the TSC (~0.3 ns) is the base.

### Requirements

#### Requirement: Invariant TSC detection
The clock SHALL check CPUID `0x80000007:EDX[8]`. Without an invariant TSC it SHALL fall back to QPC as the timebase and
the health check SHALL report it.

##### Scenario: No invariant TSC
- **WHEN** CPUID `0x80000007:EDX[8]` reports no invariant TSC
- **THEN** the clock falls back to QPC as the timebase and the health check reports it

#### Requirement: Read primitives
`fa-asm` SHALL provide `now()` (`lfence; rdtsc; lfence`) and `now_serialized()` (`rdtscp; lfence`). Asm candidates
compete with `_rdtsc`/`__rdtscp` + `_mm_lfence` intrinsics on overhead and jitter; the asm version ships only if it
wins.

##### Scenario: TSC reads
- **WHEN** `fa-asm` is built
- **THEN** it provides `now()` (`lfence; rdtsc; lfence`) and `now_serialized()` (`rdtscp; lfence`)

#### Requirement: TSC frequency
The TSC frequency SHALL come from CPUID `0x15` when its crystal ratio is non-zero, otherwise from a calibration against
QPC: 5 runs of at least 200 ms, median kept. The value is stored in the health baseline.

##### Scenario: Frequency source
- **WHEN** the TSC frequency is determined
- **THEN** it comes from CPUID `0x15` when its crystal ratio is non-zero, otherwise from a QPC calibration of 5 runs of at least 200 ms with the median kept, and is stored in the health baseline

#### Requirement: Conversions
Tick-to-nanosecond conversion SHALL use integer math (u128), without float drift. A QPC↔TSC mapping (offset and scale)
SHALL be re-estimated every 60 s and its drift recorded.

##### Scenario: Ten-second accuracy
- **WHEN** a 10 s interval is measured with both TSC and QPC
- **THEN** the elapsed times differ by less than 100 µs (calibrate; recalibrated from 10 µs in lot 2, see `docs/decisions.md` §5)

##### Scenario: Cross-core consistency
- **WHEN** a test thread pinned successively to every logical CPU reads the TSC
- **THEN** readings are monotonic and the skew between cores is under 1 µs (calibrate)

---

## cpu-topology

Introduced by change `clock-and-topology` (lot 2).

### Purpose
Know cores, SMT siblings, last-level-cache domains and efficiency classes, to pin probes and to place background
processes with CPU Sets.

### Requirements

#### Requirement: Windows API as source of truth
Topology SHALL come from `GetSystemCpuSetInformation` (CPU Set ids, `EfficiencyClass`, core and LLC indexes) and
`GetLogicalProcessorInformationEx` (caches, groups, NUMA). The model maps logical CPU → physical core → LLC domain →
efficiency class.

##### Scenario: Intel hybrid CPU
- **WHEN** topology is read on a CPU with P-cores and E-cores
- **THEN** E-cores have a lower efficiency class than P-cores

##### Scenario: AMD dual-CCD CPU
- **WHEN** topology is read on a CPU with two CCDs
- **THEN** two LLC domains are reported

#### Requirement: CPUID cross-check as a test only
A test SHALL compare the API model with CPUID leaves (`0x0B`/`0x1F`, `0x1A`, `0x8000001E`, `0x04`/`0x8000001D`) read
through `__cpuid_count` intrinsics and log mismatches. CPUID is not a runtime source (the API already exposes the
information; the cross-check is a validation and learning device, documented as such).

##### Scenario: Cross-check test
- **WHEN** the topology cross-check test runs
- **THEN** it compares the API model with the CPUID leaves read through `__cpuid_count` intrinsics and logs mismatches, and CPUID is not a runtime source

#### Requirement: Synthetic topologies
The hybrid and dual-CCD scenarios SHALL be tested with synthetic API buffers, because the target machine
(i5-11400H: 6 identical cores, one LLC) has neither; real validation waits for another machine.

##### Scenario: Hybrid and dual-CCD
- **WHEN** the hybrid and dual-CCD scenarios are tested
- **THEN** they use synthetic API buffers, not the target machine

---

## driverless-probes

Introduced by change `driverless-probes` (lot 3).

### Purpose
Measure from user mode, with no driver, what monitoring tools usually read through drivers (which Vanguard blocks):
effective clock, memory latency and bandwidth, cache sizes, thread wake-up latency. Probes run in FrameAutopsy's own
threads only.

### Requirements

#### Requirement: Pinned execution
Each probe SHALL run on a FrameAutopsy thread pinned to the chosen logical CPU (`SetThreadSelectedCpuSets` or
`SetThreadAffinityMask` on our own thread) and timestamp its result in TSC ticks.

##### Scenario: Pinned probe
- **WHEN** a probe runs
- **THEN** it runs on a FrameAutopsy thread pinned to the chosen logical CPU and timestamps its result in TSC ticks

#### Requirement: Effective frequency probe
The probe SHALL execute an asm chain of dependent `add` instructions (1-cycle latency) sized to last about 50 µs and
compute effective frequency = chain length / elapsed TSC time. Samples whose elapsed time exceeds 1.5× the recent
median (preemption) SHALL be discarded. Asm is mandatory: the compiler folds such a chain.

##### Scenario: Comparison with the Windows counter
- **WHEN** the probe and PDH `\Processor Information(*)\% Processor Performance` × nominal frequency are sampled together under steady load
- **THEN** they agree within 3 % (calibrate)

##### Scenario: Probe must add information
- **WHEN** the Lab soak shows the probe captures no change that the PDH counter smooths away
- **THEN** the probe is removed under the earn-its-place rule and PDH is used

Lot 3 establishes accuracy only. The keep/remove decision is taken in lot 10 (`cause-engine`), once Lab soaks and
samples taken at the instant of a stutter exist. A probe on a CPU the game is not using reflects the game's CPUs only
when they share a clock domain; on per-core-clock CPUs it shows package-level throttling only. Lot 3 checks which case
applies to the target machine (probe one core while another is loaded).

#### Requirement: Memory latency probe
The probe SHALL pointer-chase a buffer of at least 4× the LLC size, arranged as a random cyclic permutation (Sattolo),
on 4 KiB pages and on 2 MiB large pages when `SeLockMemoryPrivilege` is available (fallback otherwise), and report ns
per load. Reference: a Rust `read_volatile` chain; asm ships only if its measurements are more stable (lower
coefficient of variation).

##### Scenario: Pointer chase
- **WHEN** the memory latency probe runs
- **THEN** it pointer-chases a buffer of at least 4× the LLC size as a random cyclic permutation (Sattolo), on 4 KiB pages and on 2 MiB large pages when `SeLockMemoryPrivilege` is available, and reports ns per load

#### Requirement: Memory bandwidth probe
The probe SHALL measure read (AVX2 loads), write (non-temporal stores) and copy bandwidth, multi-threaded across
physical cores, in GB/s. Reference: intrinsics.

##### Scenario: Bandwidth modes
- **WHEN** the memory bandwidth probe runs
- **THEN** it measures read (AVX2 loads), write (non-temporal stores) and copy bandwidth, multi-threaded across physical cores, in GB/s

#### Requirement: Cache curve
The probe SHALL measure latency for working sets from 16 KiB to 256 MiB, detect plateaus as L1/L2/L3 sizes, and
compare them with `cpu-topology`.

##### Scenario: Working-set sweep
- **WHEN** the cache curve probe runs
- **THEN** it measures latency for working sets from 16 KiB to 256 MiB, detects plateaus as L1/L2/L3 sizes and compares them with `cpu-topology`

#### Requirement: Wake-up latency probe
The probe SHALL measure the lateness of a thread woken by a high-resolution waitable timer and report p50/p99/max in
µs. (`tpause`/`umwait`/`mwaitx` waits were dropped: the target CPU has neither WAITPKG nor MWAITX, so they could
never be validated.)

##### Scenario: Timer lateness
- **WHEN** the wake-up latency probe runs
- **THEN** it measures the lateness of a thread woken by a high-resolution waitable timer and reports p50/p99/max in µs

#### Requirement: Off-game only in this lot
In lot 3 probes SHALL run off-game only. In-game sampling needs context switches and is specified in `incident-store`
(lot 8).

##### Scenario: Lot 3 probes
- **WHEN** probes run in lot 3
- **THEN** they run off-game only

#### Requirement: Calibration record
A documented manual procedure SHALL compare probe results with HWiNFO off-game; results are recorded in
`docs/asm-unsafe-catalog.md`.

##### Scenario: HWiNFO comparison
- **WHEN** probe results are compared with HWiNFO off-game by the documented manual procedure
- **THEN** the results are recorded in `docs/asm-unsafe-catalog.md`

---

## health-check

Introduced by change `health-check` (lot 4). First visible result of the project.

### Purpose
`fa check`: in at most 90 s, a readable verdict on the machine (laptop throttling, RAM configuration, scheduling
interference) with no admin rights, no ETW and no game running. It becomes the baseline for later comparisons.

### Requirements

#### Requirement: Command and output
`fa check` SHALL run non-elevated in at most 90 s, print a French report and write JSON to
`%LOCALAPPDATA%\FrameAutopsy\health\<timestamp>.json`. It SHALL change no system setting.

##### Scenario: Check run
- **WHEN** `fa check` runs
- **THEN** it runs non-elevated in at most 90 s, prints a French report, writes JSON to `%LOCALAPPDATA%\FrameAutopsy\health\<timestamp>.json` and changes no system setting

#### Requirement: Refuse during games
`fa check` SHALL refuse to start while a matrix game process is running (Toolhelp snapshot by image name, no handle) and
warn when on battery.

##### Scenario: League of Legends open
- **WHEN** `League of Legends.exe` is running
- **THEN** `fa check` exits with a message asking to close the game first

#### Requirement: Report sections
The report SHALL contain the sections CPU, frequency, memory, scheduling, power and, when present, GPU. The frequency
section SHALL show a single-core peak per core and then a 60 s FMA load (ASM-06) on one logical CPU of every other
physical core, sampled every second on a core whose SMT sibling stays idle.

##### Scenario: Section details
- **WHEN** the report is produced
- **THEN** CPU shows model, topology, invariant TSC, TSC frequency and AVX2/AVX-512/WAITPKG/MWAITX; memory shows latency, bandwidth, cache sizes and the WMI `Win32_PhysicalMemory` modules with configured speed; scheduling shows wake-up p50/p99/max; power shows AC or battery, active scheme and overlay; GPU (NVML) shows name, P-state and power limit

##### Scenario: Report content
- **WHEN** the report is produced
- **THEN** it contains CPU, frequency, memory, scheduling, power, and GPU through NVML when present

#### Requirement: Verdicts with evidence
Each verdict SHALL state its evidence and confidence, and the report SHALL say plainly when nothing was found. The
starting rules (calibrate) are given by the rule scenarios below.

##### Scenario: Throttling rule
- **WHEN** the median frequency after the first 5 s drops more than 10 % below the first-5 s median
- **THEN** the throttling verdict fires with "bridage après X s, −Y %"

##### Scenario: Single-channel RAM rule
- **WHEN** WMI shows one memory module
- **THEN** the single-channel RAM verdict fires, with high confidence when the all-core read bandwidth is also under 60 % of the dual-channel theoretical value for the configured speed and medium otherwise; with more than one module it does not fire, and when WMI gives no module count there is no verdict (no configured speed, so no theoretical bandwidth to compare with)

##### Scenario: Scheduling rule
- **WHEN** the wake-up p99 is above 5 ms (the idle target machine measures p50 about 0.5 ms and p99 2 to 3 ms, so 500 µs would fire on every run)
- **THEN** the scheduling interference verdict fires

##### Scenario: Throttling laptop
- **WHEN** the all-core load median falls from 4.2 GHz to 3.1 GHz after 25 s
- **THEN** the report says "bridage après 25 s, −26 %" with the curve

##### Scenario: Healthy desktop
- **WHEN** no rule fires
- **THEN** the report says no problem was found and lists what was checked

#### Requirement: Baseline
The latest health check run on AC power SHALL become the baseline for in-game comparisons and A/B covariates.

##### Scenario: Latest AC run
- **WHEN** a health check run on AC power completes
- **THEN** the latest such run becomes the baseline for in-game comparisons and A/B covariates

---

## etw-capture

Introduced by change `etw-capture` (lot 5).

### Purpose
A hand-written real-time ETW consumer, zero-copy and allocation-free on the hot path, that produces frametimes and the
system events needed for diagnosis.

### Requirements

#### Requirement: Real-time session
`fa-win` SHALL drive the session through `StartTraceW`, `EnableTraceEx2`, `OpenTraceW` and `ProcessTrace`, under the
session name `FrameAutopsy`. A stale session left by a crash SHALL be stopped at startup.

#### Requirement: Timestamps
The session SHALL use `ClientContext = 3` (CPU cycle counter) when the TSC is invariant, and the consumer SHALL open
the trace with `PROCESS_TRACE_MODE_RAW_TIMESTAMP` so timestamps stay raw TSC values instead of being converted. Without
an invariant TSC, QPC is used and converted through `tsc-clock`.

#### Requirement: Providers
The session SHALL enable present events (Microsoft-Windows-DxgKrnl, Microsoft-Windows-DXGI and what PresentMon's
algorithm needs), kernel context switches, DPC, ISR, hard page faults, disk I/O, process/thread start-stop, image
loads, and Microsoft-Windows-Kernel-Process for full image names. Exact keywords and flags follow PresentMon's
open-source implementation (MIT), used as the reference.

#### Requirement: 24H2 kernel address spike
The change SHALL start with a spike on the target build (Windows 11 26H2, later than 24H2): does an elevated session
without `SeDebugPrivilege` receive non-zero kernel addresses in DPC/ISR routine fields, stack walks and image-load
rundown events? The outcome is recorded in `docs/decisions.md` and decides what `cause-engine` and
`driver-attribution` can do. FrameAutopsy SHALL NOT enable `SeDebugPrivilege`.

#### Requirement: Module map
Kernel image loads (rundown at session start + live loads) SHALL give base, size and path, and every kernel address
SHALL resolve to module + offset when the spike shows addresses are available.

#### Requirement: Zero-copy decoding
The callback SHALL hand `fa-capture` a borrowed view (header fields + `&[u8]` user data valid only during the callback).
`fa-capture` SHALL decode hot events with hand-written layouts keyed by provider, event id and version into fixed-size
`Copy` records. TDH SHALL be used only by a developer tool that dumps layouts. Unknown versions are counted and skipped.

#### Requirement: Zero allocation on the hot path
Decoding and pushing to the transport SHALL perform zero heap allocations.

##### Scenario: One million events
- **WHEN** one million synthetic `EVENT_RECORD`s go through the callback under the counting allocator
- **THEN** the allocation count is 0

#### Requirement: Earns its place against ferrisetw
A comparison on the same synthetic stream SHALL show 0 allocations per event and throughput at least equal to
`ferrisetw`; otherwise the crate is adopted for the non-hot providers.

#### Requirement: Loss accounting
Lost ETW buffers/events and transport drops SHALL be counted and shown.

#### Requirement: Frametime extraction and parity
Present events SHALL be turned into frame records (present start, displayed time, frametime, present mode) per process
and swap chain, following PresentMon's algorithm.

##### Scenario: Parity with PresentMon
- **WHEN** the Lab render loop runs while FrameAutopsy and PresentMon capture in parallel
- **THEN** at least 99 % of frames match within ±0.1 ms of frametime (calibrate)

#### Requirement: Identification without handles
Game processes SHALL be identified by PID and full image name from Microsoft-Windows-Kernel-Process events only.

---

## lockfree-transport

Introduced by change `etw-capture` (lot 5).

### Purpose
Move events from the ETW callback thread to the analysis thread without blocking and without allocation.

### Requirements

#### Requirement: SPSC ring
`fa-sync` SHALL provide a single-producer single-consumer ring: power-of-two capacity, `T: Copy`, cache-line-padded
head and tail, Acquire/Release ordering, `MaybeUninit<T>` slots. `push` SHALL never block: when full it returns an error
and increments a drop counter.

##### Scenario: Consumer stalls
- **WHEN** the consumer stops reading and the ring fills
- **THEN** the ETW callback keeps returning immediately and the drop counter grows

#### Requirement: Verified under Miri
A two-thread stress test SHALL pass under Miri.

#### Requirement: Earns its place against rtrb
A criterion bench (32-byte records) SHALL show throughput at least equal to `rtrb`; otherwise `rtrb` replaces the
module. Prefetch or non-temporal variants ship only if they win.

---

## stutter-lab

Introduced by change `stutter-lab` (lot 6).

### Purpose
Ground truth: provoke known stutters in FrameAutopsy's own render loop to test capture, diagnosis and A/B without a real
game and without any compliance question. Also the demo for the portfolio.

### Requirements

#### Requirement: Own render loop
`fa lab` SHALL open a window with a D3D11 device and a flip-model swap chain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`) and render
a trivial animated scene at 60, 144 or uncapped FPS. The Lab is the `fa-lab` entry of the matrix, where everything is
`allowed`.

#### Requirement: Injectors
The Lab SHALL provide parameterized injectors (start, duration, intensity, target CPUs), each logging ground truth:
- `cpu-burn`: busy loop on chosen CPUs until a TSC deadline (safe Rust: what matters is duration, and core cycles
  have no fixed duration when the clock moves), optionally with the FMA load (ASM-06);
- `frame-stall`: burn N ms before `Present` at frame k;
- `cache-thrash`: `clflushopt` and large-stride sweeps over an LLC-sized buffer;
- `tlb-thrash`: one touch per 4 KiB page over a large region;
- `mem-bandwidth`: non-temporal stores on N threads;
- `page-fault-storm`: commit/touch/decommit loops and working-set trims of the Lab process itself;
- `disk-burst`: unbuffered write-through writes to a temporary file, deleted afterwards;
- `timer-resolution`: `timeBeginPeriod` toggles in the Lab process (per-process since Windows 10 2004: it changes
  the Lab's own pacing, which is the point);
- `priority-contention`: a high-priority Lab worker on the render thread's CPU.

#### Requirement: Ground truth and scenarios
Each injection SHALL append (cause id, start/end TSC, parameters) to a JSONL log. Scenarios SHALL be TOML files in
`lab/scenarios/` listing injections and expected causes. `fa lab run <scenario>` runs one with capture; scoring arrives
with `cause-engine`. Capture needs admin rights, so `fa lab run` is a developer command run from an elevated terminal.

#### Requirement: Guards
The Lab SHALL refuse to start while a matrix game runs; cap each injection at 30 s; cap disk bursts at 1 GB and require
10 GB free; stop every injector and delete temporary files on Ctrl+C.

##### Scenario: Game running
- **WHEN** `Overwatch.exe` is running and `fa lab run` is called
- **THEN** the Lab refuses and names the process

---

## collector-ipc

Introduced by change `collector-service` (lot 7).

### Purpose
An elevated headless collector and a non-elevated UI that share live data safely, with a strictly validated command
channel across the privilege boundary.

### Requirements

#### Requirement: Elevated collector
`fa-collector` SHALL be a headless binary started at logon by a scheduled task "FrameAutopsy Collector" (highest
privileges, current user), installed and removed by `fa install` / `fa uninstall` (one UAC prompt each). A named mutex
SHALL keep it single-instance.

#### Requirement: Install location
`fa install` SHALL copy the binaries to `%ProgramFiles%\FrameAutopsy` and create `%ProgramData%\FrameAutopsy` with an
ACL granting write access to SYSTEM and Administrators only (users read). The scheduled task SHALL point only at the
Program Files copy, never at a user-writable path such as `target\`.

##### Scenario: Replaced binary
- **WHEN** a non-elevated process tries to overwrite `fa-collector.exe` in the installed location
- **THEN** access is denied and the next logon still runs the original binary

#### Requirement: Untrusted user-writable inputs
Files the collector reads from user-writable locations (overrides, health baselines in `%LOCALAPPDATA%`) SHALL be
validated like pipe messages: size cap, strict schema, overrides may only disable.

#### Requirement: Shared memory
The collector SHALL create a named file mapping in the session namespace with a DACL granting full access to SYSTEM and
Administrators and read-only access to the interactive user. The layout SHALL start with a versioned header (magic,
version, size) followed by seqlock-protected slots (live frametime ring, latest probe values, status). The UI SHALL map
it with `FILE_MAP_READ` only.

#### Requirement: Cross-process seqlock
`fa-sync` SHALL implement the seqlock with atomics only: the writer makes the sequence odd, issues a Release fence,
writes the payload as relaxed atomic stores of 64-bit words, then makes the sequence even with Release; the reader
loads the sequence (Acquire), copies the payload with relaxed atomic loads, issues an Acquire fence, reloads the
sequence and retries if it changed or was odd. Volatile access SHALL NOT be used (concurrent non-atomic access is a
data race, which Miri reports). Readers SHALL never form references (`&T`, `&[u8]`) into the shared region.

##### Scenario: No torn snapshot
- **WHEN** the writer updates a slot in a tight loop and the reader reads concurrently for 10 s
- **THEN** every snapshot returned passes its checksum

#### Requirement: Command channel
UI→collector commands SHALL go through a named pipe whose DACL allows only the interactive user and Administrators.
Messages are length-prefixed MessagePack, at most 64 KiB, with a strict schema. Allowed commands: `GetStatus`,
`ApplyProposal{id}`, `RejectProposal{id}`, `SetAutonomy{rule_id, game_id, mode}`, `StartExperiment{rule_id, game_id}`,
`StopExperiment{id}`, `EmergencyStop`. Commands reference ids created by the collector, never raw PIDs, paths or
arguments.

##### Scenario: Forged proposal
- **WHEN** the UI sends `ApplyProposal` with an id the collector never issued
- **THEN** the command is rejected and logged, and nothing is applied

##### Scenario: Oversized message
- **WHEN** a client sends a 100 KiB message
- **THEN** the connection is closed without parsing it

#### Requirement: Overhead budget
During a 30-minute game session the collector SHALL average under 1 % of one logical CPU and under 50 MB private
working set (calibrate), measured by itself and shown in the dashboard.

#### Requirement: Anti-cheat smoke test
At the end of the lot, the user SHALL run the collector in observe-only mode during unranked matches of each matrix game
with anti-cheat active, and record the result in `docs/compliance.md`.

---

## incident-store

Introduced by change `incident-store` (lot 8).

### Purpose
Keep the recent past in memory, freeze it around each stutter, compress it and store it with bounded disk usage.

### Requirements

#### Requirement: In-memory window
`fa-core` SHALL keep the last 10 s of frames, events (aggregated per series) and probe samples per tracked game in a
bounded ring (const generics, RiftMind style). No raw continuous kernel trace SHALL be persisted.

#### Requirement: Stutter trigger
A frame SHALL trigger an incident when its frametime exceeds 2× the rolling median of the last second AND the median
+ 4 ms (calibrate). Incidents closer than 1 s SHALL coalesce.

#### Requirement: Snapshot content
An incident SHALL hold the window from −10 s to +2 s: frames, event series, probe samples, a process table (names and
PIDs, no handles) and system state (power source, probe clock, NVML when present).

#### Requirement: Timestamp codec
Timestamps SHALL be encoded as delta-of-delta + zigzag + Stream VByte, then zstd. Candidates: scalar varint (reference),
intrinsics (`pshufb`) and asm. The codec stage ships only if it beats "delta + zstd" on ratio or speed; among
implementations the fastest that beats the scalar reference on decode throughput ships.

#### Requirement: Storage and retention
Records SHALL be stored as MessagePack + zstd blobs in SQLite (WAL) with tables `sessions`, `incidents`, `probes`,
`health_checks`, `experiments`. Retention SHALL be 30 rolling days AND 2 GB (oldest first), configurable.

##### Scenario: Cap reached
- **WHEN** the store exceeds 2 GB
- **THEN** the oldest incidents are deleted until it is under the cap

#### Requirement: Session summary
Each game session SHALL record average FPS, 1 % low, 0.1 % low, P99 frametime and stutter count per 10 minutes.

#### Requirement: In-game probe sampling
During a game, the collector SHALL run probes only on logical CPUs not hosting the game's hot threads (from context
switches), with an average duty cycle of at most 0.01 % per CPU (calibrate), and store the samples in the window and
in incident snapshots.

---

## dashboard

Introduced by change `dashboard` (lot 9).

### Purpose
A fast egui interface, opened outside games, to read sessions, incidents, health checks, proposals and experiments.

### Requirements

#### Requirement: Fast and light
`fa-ui` (egui/eframe, non-elevated) SHALL show its first frame in under 300 ms and stay under 60 MB working set
(calibrate).

#### Requirement: Views
The UI SHALL offer: sessions list (game, date, duration, FPS, lows, incidents); session timeline (frametime plot with
incident markers and probe overlays, min/max decimation per pixel for 100k+ points); incident detail (window, causes,
attribution when available); health check report; live view from shared memory when the collector runs; proposals and
experiments (from lots 12–13).

#### Requirement: Read-only data access
The UI SHALL read SQLite read-only and the shared memory read-only. Every action goes through the command channel.

#### Requirement: Not required during play
`fa-ui` MAY run during play (live view on a second screen, overlay) but nothing SHALL require it. Proposals in
`confirm` mode SHALL be queued and reviewed after the match; no window or notification SHALL take focus during a match.

#### Requirement: Export
Sessions, incidents and experiments SHALL be exportable as Markdown or JSON to a user-chosen path.

#### Requirement: Language and accessibility
UI text SHALL be French (Claude's call). Navigation SHALL work with the keyboard, and color SHALL never be the only
carrier of meaning.

---

## cause-engine

Introduced by change `cause-engine` (lot 10).

### Purpose
Rank the likely causes of each incident with declarative TOML rules, and measure the diagnosis against the Lab's ground
truth.

### Requirements

#### Requirement: Declarative rules
Rules SHALL live in `rules/causes.toml` (defaults embedded; a user override may change thresholds, not add action
types). Each rule maps conditions on features to a cause label and a French evidence template.

#### Requirement: Features
Per incident, the engine SHALL extract at least: CPU time taken from game threads on the game's CPUs, per process
(context switches); DPC/ISR time per driver module (from the `etw-capture` module map; totals only if the lot 5 spike
showed addresses are unavailable) and longest single DPC; hard page faults (game vs system); disk I/O
latency and bytes per process; effective clock drop vs baseline; wake-up latency spike; present mode or GPU queue
changes; power source changes.

#### Requirement: Lagged cross-correlation
For each event-rate series (1 ms bins over the window) the engine SHALL compute its correlation with the frametime
series for lags −200 ms…+200 ms. Kernel: f32 FMA dot products. References: scalar Rust and an FFT approach (`rustfft`);
the fastest correct implementation ships. (Rationale: Rust does not auto-vectorize float reductions under strict FP
semantics.)

#### Requirement: Ranking and honesty
Causes SHALL be scored (rule weight × evidence strength) and the top 3 shown with explanations. When no rule passes its
threshold, the result SHALL be "cause inconnue".

#### Requirement: Hot path untouched
Diagnosis SHALL run on a worker thread; the ETW → transport → window → trigger path stays allocation-free.

#### Requirement: Lab fixtures and scoring
`fa lab campaign` (developer command, elevated terminal) SHALL run the scenario set live and record each run as
incident snapshots (lot 8 format) plus ground truth in `lab/fixtures/`, versioned, under a 50 MB cap. It is rerun
whenever capture or decoding changes. `fa lab score` SHALL score these fixtures offline (no admin, no GPU, under a
minute) and report precision and recall per cause. The gate requires precision and recall ≥ 0.8 on single-cause
scenarios and top-3 recall ≥ 0.6 on two-cause scenarios (calibrate).

#### Requirement: Frequency probe verdict
Using Lab soaks and stutter-instant samples, the change SHALL decide whether the frequency probe (ASM-02) adds
information over the PDH counter, and record the verdict in `docs/asm-unsafe-catalog.md`.

##### Scenario: Injected disk burst
- **WHEN** the Lab injects a 2 s disk burst during rendering
- **THEN** the top cause of the resulting incident is disk I/O by the Lab process

#### Requirement: Real fixtures
Recorded incident snapshots (process names only) SHALL be kept as regression fixtures, RiftMind `diff_runner` style.

---

## driver-attribution

Introduced by change `driver-attribution` (lot 11).

### Purpose
Name the driver, and when public symbols exist the function, behind long DPCs and ISRs.

### Requirements

#### Requirement: Builds on the lot 5 spike
The scope SHALL follow the 24H2 spike recorded in lot 5. If kernel addresses are unavailable, attribution SHALL degrade
to whatever remains, with an explanation, or be disabled. FrameAutopsy SHALL NOT enable `SeDebugPrivilege`.

#### Requirement: Collector stores, UI resolves
The collector SHALL store module + offset and the module's CodeView RSDS GUID and age (read from the file on disk with
`goblin`). Function-level resolution (`.pdata`, PDB parsing, downloads) SHALL run only in non-elevated processes
(`fa-ui`, `fa`) at display time, so files from the internet are never parsed at high integrity. Download consent is
given in the UI; no pipe command is involved.

#### Requirement: Attribution levels
Every address SHALL resolve to module + offset; with `.pdata` (`goblin`) to a function start (`sub_<rva>`); with a PDB
(`pdb` crate) to a function name.

#### Requirement: Opt-in symbols
PDBs SHALL be downloaded only after explicit consent, from the Microsoft public symbol server and NVIDIA's public symbol
server (URL to verify), cached in `%LOCALAPPDATA%\FrameAutopsy\symbols` under a 2 GB cap. `fa-attrib` SHALL be the only
crate that reaches non-loopback hosts (a dependency test enforces it); the only other network use is the
loopback-only LoL client of `game-profiles`.

#### Requirement: Game modules stay coarse
Frames inside matrix games' user-mode modules SHALL be aggregated to the module name only: no symbol download and no
function-level attribution (no reverse engineering of games).

##### Scenario: Game frame in a stack
- **WHEN** a sampled stack contains an address inside `VALORANT-Win64-Shipping.exe`
- **THEN** the report shows only the module name for that frame

#### Requirement: Own unwind info self-check
A test SHALL parse FrameAutopsy's own binaries with `goblin` and verify that every function containing asm has a valid
`.pdata` entry.

#### Requirement: Report line
Incident details SHALL list the top DPC/ISR offenders, for example
"nvlddmkm.sys!<fonction|sub_rva> — 412 DPC, max 1,2 ms, total 38 ms dans la fenêtre".

---

## governor

Introduced by change `governor` (lot 12).

### Purpose
Reversible actions on other processes and the system, never on the game, journaled so that a crash cannot leave the
machine modified.

### Requirements

#### Requirement: Actions v1
The governor SHALL implement three fully reversible actions (Claude's call):
- A1 EcoQoS (`ProcessPowerThrottling`, execution speed) + `BELOW_NORMAL` priority for selected background processes;
- A2 CPU Sets: `SetProcessDefaultCpuSets` on background processes to keep them off the CPUs hosting the game's hot
  threads (prefer E-cores or the other CCD per `cpu-topology`);
- A3 power: switch the power overlay to best performance (`PowerSetActiveOverlayScheme`, possibly undocumented:
  verify, fallback `PowerSetActiveScheme` with the previous scheme restored) on AC.
Later actions (I/O priority, memory priority, standby-list purge) each require a matrix entry and an A/B proof before
shipping. Timer resolution is not an action: since Windows 10 2004 it only affects the process that requests it.

#### Requirement: Targets and rights
Actions SHALL go through `TargetProcess` only, with minimal access rights (`PROCESS_QUERY_LIMITED_INFORMATION` and
`PROCESS_SET_LIMITED_INFORMATION`, plus `PROCESS_SET_INFORMATION` for priority class).

#### Requirement: Write-ahead journal
Before applying an action, the governor SHALL write an undo record (target identity = PID + creation time, previous
value, checksum) to a memory-mapped journal in `%ProgramData%\FrameAutopsy` (admin-only write), flush it (`FlushViewOfFile` + `FlushFileBuffers`), apply, then mark it
applied. Undo marks the record reverted.

#### Requirement: Watchdog and recovery
`fa-watchdog`, started by the collector with the same rights, SHALL revert all applied records when the collector dies.
It detects the death by waiting on the collector's single-instance named mutex (`WAIT_ABANDONED`), so no process
handle is needed.
At collector startup any unreverted record SHALL be reverted. Undo SHALL verify target identity first (PID reuse) and
skip targets that no longer exist.

##### Scenario: Collector killed mid-session
- **WHEN** the collector is killed while A3 is applied
- **THEN** the watchdog restores the previous power overlay within 2 s

#### Requirement: Revert triggers
All actions SHALL be reverted on game exit, emergency stop, collector shutdown, UI command and experiment phase switch.

#### Requirement: Emergency stop
A global hotkey registered with `RegisterHotKey` (no keyboard hook) SHALL revert everything immediately.

#### Requirement: Autonomy earned by proof
Per rule and game, the mode SHALL be `propose` (default), `confirm` or `auto`. `auto` is allowed only when the rule has
a passing A/B result for that game on this machine, and is revoked if a later experiment fails. (Claude's call, from
the profile: autonomy is earned by proof.)

#### Requirement: Action log
Every action SHALL be logged with time, rule, target, previous and new value, and revert time.

---

## ab-experiment

Introduced by change `ab-experiment` (lot 13).

### Purpose
Prove, on this machine and this game, whether a rule improves the worst frames, with honest statistics, and switch off
rules that prove nothing.

### Requirements

#### Requirement: Protocol (paired 10-minute blocks)
An experiment, started explicitly by the user, SHALL split normal play into 10-minute blocks grouped in pairs of
consecutive blocks. Within each pair the order rule-off (A) / rule-on (B) is randomized, and the governor toggles the
rule at block boundaries, possibly mid-match (decision D12). The first 60 s after each switch are excluded (washout).
A block with less than 5 minutes of valid frames (menus, loading, present-mode change, game exit) is discarded with its
pair. An experiment may span several play sessions. The pair is the statistical unit (frames are autocorrelated).

##### Scenario: Game closed mid-block
- **WHEN** the game exits 3 minutes into a B block
- **THEN** the block and its pair are discarded, the rule is reverted, and the next play session starts a new pair

#### Requirement: Comparability control
A pair SHALL be excluded, with its reason shown, if between its two blocks the median effective clock differs by more
than 5 %, the power source differs, the GPU power limit differs, or the starting GPU temperature differs by more than
5 °C (calibrate).

#### Requirement: Metrics
Primary metric: 1 % low per block. Secondary: 0.1 % low and stutters per 10 minutes.

#### Requirement: Test
Significance SHALL come from an exact two-sided sign-flip permutation test on the within-pair differences (all 2ⁿ
labelings). The test SHALL be run at two looks, after 8 and after 12 valid pairs, each at α = 0.025 (Bonferroni,
overall α ≤ 0.05). With 8 pairs the smallest two-sided p is 2/256 ≈ 0.008; with 5 pairs it would be 0.0625, which is
why fewer than 8 pairs never conclude.

#### Requirement: Effect size
The effect SHALL be reported as the % change in 1 % low with a 95 % confidence interval from a two-stage bootstrap
(pairs, then 1 s sub-blocks within each block), 10,000 resamples.

#### Requirement: Decision
A rule is "proven" when p < 0.025 at a look and the CI lower bound shows improvement; "harmful" when significantly
worse (then disabled); "inconclusive" otherwise (stays `propose`). After 12 valid pairs without proof it is disabled for
that game. The report sentence SHALL read: "Cette règle améliore ton 1 % low de X % (IC 95 % : a–b %) sur <jeu>, sur
cette machine, d'après N paires de blocs de 10 min."

#### Requirement: SIMD kernels
The bootstrap SHALL use a vectorized f32 selection kernel and a 4-lane AVX2 xoshiro256++ (8 lanes with AVX-512 when
present). References: `select_nth_unstable_by` and scalar xoshiro256++. Parity: identical seeds give identical
resample indices and identical selected values.

#### Requirement: Validation with the Lab
Offline Monte Carlo on recorded Lab blocks SHALL show detection of a known improvement in at least 80 % of simulated
experiments and a false-positive rate of at most 5 % over at least 100 simulated null experiments, both looks included.

---

## game-profiles

Introduced by change `game-profiles` (lot 14).

### Purpose
Per-game behavior within the matrix, and the anti-cheat test protocol that moves verdicts from `unverified` to
`allowed`.

### Requirements

#### Requirement: Profiles
Profiles SHALL exist for `lol`, `valorant`, `fortnite`, `overwatch2`, `minecraft-java` (completed in lot 15) and
`generic`. Each holds detection rules, enabled observers and actions from the matrix, and default rules.

#### Requirement: League of Legends events
For `lol`, FrameAutopsy MAY poll Riot's official Live Client Data API (`https://127.0.0.1:2999/liveclientdata/`) at
1 Hz at most, only to timestamp game events for post-match correlation. The client SHALL be loopback-only (a test
asserts the host is `127.0.0.1`). Nothing derived from it is displayed during the match. Verdict to confirm in
`docs/compliance.md` before enabling.

#### Requirement: System-only games
For `valorant`, `fortnite` and `overwatch2`, FrameAutopsy SHALL use system observers and governor actions on other
processes only; no game data source of any kind. VALORANT's unofficial local client API SHALL NOT be used.

#### Requirement: Anti-cheat test protocol
Before an action or the overlay is enabled for a game, the user SHALL run it in at least 3 unranked matches with the
anti-cheat active and record: date, game version, FrameAutopsy commit, items tested, result. A clean record moves the
items to `allowed` with a link to the record; any warning, kick or notice moves them to `forbidden` and stops the
rollout.

---

## minecraft-jvm

Introduced by change `minecraft-jvm` (lot 15).

### Purpose
Explain Minecraft Java client stutters caused by the JVM (GC pauses, safepoints) and prove JVM argument changes by A/B,
without touching the game, the launcher or the mods.

### Requirements

#### Requirement: Detection
The client SHALL be detected as `javaw.exe`/`java.exe` owning a top-level window whose title starts with "Minecraft"
(`EnumWindows`, no process handle).

#### Requirement: hsperfdata counters
FrameAutopsy SHALL read `%TEMP%\hsperfdata_<user>\<pid>` (the file the JVM publishes for `jstat`), parse its header
(magic `0xcafec0c0`, byte order, version, entries) and entries, and sample GC counters, heap capacities and safepoint
time at 10 Hz. Reference access: plain file reads; a shared mapping read through `fa-sync` relaxed atomic copies ships
only if it measurably beats file reads. When `-XX:-UsePerfData` is set, the feature reports itself unavailable.

#### Requirement: JFR
When the user enables a recording with the argument string FrameAutopsy generates, the `.jfr` file SHALL be parsed
after the session (existing crate if it covers GC and safepoint events, otherwise a safe parser). JFR ticks SHALL be
mapped to the TSC timebase (mapping to verify on Windows).

#### Requirement: GC correlation
GC pause intervals SHALL be correlated with frametime spikes and feed a "pause GC" cause rule in `cause-engine`.

#### Requirement: JVM argument A/B
FrameAutopsy SHALL propose argument sets as text that the user applies in CurseForge, and tag sessions by the JVM
arguments read from hsperfdata (`java.rt.vmArgs`). It SHALL never modify Minecraft, CurseForge or mod files, and never
interact with servers.

---

## overlay

Introduced by change `overlay` (lot 16).

### Purpose
A small in-game overlay of system metrics, in a separate window, enabled per game only after a clean anti-cheat test.

### Requirements

#### Requirement: Separate window
The overlay SHALL be a window of the `fa-ui` process with `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST |
WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`, click-through, never taking focus, rendered with Direct2D + DirectComposition
through `fa-win`. No injection, no hook, no capture of the game window.

#### Requirement: Content
It SHALL show a 5 s frametime sparkline, current FPS and 1 % low of the live window, the probed clock when enabled and
an incident counter, updated at 10 Hz. It SHALL never show game data.

#### Requirement: Per-game enablement
The overlay SHALL be off by default for every game except `fa-lab`, and enabled for a game only when its `overlay`
verdict is `allowed`.

#### Requirement: Fullscreen behavior
Behavior SHALL be documented per game: invisible in exclusive fullscreen; visible in composed (flip/borderless) modes,
to verify.

#### Requirement: Cost
An overlay update SHALL cost at most 0.2 ms of CPU (calibrate). A `RegisterHotKey` hotkey toggles it.

---

## Out of scope

Kernel drivers; MSR or `rdpmc` access; any handle, read, write, injection or hook on game processes; modifying game,
launcher or mod files; changing the game's priority or affinity; input automation; network interception; live
competitive information; `SeDebugPrivilege`; function-level symbolization of game modules; JIT compilation of rules;
a timer-resolution action (per-process since Windows 10 2004); WAITPKG/MWAITX waits (absent from the target CPU);
paid services; telemetry without explicit consent; Minecraft servers.
