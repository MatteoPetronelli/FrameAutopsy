## ADDED Requirements

### Requirement: Elevated collector
`fa-collector` SHALL be a headless binary started at logon by a scheduled task "FrameAutopsy Collector" (highest
privileges, current user), installed and removed by `fa install` / `fa uninstall` (one UAC prompt each). A named mutex
SHALL keep it single-instance.

#### Scenario: Second instance
- **WHEN** `fa-collector` is already running and a second one is started
- **THEN** the second instance exits at once because the named mutex is held

#### Scenario: Install and uninstall
- **WHEN** `fa install` then `fa uninstall` run in an elevated session
- **THEN** the task "FrameAutopsy Collector" exists with highest privileges after install and is gone after uninstall

### Requirement: Install location
`fa install` SHALL copy the binaries to `%ProgramFiles%\FrameAutopsy` and create `%ProgramData%\FrameAutopsy` with an
ACL granting write access to SYSTEM and Administrators only (users read). The scheduled task SHALL point only at the
Program Files copy, never at a user-writable path such as `target\`.

#### Scenario: Replaced binary
- **WHEN** a non-elevated process tries to overwrite `fa-collector.exe` in the installed location
- **THEN** access is denied and the next logon still runs the original binary

### Requirement: Untrusted user-writable inputs
Files the collector reads from user-writable locations (overrides, health baselines in `%LOCALAPPDATA%`) SHALL be
validated like pipe messages: size cap, strict schema, overrides may only disable.

#### Scenario: Oversized or malformed baseline
- **WHEN** `health\baseline.json` is larger than its cap or does not match the strict schema
- **THEN** the collector rejects it, logs the rejection and does not use any of its content

#### Scenario: Override tries to enable
- **WHEN** an override file sets an item to a value other than `forbidden`
- **THEN** the override is rejected as a whole and the embedded verdicts stay in force

### Requirement: Shared memory
The collector SHALL create a named file mapping in the session namespace with a DACL granting full access to SYSTEM and
Administrators and read-only access to the interactive user. The layout SHALL start with a versioned header (magic,
version, size) followed by seqlock-protected slots (live frametime ring, latest probe values, status). The UI SHALL map
it with `FILE_MAP_READ` only.

#### Scenario: UI cannot write
- **WHEN** the UI maps the region
- **THEN** the view is read-only and the DACL read back lists exactly SYSTEM and Administrators full, interactive user read

#### Scenario: Wrong header
- **WHEN** a reader finds a wrong magic, version or size in the header
- **THEN** it refuses to read the slots

### Requirement: Cross-process seqlock
`fa-sync` SHALL implement the seqlock with atomics only: relaxed atomic 64-bit word access guarded by a sequence
counter, Release and Acquire fences, and readers retrying on an odd or changed sequence. Volatile access and references
into the shared region SHALL NOT be used.

#### Scenario: Write and read protocol
- **WHEN** the writer updates a slot
- **THEN** it makes the sequence odd, issues a Release fence, stores the payload as relaxed atomic 64-bit words, then makes the sequence even with Release
- **AND** the reader loads the sequence (Acquire), copies the payload with relaxed atomic loads, issues an Acquire fence, reloads the sequence and retries if it changed or was odd

#### Scenario: No torn snapshot
- **WHEN** the writer updates a slot in a tight loop and the reader reads concurrently for 10 s
- **THEN** every snapshot returned passes its checksum

#### Scenario: Miri
- **WHEN** `cargo +nightly miri test -p fa-sync` runs the two-thread seqlock test
- **THEN** Miri reports no data race and no undefined behavior

### Requirement: Command channel
UI to collector commands SHALL go through a named pipe whose DACL allows only the interactive user and Administrators.
Messages are length-prefixed MessagePack, at most 64 KiB, with a strict schema. Allowed commands: `GetStatus`,
`ApplyProposal{id}`, `RejectProposal{id}`, `SetAutonomy{rule_id, game_id, mode}`, `StartExperiment{rule_id, game_id}`,
`StopExperiment{id}`, `EmergencyStop`. Commands reference ids created by the collector, never raw PIDs, paths or
arguments.

#### Scenario: Forged proposal
- **WHEN** the UI sends `ApplyProposal` with an id the collector never issued
- **THEN** the command is rejected and logged, and nothing is applied

#### Scenario: Oversized message
- **WHEN** a client sends a 100 KiB message
- **THEN** the connection is closed without parsing it

#### Scenario: Unknown command or field
- **WHEN** a message names a command outside the list or carries an extra field such as a PID or a path
- **THEN** it is rejected and logged

### Requirement: Overhead budget
During a 30-minute game session the collector SHALL average under 1 % of one logical CPU and under 50 MB private
working set (calibrate), measured by itself and shown in the dashboard.

#### Scenario: Thirty-minute session
- **WHEN** the user plays 30 minutes with the collector running
- **THEN** the self-measured averages are under 1 % of one logical CPU and under 50 MB private working set, or the measured values are logged for recalibration

### Requirement: Anti-cheat smoke test
At the end of the lot, the user SHALL run the collector in observe-only mode during unranked matches of each matrix game
with anti-cheat active, and record the result in `docs/compliance.md`.

#### Scenario: User-run test
- **WHEN** the user has played at least 3 unranked matches of a game with the collector in observe-only mode
- **THEN** the result is recorded in `docs/compliance.md` section 6, and `compliance.toml` changes only on the user's word
