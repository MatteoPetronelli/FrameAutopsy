

## ADDED Requirements

### Requirement: Matrix file
`compliance.toml` SHALL list, per game: id, display name, detection rule (process image names; window-title prefix for
Minecraft), anti-cheat, and a verdict for every observer and action (`allowed`, `gray`, `forbidden`, `unverified`),
each verdict with source URLs, consultation date and anti-cheat test record when one exists. Game ids: `lol`,
`valorant`, `fortnite`, `overwatch2`, `minecraft-java`, `fa-lab`, plus the implicit `unknown`. The file is embedded at
build time; a user override MAY only disable items, never enable them.

#### Scenario: Override tries to enable an action
- **WHEN** a user override sets `governor.ecoqos = "allowed"` for `valorant` while the embedded verdict is `unverified`
- **THEN** the override is rejected with an error naming the item

### Requirement: Enablement semantics
Observers in the baseline observe-only set (present events, kernel scheduling/DPC/page-fault/disk events, in-game
probes, system counters) SHALL be enabled for every game unless the verdict is `forbidden`. Every other observer, every
action and the overlay SHALL be enabled only when the verdict is `allowed`. (Claude's call, reconciling handoff §2
"observe-only on any game" with §4.2 "to verify"; the anti-cheat smoke test at the end of lot 7 confirms the baseline
per game.)

#### Scenario: Unverified action
- **WHEN** an EcoQoS proposal targets a background process during a VALORANT match and `valorant.governor.ecoqos` is `unverified`
- **THEN** nothing is applied and the UI shows "en attente de vérification de conformité"

#### Scenario: Unknown game
- **WHEN** an unlisted game is detected through its presents
- **THEN** only the baseline observers run; no action and no overlay are available

### Requirement: Compliance tests
A test SHALL fail if any action is enabled for a game without a matrix entry, or if an entry marked `allowed` lacks a
source URL and a date.

#### Scenario: Matrix entry check
- **WHEN** an action is enabled for a game without a matrix entry, or an entry marked `allowed` lacks a source URL and a date
- **THEN** the test fails

### Requirement: TargetProcess guard
The governor SHALL act only through `TargetProcess`, constructible only by `TargetProcess::try_new(pid, image_name,
creation_time, &Matrix)`. Construction SHALL fail for matrix games, anti-cheat processes listed in the matrix (for
example `vgc.exe`, `vgtray.exe`, `EasyAntiCheat.exe`, `EasyAntiCheat_EOS.exe`, `BEService.exe`), critical processes
(`System`, `smss.exe`, `csrss.exe`, `wininit.exe`, `winlogon.exe`, `services.exe`, `lsass.exe`, `dwm.exe`,
`audiodg.exe`) and FrameAutopsy's own processes.

#### Scenario: Governor aims at the game
- **WHEN** code tries to build a `TargetProcess` for `VALORANT-Win64-Shipping.exe`
- **THEN** it gets an error and no handle is opened

### Requirement: No handle on game processes
FrameAutopsy SHALL never call `OpenProcess` (or open any handle) on a matrix game's process. Games are detected by
their full image name, from Microsoft-Windows-Kernel-Process ETW events (collector) or a Toolhelp snapshot (CLI), and
Minecraft by window title through `EnumWindows`/`GetWindowThreadProcessId`. Classic kernel process events truncate the
image name to 15 bytes and SHALL NOT be used for identification. `TargetProcess` construction SHALL fail closed when
only a truncated or unknown name is available. A source test SHALL fail if `OpenProcess` is called anywhere except
`fa-win::process::open_target(&TargetProcess)`.

#### Scenario: Truncated name
- **WHEN** the only name known for a PID is the 15-byte kernel name `VALORANT-Win64-`
- **THEN** `TargetProcess::try_new` refuses it

### Requirement: No game data during play
No game-state data SHALL be displayed or signalled while a match is in progress, in any game. Only system metrics MAY
be shown live. Game-event correlation appears only in post-match reports.

#### Scenario: Match in progress
- **WHEN** a match is in progress
- **THEN** no game-state data is displayed or signalled, only system metrics MAY be shown live, and game-event correlation appears only in post-match reports


