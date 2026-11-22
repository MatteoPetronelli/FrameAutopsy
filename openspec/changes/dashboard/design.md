# Design

## Context

Lot 7 provides the collector, the read-only shared memory (seqlock, SYNC-02) and the command pipe. Lot 8 provides
`store.db` (SQLite WAL, MessagePack + zstd blobs) in `%ProgramData%\FrameAutopsy` (admin write, users read). Budgets
(`docs/architecture.md` section 6): first frame under 300 ms, working set under 60 MB (calibrate). Target machine:
i5-11400H, RTX 3060 Laptop + Intel UHD (Optimus), Windows 11.

## Goals / Non-Goals

**Goals:**
- Fast first frame; a real session timeline of 100k+ points that stays smooth.
- Read-only everywhere; actions only through the pipe.
- French text, keyboard navigation, no colour-only meaning.

**Non-Goals:**
- No overlay, no tray icon, no notifications (no tray in v1, Claude's call).
- No action UI beyond what the lot 7 commands allow; proposals and experiments views are filled by lots 12 and 13.
- No symbol resolution view (lot 11).

## Decisions

**D-1 Order of work.** Spike (D-2), then only the timeline of a real lot 8 session, then measure first-frame time and
working set, then add other views. If the budget is missed, fix it before adding views.

**D-2 Read-only WAL spike (UNVERIFIED).** SQLite can open a WAL database read-only as a non-writer only when the `-wal`
and `-shm` files already exist (SQLite 3.22 or later), and the reader needs read access to the `-shm`. Group 1 opens
`store.db` read-only as a standard user (a) while the elevated collector writes and (b) with no writer running (the
`-wal` and `-shm` may be absent or stale). Outcomes: works in both, then the UI uses a read-only connection. Fails in
(b): the UI shows "collecteur arrete" and falls back to the last checkpointed state if readable, decided with the user.
Fails in (a): fallback decided with the user before any view is built; candidates are the collector exposing the data
through the read-only shared memory, or writing a read-only snapshot file. A fallback changes lot 7 or 8 specs, so it
needs the user's go. The UI never writes the store, never runs a checkpoint, never opens a read-write connection.

**D-3 Data access layer.** `fa-ui` reads through `fa-storage` read-only functions; loading runs on a worker thread so
the first frame never waits for SQLite. Views show a "chargement" state until data arrives.

**D-4 Live view.** Maps the lot 7 region with `FILE_MAP_READ` through the lot 7 `fa-win` wrapper and reads slots with
the SYNC-02 seqlock reader (no references into the region, retry on torn read). The UI never writes it. Collector not
running means "collecteur arrete", not an error.

**D-5 Actions.** The UI sends only the lot 7 commands (collector-issued ids, 64 KiB cap). In `confirm` mode proposals
are queued and reviewed after the match; no window, sound or notification takes focus. The window never requests
foreground and is created without stealing focus when a game is in the foreground.

**D-6 French numbers.** `fr_num(v, decimals)` lives in `fa-probe::health`. `fa-ui` should not depend on `fa-probe`
(it drags wmi and NVML). Smallest option: move the one function to `fa-core` (pure, no unsafe) and re-export it from
`fa-probe::health` so existing callers and golden tests do not change. Verified by the existing health golden tests.

**D-7 Decimation.** For N points and W pixel columns, keep min and max per column (plus first and last), so spikes
survive. O(N) per zoom change, cached per (range, width); recomputed only on pan, zoom or resize. Unit tests: peak
preserved, output length at most 2W+2, monotonic x. Benchmark with 100k and 1M points; target stays under one frame at
60 Hz (calibrate).

**D-8 Renderer.** eframe offers glow (OpenGL) and wgpu. Choice by measurement on the target machine: first-frame time
and working set for each. Optimus risk: the window may wake the RTX 3060 and cost power and memory (unverified); measure
which adapter is used and prefer the integrated one if it meets the budget.

**D-9 Accessibility.** Full keyboard navigation (tab order, arrow keys in lists and timeline, Enter to open, Esc to go
back). Incident markers use shape and label, not only colour; severities carry text. Contrast checked by eye on both
themes; egui's AccessKit support is used as available.

**D-10 Export.** Markdown and JSON, written to a user-chosen path, atomic write (temporary file then rename). JSON uses
plain numbers; Markdown uses `fr_num`. Exports contain recorded data only.

**D-11 Unsafe.** `fa-ui` has `#![forbid(unsafe_code)]`. This lot adds no unsafe. Any FFI would go in `fa-win` with a
catalog line first and an explanation checkpoint. None is planned.

## Risks / Trade-offs

- [WAL read-only fails for a standard user] -> spike first, fallback agreed with the user (D-2).
- [First frame over 300 ms or 60 MB] -> measured before extra views; renderer choice (D-8); lazy view construction.
- [Optimus wakes the dGPU] -> unverified; measure adapter and power; prefer integrated GPU.
- [100k+ points janky] -> decimation, cache, benchmark (D-7).
- [Store and collector schema drift] -> the UI checks the schema version and shows "version incompatible".
- [Live view torn reads] -> seqlock reader with retry, checksum test inherited from lot 7.
- [UI open in game is distracting or heavy] -> repaint only on change, live view at 10 Hz at most; nothing required.
