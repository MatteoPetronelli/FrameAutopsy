

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
The report SHALL contain: CPU (model, topology, invariant TSC, TSC frequency, AVX2/AVX-512/WAITPKG/MWAITX); frequency
(idle per-core peak, then a 60 s all-core FMA load — ASM-06 — sampled every second); memory (latency, bandwidth, cache sizes, WMI
`Win32_PhysicalMemory` modules and configured speed); scheduling (wake-up p50/p99/max); power (AC or battery, active
scheme and overlay); GPU through NVML when present (name, P-state, power limit).

#### Scenario: Report content
- **WHEN** the report is produced
- **THEN** it contains CPU, frequency, memory, scheduling, power, and GPU through NVML when present

### Requirement: Verdicts with evidence
Each verdict SHALL state its evidence and confidence, and the report SHALL say plainly when nothing was found.
Starting rules (calibrate):
- throttling: median frequency drops more than 10 % from the first 5 s → "bridage après X s, −Y %";
- single-channel RAM likely: measured bandwidth under 60 % of the dual-channel theoretical value for the configured speed;
- scheduling interference likely: wake-up p99 above 500 µs.

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


