## ADDED Requirements

### Requirement: In-memory window
`fa-core` SHALL keep the last 10 s of frames, events (aggregated per series) and probe samples per tracked game in a bounded, pre-sized ring (const generics). No raw continuous kernel trace SHALL be persisted. The hot path SHALL NOT allocate.

#### Scenario: Window bounded
- **WHEN** a game runs for 30 minutes
- **THEN** the ring holds only the last 10 s, the hot path performs zero allocations under the counting allocator and no raw trace file exists on disk

### Requirement: Stutter trigger
A frame SHALL trigger an incident when its frametime exceeds 2x the rolling median of the last second AND the median + 4 ms (calibrate). Incidents closer than 1 s SHALL coalesce.

#### Scenario: Spike at 144 FPS
- **WHEN** the median is 6.9 ms and a frame takes 20 ms
- **THEN** one incident fires, and a second spike 0.5 s later joins it

#### Scenario: Small deviation
- **WHEN** the median is 6.9 ms and a frame takes 12 ms
- **THEN** no incident fires

### Requirement: Snapshot content
An incident SHALL hold the window from -10 s to +2 s around the trigger: frames, event series, probe samples, a process table (names and PIDs, no handles) and system state (power source, probe clock, NVML when present). The pre-trigger 10 s SHALL be copied at trigger time into a pre-sized slot and the next 2 s appended, without hot-path allocation.

#### Scenario: Snapshot span
- **WHEN** a trigger fires at time T
- **THEN** the stored snapshot covers T-10 s to T+2 s, even if the ring has wrapped since, and the copy allocates nothing

#### Scenario: Slot pool exhausted
- **WHEN** all snapshot slots are in use and a new incident fires
- **THEN** the incident is dropped and counted, and nothing is allocated

### Requirement: Timestamp codec
The codec stage (delta-of-delta + zigzag + varint or Stream VByte, then zstd) SHALL be measured against zstd alone and delta + zstd on real snapshots before any codec code is written, and SHALL ship only if it saves at least 5 % of stored bytes over delta + zstd. A throughput-only win SHALL NOT justify it.

#### Scenario: Codec measured first
- **WHEN** lot 8 starts
- **THEN** a recorded comparison on real snapshots exists before any codec is written, and a stage that does not beat delta + zstd by 5 % is not shipped

### Requirement: Storage and retention
Records SHALL be stored as MessagePack + zstd blobs in SQLite (WAL) at `%ProgramData%\FrameAutopsy\store.db` with tables `sessions`, `incidents`, `probes`, `health_checks`, `experiments`. Retention SHALL be 30 rolling days AND 2 GB (oldest first), configurable.

#### Scenario: Cap reached
- **WHEN** the store exceeds 2 GB
- **THEN** the oldest incidents are deleted until it is under the cap

#### Scenario: Age limit
- **WHEN** an incident is older than 30 days
- **THEN** it is deleted even if the store is under 2 GB

### Requirement: Session summary
Each game session SHALL record average FPS, 1 % low, 0.1 % low, P99 frametime and stutter count per 10 minutes.

#### Scenario: Summary fixture
- **WHEN** a 10-minute fixture with known frametimes ends
- **THEN** the recorded values match the known average FPS, 1 % low, 0.1 % low, P99 and stutter count

### Requirement: In-game probe sampling
During a game, the collector SHALL run probes only on logical CPUs hosting none of the game's hot threads (inferred from ETW context switches, never a process handle) and whose SMT sibling is not busy, with an average duty cycle of at most 0.01 % per CPU (calibrate), and store the samples in the window and in snapshots.

#### Scenario: Busy SMT sibling
- **WHEN** the SMT sibling of a candidate CPU is busy
- **THEN** that CPU is not probed, because the frequency probe reads about 9 % low there

#### Scenario: Duty cycle
- **WHEN** the frequency probe runs on an eligible CPU
- **THEN** it takes at most about 2 samples per second (50 us each), staying within 0.01 % duty
