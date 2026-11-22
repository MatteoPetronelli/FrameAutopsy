# driverless-probes Specification

## Purpose
Measure from user mode, with no driver, what monitoring tools usually read through drivers (which Vanguard blocks):
effective clock, memory latency and bandwidth, cache sizes, thread wake-up latency. Probes run in FrameAutopsy's own
threads only.

## Requirements

### Requirement: Pinned execution
Each probe SHALL run on a FrameAutopsy thread pinned to the chosen logical CPU (`SetThreadSelectedCpuSets` or
`SetThreadAffinityMask` on our own thread) and timestamp its result in TSC ticks.

#### Scenario: Pinned probe
- **WHEN** a probe runs
- **THEN** it runs on a FrameAutopsy thread pinned to the chosen logical CPU and timestamps its result in TSC ticks

### Requirement: Effective frequency probe
The probe SHALL execute an asm chain of dependent `add` instructions (1-cycle latency) sized to last about 50 µs and
compute effective frequency = chain length / elapsed TSC time. Samples whose elapsed time exceeds 1.5× the recent
median (preemption) SHALL be discarded. Asm is mandatory: the compiler folds such a chain.

#### Scenario: Comparison with the Windows counter
- **WHEN** the probe and PDH `\Processor Information(*)\% Processor Performance` × nominal frequency are sampled together under steady load
- **THEN** they agree within 3 % (calibrate)

#### Scenario: Probe must add information
- **WHEN** the Lab soak shows the probe captures no change that the PDH counter smooths away
- **THEN** the probe is removed under the earn-its-place rule and PDH is used

Lot 3 establishes accuracy only. The keep/remove decision is taken in lot 10 (`cause-engine`), once Lab soaks and
samples taken at the instant of a stutter exist. A probe on a CPU the game is not using reflects the game's CPUs only
when they share a clock domain; on per-core-clock CPUs it shows package-level throttling only. Lot 3 checks which case
applies to the target machine (probe one core while another is loaded).

### Requirement: Memory latency probe
The probe SHALL pointer-chase a buffer of at least 4× the LLC size, arranged as a random cyclic permutation (Sattolo),
on 4 KiB pages and on 2 MiB large pages when `SeLockMemoryPrivilege` is available (fallback otherwise), and report ns
per load. Reference: a Rust `read_volatile` chain; asm ships only if its measurements are more stable (lower
coefficient of variation).

#### Scenario: Pointer chase
- **WHEN** the memory latency probe runs
- **THEN** it pointer-chases a buffer of at least 4× the LLC size as a random cyclic permutation (Sattolo), on 4 KiB pages and on 2 MiB large pages when `SeLockMemoryPrivilege` is available, and reports ns per load

### Requirement: Memory bandwidth probe
The probe SHALL measure read (AVX2 loads), write (non-temporal stores) and copy bandwidth, multi-threaded across
physical cores, in GB/s. Reference: intrinsics.

#### Scenario: Bandwidth modes
- **WHEN** the memory bandwidth probe runs
- **THEN** it measures read (AVX2 loads), write (non-temporal stores) and copy bandwidth, multi-threaded across physical cores, in GB/s

### Requirement: Cache curve
The probe SHALL measure latency for working sets from 16 KiB to 256 MiB, detect plateaus as L1/L2/L3 sizes, and
compare them with `cpu-topology`.

#### Scenario: Working-set sweep
- **WHEN** the cache curve probe runs
- **THEN** it measures latency for working sets from 16 KiB to 256 MiB, detects plateaus as L1/L2/L3 sizes and compares them with `cpu-topology`

### Requirement: Wake-up latency probe
The probe SHALL measure the lateness of a thread woken by a high-resolution waitable timer and report p50/p99/max in
µs. (`tpause`/`umwait`/`mwaitx` waits were dropped: the target CPU has neither WAITPKG nor MWAITX, so they could
never be validated.)

#### Scenario: Timer lateness
- **WHEN** the wake-up latency probe runs
- **THEN** it measures the lateness of a thread woken by a high-resolution waitable timer and reports p50/p99/max in µs

### Requirement: Off-game only in this lot
In lot 3 probes SHALL run off-game only. In-game sampling needs context switches and is specified in `incident-store`
(lot 8).

#### Scenario: Lot 3 probes
- **WHEN** probes run in lot 3
- **THEN** they run off-game only

### Requirement: Calibration record
A documented manual procedure SHALL compare probe results with HWiNFO off-game; results are recorded in
`docs/asm-unsafe-catalog.md`.

#### Scenario: HWiNFO comparison
- **WHEN** probe results are compared with HWiNFO off-game by the documented manual procedure
- **THEN** the results are recorded in `docs/asm-unsafe-catalog.md`
