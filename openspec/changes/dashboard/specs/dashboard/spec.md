## ADDED Requirements

### Requirement: Fast and light
`fa-ui` (egui/eframe, non-elevated) SHALL show its first frame in under 300 ms and stay under 60 MB working set
(calibrate).

#### Scenario: Startup budget
- **WHEN** `fa-ui` starts on the target machine with a populated store
- **THEN** the first frame appears in under 300 ms and the working set stays under 60 MB

### Requirement: Views
The UI SHALL offer: sessions list (game, date, duration, FPS, lows, incidents); session timeline (frametime plot with
incident markers and probe overlays, min/max decimation per pixel for 100k+ points); incident detail (window, causes,
attribution when available); health check report; live view from shared memory when the collector runs; proposals and
experiments (from lots 12 and 13).

#### Scenario: Large timeline
- **WHEN** a session of 100000 or more frames is opened
- **THEN** the timeline is drawn with min/max decimation per pixel column and stays smooth while panning and zooming

#### Scenario: Collector not running
- **WHEN** the collector is not running
- **THEN** the live view says so in French and the other views still work from the store

### Requirement: Read-only data access
The UI SHALL read SQLite read-only and the shared memory read-only (`FILE_MAP_READ`). Every action SHALL go through the
command channel.

#### Scenario: UI never writes
- **WHEN** the UI is used for any view or export
- **THEN** it opens no read-write database connection, never writes the shared memory and sends actions only as command channel messages

### Requirement: Not required during play
`fa-ui` MAY run during play (live view on a second screen, overlay) but nothing SHALL require it. Proposals in
`confirm` mode SHALL be queued and reviewed after the match; no window or notification SHALL take focus during a match.

#### Scenario: Match in progress
- **WHEN** a proposal in `confirm` mode is created during a match
- **THEN** it is queued for review after the match and no window or notification takes focus

### Requirement: Export
Sessions, incidents and experiments SHALL be exportable as Markdown or JSON to a user-chosen path.

#### Scenario: Export a session
- **WHEN** the user exports a session as Markdown or JSON to a chosen path
- **THEN** the file is written there and contains the recorded session data

### Requirement: Language and accessibility
UI text SHALL be French (Claude's call), with decimal commas. Navigation SHALL work with the keyboard, and color SHALL
never be the only carrier of meaning.

#### Scenario: Keyboard only
- **WHEN** the user navigates every view with the keyboard alone
- **THEN** every control is reachable and incident markers are distinguishable without color
