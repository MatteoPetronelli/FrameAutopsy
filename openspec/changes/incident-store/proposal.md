# Proposal

## Why

The collector (lots 5 to 7) sees every frame and every kernel event, but keeps nothing: when a stutter happens, the
evidence is gone a few seconds later. Lot 8 keeps the recent past in a bounded in-memory window, freezes the 12 s around
each stutter, compresses it and stores it under a hard disk cap. Without it there is nothing for diagnosis (lot 10), the
dashboard (lot 9) or the A/B proof (lot 13) to read. It also adds the in-game probe samples (O4) that explain
throttling-type stutters.

Capability purpose (from `specifications.md`): keep the recent past in memory, freeze it around each stutter, compress it
and store it with bounded disk usage.

## What Changes

- Window ring, stutter trigger and snapshot assembly in `fa-core` (safe, allocation-free on the hot path, proven with
  the counting allocator).
- New crate `fa-storage` (no unsafe): SQLite WAL (`rusqlite`, bundled SQLite), MessagePack, zstd; tables `sessions`,
  `incidents`, `probes`, `health_checks`, `experiments`; retention of 30 rolling days AND 2 GB, oldest first.
- Session summary per 10 minutes: average FPS, 1 % low, 0.1 % low, P99 frametime, stutter count.
- In-game probe sampling: reuse the lot 3 probes (`fa-probe`) on CPUs without game hot threads, never on a core whose
  SMT sibling is busy, duty cycle at most 0.01 % per CPU.
- Timestamp codec measured first (zstd alone vs delta + zstd on real snapshots). Stream VByte (SIMD-01) is written only
  if the measurement justifies it; the likely outcome is that it is never written.

## Capabilities

### New Capabilities
- `incident-store`: window, trigger, snapshot, timestamp codec decision, storage and retention, session summary, in-game
  probe sampling.

### Modified Capabilities

None. `driverless-probes` is reused, not changed; its keep/remove decision for the frequency probe stays in lot 10.

## Impact

- **Compliance.** `compliance.toml` is not changed. In-game probes are matrix item O4 (baseline observer, own threads,
  non-game CPUs, at most 0.01 % duty), already declared; it stays off for any game where it is `forbidden`. The "never
  touch the game" boundary is unchanged: hot threads of the game are inferred only from ETW context-switch events
  already captured by lot 5, never through a handle on the game process, and no game data is read. The process table in
  a snapshot holds names and PIDs from ETW/Toolhelp only. No raw continuous trace is written; nothing leaves the
  machine (no telemetry).
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): **SIMD-01** Stream VByte (intrinsics + asm-candidate), status
  `pending`, conditional on the task 2 measurement. No new `fa-win` or `fa-sync` line. Probes reused: ASM-02 (frequency),
  ASM-03 (latency) from `fa-probe`; no new probe.
- **Code:** `fa-core` (window ring, trigger, snapshot, CPU choice), new `fa-storage`, `fa-probe` (sampling schedule),
  `fa-asm` (SIMD-01, only if it earns its place).
- **Dependencies:** `rusqlite` (feature `bundled`, ships its own SQLite), `rmp-serde` (MessagePack), `zstd`, `serde`.
- **Storage:** `%ProgramData%\FrameAutopsy\store.db` (admin write, users read), bounded to 30 days and 2 GB.
- **Docs:** bench result and verdict in the catalog; calibrated values in `docs/decisions.md` section 5.
