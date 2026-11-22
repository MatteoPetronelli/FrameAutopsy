# Design

## Context

Lots 2 and 3 provide the TSC clock (`tsc-clock`), topology (`cpu-topology`) and driverless probes (frequency ASM-02,
latency ASM-03, bandwidth ASM-04, wake-up jitter), all non-elevated. This lot assembles them into the `fa check` flow
and adds the one new kernel, ASM-06. Budget: `fa check` at most 90 s (`docs/architecture.md` section 6). See
`proposal.md` for motivation and compliance effect. Target machine: i5-11400H, six identical cores, AVX2 + AVX-512, no
WAITPKG or MWAITX.

## Goals / Non-Goals

**Goals:**
- A verdict a gamer can read, backed by numbers they can see (curve, bandwidth, p99), never a bare label.
- Say "nothing found" plainly and list what was checked.
- ASM-06 earns its place or is deleted; either way the load exists for the Lab.

**Non-Goals:**
- No fix is applied and no setting is written: `fa check` only reads.
- No GPU load test and no thermal sensor reads (they need a driver or admin; forbidden).
- No in-game comparison logic; this lot only stores the baseline.
- No automatic scheduling of the check (manual command).

## Decisions

**D-1 Flow and 90 s budget.** `fa check` runs sequential phases under one deadline and aborts cleanly at 85 s:

| Phase | Budget | Source |
|---|---|---|
| refusal checks (game, battery warning), CPU identification | under 1 s | Toolhelp snapshot, CPUID, lot 2 |
| single-core peak, each core in turn (the probe keeps its core busy: a peak, not an idle reading) | 4 s (measured 3.7 s) | ASM-02 probe, lot 3 |
| 60 s all-core load + 1 Hz frequency sampling | 60 s | ASM-06 + ASM-02 |
| memory latency, bandwidth, cache sizes | 6 s (measured 2.9 s with a sweep up to 64 MiB, 2 runs per point) | ASM-03, ASM-04 |
| scheduling wake-up p50/p99/max | 4 s (measured 3.2 s) | lot 3 timer probe |
| WMI, NVML, power queries | about 3 s, overlapped with other phases | `wmi`, `nvml-wrapper`, `fa-win` |

Measured on 2026-11-22 (AC, performance mode): 3.7 + 2.9 + 3.2 + 60.7 s = 70.5 s in total, so about 15 s of margin under the 85 s deadline. Memory and scheduling probes run after the load only if the hot CPU does not
distort them; the order is compared once at implementation and the choice is logged in `docs/decisions.md` section 5.
The sampler thread reads the frequency of its own core once per second while one load thread runs on one logical CPU of
every OTHER physical core (the sampler's core and its SMT sibling are excluded; stated in the report). The sibling must
stay idle: with it busy the ASM-02 probe reads about 9 % low (3.71 GHz against 4.08 on PDH, lot 3), which would make a
healthy machine look throttled; with it idle probe and PDH agree within 0.2 %. The CPU choice is a pure function of the
topology (`fa-probe::bandwidth::physical_core_cpus` plus the core of the sampler), unit tested on synthetic topologies.

**D-2 ASM-06 FMA load (`fa-asm`).** The catalog line exists (`pending`) and is updated before the code. Reference:
intrinsics. Candidate: `asm!` Intel syntax, 8 independent accumulator chains (`vfmadd231ps` on `ymm` for AVX2, `zmm`
for AVX-512), so the loop is port-bound and not latency-bound (4-cycle latency x 2 ports needs at least 8 chains). Why
asm may win: the compiler cannot reorder or merge the chains and the registers are fixed. Why it may lose: LLVM already
emits this loop from intrinsics; a tie is the expected result, and on a tie (win under 5 %) intrinsics ship and the asm
module is deleted. Metric: FLOP per TSC tick (closeness to 2 FMA per cycle) and the resulting frequency-drop curve.
Parity test: same FLOP count and same final accumulator values for fixed inputs. Runtime fallback: AVX-512 to AVX2 to a
scalar `f32::mul_add` loop (`FA_FORCE_SCALAR=1` forces the scalar one). The asm block is a fixed-count inner loop of
register-only instructions; the stop flag is an `AtomicBool` read in safe Rust between blocks of about 1 ms.
AVX-512 can lower the clock on some Intel parts independently of heat: the report names the ISA used and the verdict
compares against the first 5 s of the same variant. Alternative considered: a Rust `black_box` loop (rejected, the
compiler decides port pressure).

**D-3 Verdicts (`fa-probe`, safe).** Pure functions from a measurement struct to a list of verdicts; each has `id`,
`fired`, `confidence` (low, medium, high), `evidence` (numbers and the rule threshold) and a French sentence. Starting
rules, all `(calibrate)`:
- throttling: median all-core frequency per second; fires when the median after the first 5 s drops more than 10 %
  below the first-5 s median. Text "bridage apres X s, -Y %", X being the first second where the 3 s moving median is
  under 90 %, with the curve. Confidence high when the drop is monotonic over 3 s or more, medium when it recovers.
- frequency cap (added 2026-11-22 after the first AC and battery runs): the median clock held under the load against the
  highest single-core peak of the same run; fires when it is more than 15 % below. Why: on battery the target machine
  held 2.49 GHz flat for 60 s against a 3.39 GHz peak (-26 %); the throttling rule, which compares with the first 5 s of the
  same curve, saw nothing and said "pas de bridage". On AC the same load holds 4.09 against 4.49 (-9 %), under the
  threshold. Calibrated on two runs of one machine.
- single-channel RAM likely: the WMI module count decides. One module fires the verdict (high confidence when the
  all-core READ bandwidth is also under 60 % of the dual-channel theoretical value, speed x 8 B x 2; medium otherwise);
  more than one module does not fire it. When WMI fails there is no module count and no configured speed, hence no theoretical bandwidth: no RAM verdict (low confidence, said in the report). Why not
  bandwidth first: lot 3 measured about 32 GB/s read and 26 to 30 GB/s write or copy, +-10 % between runs, which is
  right at 60 % of 51.2 GB/s (DDR4-3200), so the bandwidth cannot decide alone. Read only: the least noisy mode.
- scheduling interference likely: wake-up p99 above 5 ms. Lot 3 measured on the idle target machine a lateness p50 of
  about 0.5 ms and a p99 of 2 to 3 ms (timer resolution), so the first draft of 500 us would have fired on every run.
  5 ms is about twice the idle p99; it is a starting value, frozen at task 7.3 after real reports.
Missing inputs (WMI failed, NVML absent) lower confidence or skip a line and say so; they never fire a verdict. Unit
tests cover the two spec scenarios (4.2 to 3.1 GHz after 25 s gives "bridage apres 25 s, -26 %"; healthy desktop) from
fixtures, and each threshold gets a mutation check (`mutation-verified-testing`).

**D-4 Report and JSON.** One serde `HealthReport` struct rendered twice: French text for the terminal and the JSON file
`%LOCALAPPDATA%\FrameAutopsy\health\<timestamp>.json` (UTC, `YYYYMMDDTHHMMSSZ`), with `schema_version`, raw
measurements, verdicts, power state and the list of checks run. The JSON keeps numbers, not translated strings, so later
lots can compare. A "nothing found" report is a plain sentence plus the list of what was checked with its values. The
curve is printed as a text table, one line per 5 s.

**D-5 Game refusal without a handle.** `fa-win` wraps `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` and
`Process32NextW`, returns full image names, and closes the snapshot handle with an RAII guard. It never calls
`OpenProcess`. Names are compared with the matrix game list from `fa-core` by full-name, case-insensitive equality
(never a prefix; the 15-byte truncation only affects classic kernel events). The source test that confines `OpenProcess`
to `open_target` is extended to cover the new module. Minecraft is detected by window class (see Decisions).
Games outside the matrix are not refused.

**D-6 WMI and NVML.** `wmi` queries `Win32_PhysicalMemory` (capacity, `ConfiguredClockSpeed`, `Speed`, bank label) on a
dedicated thread with COM initialized, with a 3 s timeout; failure becomes "inconnu". `nvml-wrapper` loads `nvml.dll`
at run time and reads name, performance state and power limit. On Optimus laptops the dGPU may be asleep and NVML can
wake it, so the call is made once, read-only, and the report says so. No NVIDIA GPU or no DLL: section omitted.
Alternatives rejected: raw WMI COM through `windows` (more unsafe), PDH for the GPU (no P-state).

**D-7 Power.** Read-only calls in `fa-win` (WIN-10): `GetSystemPowerStatus` (AC or battery), `PowerGetActiveScheme`
with its friendly name, and the effective power overlay. On battery the command warns at start and still runs; the
report marks `power.source = "battery"` and throttling evidence quotes it.

**D-8 Baseline.** After writing the JSON, a run with `power.source = "ac"` is also written to
`health\baseline.json` (write to a temporary file, then rename). A battery run never overwrites it. Consumers read it
as untrusted input: size cap and strict schema (`deny_unknown_fields`), per `docs/architecture.md` section 5.

**D-9 Threshold freezing.** The thresholds above are starting values. The last task group runs `fa check` on the
user's laptop on AC then on battery and shows both reports to the user before any threshold is changed; changes are
logged in `docs/decisions.md` section 5.

## Risks / Trade-offs

- [ASM-06 ties intrinsics] -> intrinsics ship, the asm module is deleted, the numbers stay in the catalog; the load is
  still delivered.
- [60 s of full load heats the laptop] -> the user is told before it starts; Ctrl-C stops the load threads within 1 s;
  no setting is changed.
- [Throttling confused with AVX-512 license downclock or PL2 to PL1 expiry] -> the evidence prints the curve and the
  ISA used; PL2 expiry is a real power limit and is reported as throttling, which is honest for a gamer.
- [Background load (browser, antivirus) pollutes wake-up p99 and bandwidth] -> the report records system load before
  the tests, lowers confidence when it is high and asks to close other apps.
- [WMI slow or hung] -> hard timeout, section marked unknown.
- [NVML wakes a sleeping dGPU] -> single read-only call, noted in the report.
- [Only matrix games are refused] -> an unknown game running is not detected; documented in the report footer.
- [Starting thresholds are guesses] -> `(calibrate)` markers; frozen only after the user sees real AC and battery
  reports.

## Decisions (user, 2026-11-21)

- **Minecraft detection**: a `java.exe`/`javaw.exe` process counts as Minecraft only if it owns a visible top-level
  window whose title matches Minecraft. Window enumeration (`EnumWindows` + `GetWindowThreadProcessId`) needs no process
  handle. A `javaw.exe` without such a window (IDE, build tool, other Java app) is NOT a game and does not block
  `fa check`. Reading the command line is rejected: it needs a handle on a process not yet known to be a game.
- The exact title pattern is **unverified**: it is read from a real Minecraft window on the user's machine (client
  launched from the official launcher, then from the other launchers the user uses) before being coded. Never taken from memory.
