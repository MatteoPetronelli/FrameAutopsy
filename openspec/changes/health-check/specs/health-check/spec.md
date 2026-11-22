

## ADDED Requirements

### Requirement: Command and output
`fa check` SHALL run non-elevated in at most 90 s, print a French report and write JSON to
`%LOCALAPPDATA%\FrameAutopsy\health\<timestamp>.json`. It SHALL change no system setting.

#### Scenario: Check run
- **WHEN** `fa check` runs
- **THEN** it runs non-elevated in at most 90 s, prints a French report, writes JSON to `%LOCALAPPDATA%\FrameAutopsy\health\<timestamp>.json` and changes no system setting

### Requirement: Refuse during games
`fa check` SHALL refuse to start while a matrix game process is running (Toolhelp snapshot by image name, no handle) and
warn when on battery.

#### Scenario: League of Legends open
- **WHEN** `League of Legends.exe` is running
- **THEN** `fa check` exits with a message asking to close the game first

### Requirement: Report sections
The report SHALL contain the sections CPU, frequency, memory, scheduling, power and, when present, GPU. The frequency
section SHALL show a single-core peak per core and then a 60 s FMA load (ASM-06) on one logical CPU of every other
physical core, sampled every second on a core whose SMT sibling stays idle.

#### Scenario: Section details
- **WHEN** the report is produced
- **THEN** CPU shows model, topology, invariant TSC, TSC frequency and AVX2/AVX-512/WAITPKG/MWAITX; memory shows latency, bandwidth, cache sizes and the WMI `Win32_PhysicalMemory` modules with configured speed; scheduling shows wake-up p50/p99/max; power shows AC or battery, active scheme and overlay; GPU (NVML) shows name, P-state and power limit

#### Scenario: Report content
- **WHEN** the report is produced
- **THEN** it contains CPU, frequency, memory, scheduling, power, and GPU through NVML when present

### Requirement: Verdicts with evidence
Each verdict SHALL state its evidence and confidence, and the report SHALL say plainly when nothing was found. The
starting rules (calibrate) are given by the rule scenarios below.

#### Scenario: Throttling rule
- **WHEN** the median frequency after the first 5 s drops more than 10 % below the first-5 s median
- **THEN** the throttling verdict fires with "bridage après X s, −Y %"

#### Scenario: Single-channel RAM rule
- **WHEN** WMI shows one memory module
- **THEN** the single-channel RAM verdict fires, with high confidence when the all-core read bandwidth is also under 60 % of the dual-channel theoretical value for the configured speed and medium otherwise; with more than one module it does not fire, and with an unknown module count the bandwidth alone fires it with low confidence

#### Scenario: Scheduling rule
- **WHEN** the wake-up p99 is above 5 ms (the idle target machine measures p50 about 0.5 ms and p99 2 to 3 ms, so 500 µs would fire on every run)
- **THEN** the scheduling interference verdict fires

#### Scenario: Throttling laptop
- **WHEN** the all-core load median falls from 4.2 GHz to 3.1 GHz after 25 s
- **THEN** the report says "bridage après 25 s, −26 %" with the curve

#### Scenario: Healthy desktop
- **WHEN** no rule fires
- **THEN** the report says no problem was found and lists what was checked

### Requirement: Baseline
The latest health check run on AC power SHALL become the baseline for in-game comparisons and A/B covariates.

#### Scenario: Latest AC run
- **WHEN** a health check run on AC power completes
- **THEN** the latest such run becomes the baseline for in-game comparisons and A/B covariates


