# stutter-lab Specification

## Purpose
A dev-only Lab that renders its own D3D11 window at a fixed frame rate and injects nine known causes of stutter (frame stall, CPU, cache, TLB, memory bandwidth, page faults, disk, timer resolution, priority contention) on itself, with a JSONL ground-truth journal. It gives later lots a source of truth to score detection and attribution against, without ever touching a game.

## Requirements

### Requirement: Own render loop
`fa lab` SHALL open a window with a D3D11 device and a flip-model swap chain (`DXGI_SWAP_EFFECT_FLIP_DISCARD`) and
render a trivial animated scene at 60, 144 or uncapped FPS. The Lab is the `fa-lab` entry of the matrix, where
everything is `allowed`.

#### Scenario: Loop at 144 FPS
- **WHEN** `fa lab run` starts a scenario at 144 FPS
- **THEN** a flip-model window renders the animated scene and the loop's baseline jitter is recorded

### Requirement: Injectors
The Lab SHALL provide parameterized injectors (start, duration, intensity, target CPUs), each logging ground truth.
They act on the Lab's own threads, memory and files only.

#### Scenario: CPU and frame injectors
- **WHEN** `cpu-burn` or `frame-stall` is injected
- **THEN** `cpu-burn` busy-loops on the chosen CPUs until a TSC deadline, optionally with the FMA load (ASM-06), and `frame-stall` burns N ms before `Present` at frame k

#### Scenario: Memory injectors
- **WHEN** `cache-thrash`, `tlb-thrash` or `mem-bandwidth` is injected
- **THEN** `cache-thrash` sweeps an LLC-exceeding buffer with large strides and cache-line flushes so that a latency jump is measurable, `tlb-thrash` touches one byte per 4 KiB page over a large region, and `mem-bandwidth` writes with non-temporal stores on N threads

#### Scenario: System injectors
- **WHEN** `page-fault-storm`, `disk-burst`, `timer-resolution` or `priority-contention` is injected
- **THEN** the Lab commits, touches and decommits memory and trims its own working set, writes unbuffered write-through to a temporary file deleted afterwards, toggles `timeBeginPeriod` (per-process since Windows 10 2004, so it changes the Lab's own pacing), or runs a high-priority Lab worker on the render thread's CPU

### Requirement: Ground truth and scenarios
Each injection SHALL append (cause id, start/end TSC, parameters) to a JSONL log. Scenarios SHALL be TOML files in
`lab/scenarios/` listing injections and expected causes.

#### Scenario: Journal line
- **WHEN** an injection starts and ends
- **THEN** the JSONL log holds its cause id, start and end TSC ticks and parameters

#### Scenario: Scenario run
- **WHEN** `fa lab run <scenario>` is called
- **THEN** the Lab runs the scenario with capture, which needs admin rights, so it is a developer command run from an elevated terminal, and scoring arrives with `cause-engine`

### Requirement: Guards
The Lab SHALL refuse to start while a matrix game runs; cap each injection at 30 s; cap disk bursts at 1 GB and require
10 GB free; stop every injector and delete temporary files on Ctrl+C.

#### Scenario: Game running
- **WHEN** `Overwatch.exe` is running and `fa lab run` is called
- **THEN** the Lab refuses and names the process

#### Scenario: Ctrl+C
- **WHEN** the user presses Ctrl+C during a disk burst
- **THEN** every injector stops and the temporary file is deleted

#### Scenario: Over the caps
- **WHEN** a scenario asks for an injection over 30 s, a disk burst over 1 GB, or a disk burst with under 10 GB free
- **THEN** the Lab refuses to run it
