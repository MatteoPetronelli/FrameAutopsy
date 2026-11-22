# Proposal

## Why

Lots 5 to 8 produce sessions, incidents and health checks that only exist as rows in a store and as CLI text. Decision D7
chose egui for an interface that starts instantly and stays light. This lot gives the user a place to read what the
collector recorded, outside games, and a read-only live view. It is also where the "the UI only reads" rule becomes
testable: the UI never writes the store or the shared memory, and every action goes through the lot 7 command channel.

Capability purpose (from `specifications.md`): a fast egui interface, opened outside games, to read sessions, incidents,
health checks, proposals and experiments.

## What Changes

- New binary `fa-ui` (eframe, egui_plot), non-elevated, `#![forbid(unsafe_code)]`, French text with decimal commas.
- Views: sessions list, decimated session timeline, incident detail, health check report, live view (shared memory,
  read-only), proposals and experiments (rendered when lots 12 and 13 exist; empty state before).
- Min/max decimation per pixel column so 100k+ points stay smooth.
- Export of sessions, incidents and experiments as Markdown or JSON to a user-chosen path.
- Keyboard navigation and non-colour cues (marker shapes, text labels).
- A spike first (group 1): can a non-elevated process open the collector's WAL `store.db` read-only? Unverified.
- The first vertical slice is the timeline of a real lot 8 session, with first-frame time measured before other views.

## Capabilities

### New Capabilities
- `dashboard`: the `fa-ui` binary, its views, read-only data access, behaviour during play, export, language and
  accessibility.

### Modified Capabilities

None. `incident-store` (lot 8) and `collector-ipc` (lot 7) are consumed, not changed, unless the spike forces a fallback
(see design D-2), which would be agreed with the user and recorded as a change to those specs.

## Impact

- **Compliance.** `compliance.toml` is not changed and no matrix row is added: the UI is not an observer. The "never
  touch the game" boundary is unchanged: `fa-ui` opens no handle on any process, shows no game data during a match and
  steals no focus (no notification, no modal, no always-on-top), has no overlay and no tray icon in v1. It is opened
  outside games; it may stay open during play, but nothing requires it. Game-event correlation stays in post-match views.
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): none. No `unsafe` and no asm in this lot. If the shared memory
  reader needs FFI beyond what lot 7 provides in `fa-win`, it goes in `fa-win` with a catalog line and a French
  explanation checkpoint first. SYNC-02 (seqlock reader, lot 7) is consumed only.
- **Code:** new crate `fa-ui`; `fr_num` moves from `fa-probe` to `fa-core` and is re-exported (design D-6).
- **Dependencies:** `eframe`, `egui_plot` (versions current at lot start, pinned in `Cargo.toml`), a SQLite binding
  through `fa-storage` (lot 8), `serde_json`.
- **Storage:** reads `%ProgramData%\FrameAutopsy\store.db`; writes only exports (user-chosen path) and UI preferences in
  `%LOCALAPPDATA%\FrameAutopsy\`.
