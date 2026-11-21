# cpu-topology Specification

## Purpose
Know cores, SMT siblings, last-level-cache domains and efficiency classes, to pin probes and to place background
processes with CPU Sets.

## Requirements

### Requirement: Windows API as source of truth
Topology SHALL come from `GetSystemCpuSetInformation` (CPU Set ids, `EfficiencyClass`, core and LLC indexes) and
`GetLogicalProcessorInformationEx` (caches, groups, NUMA). The model maps logical CPU → physical core → LLC domain →
efficiency class.

#### Scenario: Intel hybrid CPU
- **WHEN** topology is read on a CPU with P-cores and E-cores
- **THEN** E-cores have a lower efficiency class than P-cores

#### Scenario: AMD dual-CCD CPU
- **WHEN** topology is read on a CPU with two CCDs
- **THEN** two LLC domains are reported

### Requirement: CPUID cross-check as a test only
A test SHALL compare the API model with CPUID leaves (`0x0B`/`0x1F`, `0x1A`, `0x8000001E`, `0x04`/`0x8000001D`) read
through `__cpuid_count` intrinsics and log mismatches. CPUID is not a runtime source (the API already exposes the
information; the cross-check is a validation and learning device, documented as such).

#### Scenario: Cross-check test
- **WHEN** the topology cross-check test runs
- **THEN** it compares the API model with the CPUID leaves read through `__cpuid_count` intrinsics and logs mismatches, and CPUID is not a runtime source

### Requirement: Synthetic topologies
The hybrid and dual-CCD scenarios SHALL be tested with synthetic API buffers, because the target machine
(i5-11400H: 6 identical cores, one LLC) has neither; real validation waits for another machine.

#### Scenario: Hybrid and dual-CCD
- **WHEN** the hybrid and dual-CCD scenarios are tested
- **THEN** they use synthetic API buffers, not the target machine
