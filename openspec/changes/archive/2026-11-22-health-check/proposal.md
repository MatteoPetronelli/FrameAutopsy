# Proposal

## Why

`fa check` is the first visible result of the project (decision D3: lots 0 to 4 need no admin, no ETW and no game). In
at most 90 s it gives a readable verdict on the machine itself (laptop throttling, single-channel RAM, scheduling
interference), the causes of stutter that need no game to be seen. It also produces the reference run that later lots
compare in-game measurements and A/B covariates against. Lots 2 and 3 (clock, topology, probes) provide the
measurements; without this lot they have no user-facing output.

Capability purpose (from `specifications.md`): `fa check`, in at most 90 s, a readable verdict on the machine (laptop
throttling, RAM configuration, scheduling interference) with no admin rights, no ETW and no game running. It becomes the
baseline for later comparisons.

## What Changes

- New subcommand `fa check` in the non-elevated `fa` binary: runs the lot 3 probes, a 60 s all-core FMA load sampled
  every second, a WMI memory query, an optional NVML query and a power-state query, then prints a French report and
  writes JSON to `%LOCALAPPDATA%\FrameAutopsy\health\<timestamp>.json`.
- New FMA power load (catalog ASM-06, AVX2 and AVX-512 variants) in `fa-asm`, benchmarked against intrinsics. It is born
  here and reused by the Lab (lot 6).
- Verdicts with evidence and confidence (throttling, single-channel RAM likely, scheduling interference likely) and an
  honest "nothing found" report that lists what was checked.
- Refusal to start while a game of the compliance matrix is running; warning when on battery.
- The latest run on AC power is recorded as the baseline for later comparisons.
- Verdict logic and report rendering in `fa-probe` (safe); thin Windows calls in `fa-win`.

## Capabilities

### New Capabilities
- `health-check`: the `fa check` command, its report sections, evidence-backed verdicts, game refusal, and the AC
  baseline.

### Modified Capabilities

None. The specs of lots 2 and 3 (`tsc-clock`, `cpu-topology`, `driverless-probes`) are consumed, not changed.

## Impact

- **Compliance.** `compliance.toml` is not changed and no matrix row is added: `fa check` is not an observer, an action
  or an overlay. It runs only when no matrix game is running and refuses otherwise. Games are detected by full image
  name in a Toolhelp snapshot, **without opening any handle on any process** (a snapshot handle is not a process
  handle; same information as Task Manager, `docs/compliance.md` section 4). The "never touch the game" boundary is
  unchanged: no `OpenProcess`, no game data, no ETW. `fa check` changes **no system setting** (power plan and overlay
  are read, never written). Games outside the matrix are not detected (documented limit).
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): **ASM-06** FMA power load, asm-candidate against intrinsics
  (`_mm256_fmadd_ps` / `_mm512_fmadd_ps`); result and verdict filled in this lot. Proposed new `fa-win` line **WIN-10**
  (unsafe-FFI, Claude's call): Toolhelp snapshot, `GetSystemPowerStatus`, power scheme and overlay reads; thin wrappers,
  exempt from benchmarks. The line is added to the catalog before the code. WMI (`wmi`) and NVML (`nvml-wrapper`) write
  no unsafe (catalog line "WMI, NVML, PDH").
- **Code:** `fa-asm` (ASM-06), `fa-win` (WIN-10), `fa-probe` (verdicts, report), `fa` (`check`), `fa-core` (matrix game
  names, read only).
- **Dependencies:** `wmi`, `nvml-wrapper`, `serde`, `serde_json`. NVML needs `nvml.dll` from the NVIDIA driver, loaded
  at run time; absent means the GPU section is skipped.
- **Storage:** new user-writable folder `%LOCALAPPDATA%\FrameAutopsy\health\`. The baseline is a user-writable input:
  when the collector reads it (lot 7 onward) it validates it like a pipe message (size cap, strict schema).
- **Docs:** the final thresholds are logged in `docs/decisions.md` section 5.
