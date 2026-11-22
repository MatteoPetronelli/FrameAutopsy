# Tasks

Gate = `powershell -File scripts/gate.ps1` (headers, fmt, clippy -D warnings, test --workspace, graphify update). Each
group below ends with it, green. Dependency: lots 7 and 8 merged (real `store.db`, shared memory, pipe).

## 1. Spike: read-only WAL access

- [ ] 1.1 Open `%ProgramData%\FrameAutopsy\store.db` read-only as a standard user while the elevated collector writes; verify the read succeeds and record the result.
- [ ] 1.2 Repeat with no writer running (with and without `-wal`/`-shm` present); verify and record the result.
- [ ] 1.3 If either case fails, show the result to the user and decide the fallback (read-only shared memory or snapshot file) BEFORE any view; verify the decision is recorded in `docs/decisions.md` section 5.
- [ ] 1.4 Run the gate and verify it is green.

## 2. Crate, dependencies, shared helpers

- [ ] 2.1 Create `fa-ui` with `#![forbid(unsafe_code)]`, `eframe` and `egui_plot` (versions current at lot start); verify `cargo build -p fa-ui` succeeds and the crate contains no `unsafe`.
- [ ] 2.2 Move `fr_num` to `fa-core` and re-export from `fa-probe::health`; verify the existing health golden tests still pass.
- [ ] 2.3 Add a source test that `fa-ui` opens no process handle and no read-write SQLite connection; verify it fails on a deliberately added `OpenProcess` and read-write open (`mutation-verified-testing`).
- [ ] 2.4 Run the gate and verify it is green.

## 3. Timeline first, then measure

- [ ] 3.1 Implement min/max decimation with unit tests (peak preserved, length at most 2W+2, monotonic x); verify with a bench on 100k and 1M points.
- [ ] 3.2 Load one real lot 8 session on a worker thread and draw its timeline with incident markers; verify manually on a real session.
- [ ] 3.3 Measure first-frame time and working set for glow and wgpu, and the adapter used on the Optimus laptop; verify under 300 ms and 60 MB or record the gap, and log the renderer choice in `docs/decisions.md` section 5.
- [ ] 3.4 Run the gate and verify it is green.

## 4. Other views

- [ ] 4.1 Sessions list (game, date, duration, FPS, lows, incidents) with French text and `fr_num`; verify with a test on the rendered strings.
- [ ] 4.2 Incident detail (window, causes, attribution when available) and health check report; verify on real data.
- [ ] 4.3 Live view through the read-only shared memory and the SYNC-02 reader, with a "collecteur arrete" state; verify manually with the collector running and stopped, and that the mapping uses `FILE_MAP_READ`.
- [ ] 4.4 Proposals and experiments views with empty states; actions only via the command channel; verify a forged id sent by a test client is rejected by the collector and the UI writes nothing.
- [ ] 4.5 Run the gate and verify it is green.

## 5. Export, accessibility, play behaviour

- [ ] 5.1 Markdown and JSON export to a user-chosen path with atomic write; verify with golden files and an error on a read-only target path.
- [ ] 5.2 Keyboard navigation and non-colour cues; verify by using every view without the mouse and a test that markers differ by shape and label.
- [ ] 5.3 No focus steal; verify manually, with a window in the foreground, that launching or updating `fa-ui` does not take focus (any test with a real game is run by the user only).
- [ ] 5.4 Run the gate and verify it is green.

## 6. Done criteria

- [ ] 6.1 Verify first frame under 300 ms and working set under 60 MB on the laptop and a 100k-point timeline that stays smooth; paste the numbers in `docs/decisions.md` section 5.
- [ ] 6.2 Take screenshots for the README (sessions, timeline, incident, live view); verify they are in the repo and small.
- [ ] 6.3 Run `openspec validate dashboard --strict`; verify it passes.
- [ ] 6.4 Run the gate and verify it is green.

## Workflow follow-up

- Archive the change after the user validates the views and the branch is merged (`openspec-archive-change`).
