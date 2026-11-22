# Proposal

## Why

Lots 5 (`etw-capture`), 10 (`cause-engine`) and 13 (A/B) need ground truth: a stutter whose cause, start and end are
known to the tick. Real games cannot give it, and touching them is forbidden. The Stutter Lab provokes known stutters
in FrameAutopsy's own render loop, so capture, diagnosis and A/B can be tested without a game and without any
compliance question. It is also the portfolio demo (decision 18).

Capability purpose (from `specifications.md`): ground truth for capture, diagnosis and A/B, without a real game.

## What Changes

- New dev command `fa lab run <scenario>` (elevated terminal, because the lot 5 capture needs ETW). The Lab code lives
  in the module directory `crates/fa/src/lab/` (safe Rust).
- Own render loop: window, D3D11 device, flip-model swap chain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`), animated scene at 60,
  144 or uncapped FPS; paced without sleep jitter (design D-2).
- Nine injectors: `cpu-burn`, `frame-stall`, `cache-thrash`, `tlb-thrash`, `mem-bandwidth`, `page-fault-storm`,
  `disk-burst`, `timer-resolution`, `priority-contention`.
- JSONL ground-truth journal in TSC ticks, and TOML scenarios in `lab/scenarios/` (10 single-cause, 3 two-cause).
- Guards: refusal while a matrix game runs, 30 s cap per injection, disk cap and free-space check, cleanup on Ctrl+C.
- Gate fix: `scripts/gate.ps1` runs `fa lab score` as soon as `crates/fa/src/lab.rs` exists, but the scorer arrives in
  lot 10. The trigger moves to `crates/fa/src/lab/score.rs`.
- Lot 5 PresentMon parity replayed on the Lab loop; a demo video recorded by the user.

## Capabilities

### New Capabilities
- `stutter-lab`: own render loop, injectors, JSONL ground truth, TOML scenarios, guards.

### Modified Capabilities

None. `health-check`, `driverless-probes` and the lot 5 capture are consumed, not changed.

## Impact

- **Compliance.** `compliance.toml` is not changed: the `fa-lab` entry already exists with every item `allowed`
  (ownership). The "never touch the game" boundary is unchanged: injectors act on the Lab's own threads, memory and
  files only; no handle on any other process. The Lab refuses to start while a matrix game runs, detected by Toolhelp
  image names and window class (`fa_probe::plan::running_games`), with no process handle. The `timer-resolution`
  injector only affects the Lab itself (timer resolution is per-process since Windows 10 2004).
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): **WIN-05** D3D11 loop (`fa-win`, COM in unsafe; French
  explanation checkpoint); **ASM-07** `clflushopt` sweeps (`fa-asm`, to earn its place, see D-6); **ASM-06** reused as
  `fa_asm::fma::burn` (shipped as intrinsics in lot 4 after a tie, nothing to earn); ASM-04 non-temporal kernels reused
  if they fit `mem-bandwidth`. Proposed new `fa-win` line **WIN-12** (Lab system calls: commit/decommit, working-set
  trim, unbuffered file open, `timeBeginPeriod`, own-thread priority), added to the catalog before its code.
- **Code:** `fa-win`, `fa-asm`, `fa` (`lab/`), `scripts/gate.ps1`, `lab/scenarios/`.
- **Dependencies:** `windows` features for D3D11, DXGI, windowing, file and memory APIs; `toml`, `serde`.
- **Storage:** journals and temporary files in `%LOCALAPPDATA%\FrameAutopsy\lab\`.
