

## ADDED Requirements

### Requirement: Invariant TSC detection
The clock SHALL check CPUID `0x80000007:EDX[8]`. Without an invariant TSC it SHALL fall back to QPC as the timebase and
the health check SHALL report it.

#### Scenario: No invariant TSC
- **WHEN** CPUID `0x80000007:EDX[8]` reports no invariant TSC
- **THEN** the clock falls back to QPC as the timebase and the health check reports it

### Requirement: Read primitives
`fa-asm` SHALL provide `now()` (`lfence; rdtsc; lfence`) and `now_serialized()` (`rdtscp; lfence`). Asm candidates
compete with `_rdtsc`/`__rdtscp` + `_mm_lfence` intrinsics on overhead and jitter; the asm version ships only if it
wins.

#### Scenario: TSC reads
- **WHEN** `fa-asm` is built
- **THEN** it provides `now()` (`lfence; rdtsc; lfence`) and `now_serialized()` (`rdtscp; lfence`)

### Requirement: TSC frequency
The TSC frequency SHALL come from CPUID `0x15` when its crystal ratio is non-zero, otherwise from a calibration against
QPC: 5 runs of at least 200 ms, median kept. The value is stored in the health baseline.

#### Scenario: Frequency source
- **WHEN** the TSC frequency is determined
- **THEN** it comes from CPUID `0x15` when its crystal ratio is non-zero, otherwise from a QPC calibration of 5 runs of at least 200 ms with the median kept, and is stored in the health baseline

### Requirement: Conversions
Tick-to-nanosecond conversion SHALL use integer math (u128), without float drift. A QPC↔TSC mapping (offset and scale)
SHALL be re-estimated every 60 s and its drift recorded.

#### Scenario: Ten-second accuracy
- **WHEN** a 10 s interval is measured with both TSC and QPC
- **THEN** the elapsed times differ by less than 10 µs (calibrate)

#### Scenario: Cross-core consistency
- **WHEN** a test thread pinned successively to every logical CPU reads the TSC
- **THEN** readings are monotonic and the skew between cores is under 1 µs (calibrate)


