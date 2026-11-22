# etw-capture Specification

## Purpose
A real-time ETW session that captures present events, kernel scheduling, DPC and ISR, disk, image and process events
with raw TSC timestamps, decodes them without allocation into fixed-size records, extracts per-swap-chain frametimes
(parity with PresentMon) and identifies processes by full image name without ever opening a handle.

## Requirements

### Requirement: Real-time session
`fa-win` SHALL drive the session through `StartTraceW`, `EnableTraceEx2`, `OpenTraceW` and `ProcessTrace`, under the
session name `FrameAutopsy`. A stale session left by a crash SHALL be stopped at startup.

#### Scenario: Stale session
- **WHEN** a session named `FrameAutopsy` is left running by a crashed process and FrameAutopsy starts
- **THEN** the stale session is stopped and a new one starts

### Requirement: Timestamps
The session SHALL use `ClientContext = 3` (CPU cycle counter) when the TSC is invariant, and the consumer SHALL open
the trace with `PROCESS_TRACE_MODE_RAW_TIMESTAMP` so timestamps stay raw TSC values instead of being converted. Without
an invariant TSC, QPC is used and converted through `tsc-clock`.

#### Scenario: Invariant TSC
- **WHEN** the TSC is invariant and a trace is opened
- **THEN** event timestamps are raw TSC ticks comparable with `tsc-clock` reads

#### Scenario: No invariant TSC
- **WHEN** the TSC is not invariant
- **THEN** QPC timestamps are used, converted through `tsc-clock`, and the degraded mode is reported

### Requirement: Providers
The session SHALL enable present events (Microsoft-Windows-DxgKrnl, Microsoft-Windows-DXGI and what PresentMon's
algorithm needs), kernel context switches, DPC, ISR, hard page faults, disk I/O, process/thread start-stop, image
loads, and Microsoft-Windows-Kernel-Process for full image names. Exact keywords and flags follow PresentMon's
open-source implementation (MIT), used as the reference.

#### Scenario: Providers enabled
- **WHEN** the session starts
- **THEN** every listed provider is enabled with the keywords and flags taken from PresentMon's implementation

### Requirement: 24H2 kernel address spike
The change SHALL start with a spike on the target build (Windows 11 26H2, later than 24H2): does an elevated session
without `SeDebugPrivilege` receive non-zero kernel addresses in DPC/ISR routine fields, stack walks and image-load
rundown events? The outcome is recorded in `docs/decisions.md` and decides what `cause-engine` and
`driver-attribution` can do. FrameAutopsy SHALL NOT enable `SeDebugPrivilege`.

#### Scenario: Spike verdict
- **WHEN** the spike has run in an elevated session without `SeDebugPrivilege`
- **THEN** the counts of zero and non-zero kernel addresses and the resulting verdict are recorded in `docs/decisions.md` before later groups start

### Requirement: Module map
Kernel image loads (rundown at session start + live loads) SHALL give base, size and path, and every kernel address
SHALL resolve to module + offset when the spike shows addresses are available.

#### Scenario: Address resolution
- **WHEN** a kernel address falls inside a loaded module and addresses are available
- **THEN** it resolves to that module and an offset

#### Scenario: Addresses unavailable
- **WHEN** the spike shows kernel addresses are zeroed
- **THEN** the map keeps module names and resolves no address

### Requirement: Zero-copy decoding
The callback SHALL hand `fa-capture` a borrowed view (header fields + `&[u8]` user data valid only during the callback).
`fa-capture` SHALL decode hot events with hand-written layouts keyed by provider, event id and version into fixed-size
`Copy` records. TDH SHALL be used only by a developer tool that dumps layouts. Unknown versions are counted and skipped.

#### Scenario: Unknown version
- **WHEN** an event arrives with a version that has no layout
- **THEN** it is skipped, counted, and no record is produced

### Requirement: Zero allocation on the hot path
Decoding and pushing to the transport SHALL perform zero heap allocations.

#### Scenario: One million events
- **WHEN** one million synthetic `EVENT_RECORD`s go through the callback under the counting allocator
- **THEN** the allocation count is 0

### Requirement: Earns its place against ferrisetw
A comparison on the same synthetic stream SHALL show 0 allocations per event and throughput at least equal to
`ferrisetw`; otherwise the crate is adopted for the non-hot providers.

#### Scenario: Comparison on the same stream
- **WHEN** the hand-written consumer and `ferrisetw` process the same synthetic stream
- **THEN** the hand-written one stays for hot providers only if it shows 0 allocations per event and at least equal throughput, otherwise `ferrisetw` is adopted for non-hot providers

### Requirement: Loss accounting
Lost ETW buffers/events and transport drops SHALL be counted and shown.

#### Scenario: Losses shown
- **WHEN** ETW reports lost buffers or events, or the transport drops records
- **THEN** each count is incremented and shown by `fa capture`

### Requirement: Frametime extraction and parity
Present events SHALL be turned into frame records (present start, displayed time, frametime, present mode) per process
and swap chain, following PresentMon's algorithm.

#### Scenario: Parity with PresentMon
- **WHEN** a render loop runs while FrameAutopsy and PresentMon capture in parallel (a non-game DXGI application in lot 5, the Lab render loop from lot 6)
- **THEN** at least 99 % of frames match within ±0.1 ms of frametime (calibrate)

### Requirement: Identification without handles
Game processes SHALL be identified by PID and full image name from Microsoft-Windows-Kernel-Process events only.

#### Scenario: Truncated kernel name
- **WHEN** a classic kernel event carries an image name truncated to 15 bytes
- **THEN** it is not used as a process name and no process handle is opened
