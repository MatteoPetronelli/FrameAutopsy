# Design

## Context

Lot 5 delivers decoded events with raw TSC timestamps on an SPSC ring; lot 7 hosts the collector. The analysis thread
updates the window and the trigger (hot path, allocation-free); a worker compresses and stores (may allocate). See
`docs/architecture.md` sections 4, 6, 7 and `proposal.md`. Target: i5-11400H, 6 cores / 12 threads, one LLC.

## Goals / Non-Goals

**Goals:**
- Every stutter yields one complete, bounded snapshot of -10 s to +2 s.
- Disk use bounded (30 days AND 2 GB), proven by a test.
- Probe sampling that never perturbs the game and never reads biased values.

**Non-Goals:**
- Diagnosis rules and attribution (lot 10); keep/remove of the frequency probe (lot 10).
- UI reading the store (lot 9); only the schema is kept friendly to it.
- Calibration of the trigger on Lab scenarios (lot 10).

## Decisions

**D-1 Window vs snapshot.** The window holds the last 10 s but a snapshot spans -10 s to +2 s. Choice: **copy the
pre-trigger 10 s at trigger time into a slot of a pre-sized pool, then append the next 2 s into the same slot**; the ring
stays 10 s. Why not a ring of 12 s: the snapshot would have to be taken exactly 2 s later, and a late analysis thread or
a second trigger would find the oldest frames overwritten. A copy of about 1500 frames plus aggregated series is some tens
of KB (microseconds). The pool (4 slots, calibrate) is allocated at start; a closed slot goes to the worker through a
channel and returns empty. Pool exhausted: the incident is dropped and counted, never allocated. Test: counting allocator
(`fa_sync::counting::CountingAlloc`, `assert_no_alloc`, feature `test-support`) around push, trigger, slot copy and append.

**D-2 Window ring (`fa-core`, forbid unsafe).** Const-generic ring buffers for frames, aggregated event series and
probe samples, per tracked game, pre-sized, no arena. Events are aggregated per series (counts, sums, maxima per time
bucket), not stored raw. Nothing in it is ever written to disk except through a snapshot.

**D-3 Trigger.** Fire when frametime > 2 x rolling median of the last second AND > median + 4 ms (both calibrate);
incidents closer than 1 s coalesce. The threshold is max(2 x m, m + 4 ms); 2 x m wins whenever m > 4 ms, i.e. below
250 FPS: at 144 FPS (6.9 ms) the threshold is 13.9 ms, at 30 FPS it is 66.7 ms. The +4 ms term only binds above 250 FPS
and is nearly inert on target games; kept as a guard. Calibration on Lab scenarios happens in lot 10; this lot only
fixes the mechanism. The rolling median uses a pre-sized structure, no allocation.

**D-4 Timestamp codec: measure first (SIMD-01).** Task 2 runs before any codec: on real snapshots from lot 5 captures,
compare (a) zstd alone, (b) delta + zstd, (c) delta-of-delta + zigzag + scalar varint + zstd. The codec runs on a worker
once per incident over a few thousand timestamps, so throughput barely matters and **compressed size decides**: a codec
stage ships only if it beats "delta + zstd" by at least 5 % on stored bytes; a throughput-only win does not count. Likely
outcome, said plainly: Stream VByte is never written (4 of 5 earn-its-place candidates tied so far). If the
delta-of-delta + varint stage wins, it ships as safe scalar Rust. Stream VByte (scalar, then SSSE3 `pshufb` intrinsics,
asm only if intrinsics lose) follows only if the scalar varint stage is a measured bottleneck, with reference, parity
test (round trip on random and real timestamps), recorded bench and `FA_FORCE_SCALAR=1` fallback. A win under 5 % is a
tie; the reference ships and the module is deleted, numbers stay in the catalog.

**D-5 `fa-storage` (no unsafe).** `rusqlite` (bundled SQLite), WAL mode, `rmp-serde`, `zstd`. Blobs are MessagePack +
zstd; tables `sessions`, `incidents`, `probes`, `health_checks`, `experiments` with plain indexed columns (session,
time, size) next to the blob so a reader can list without decoding. File `%ProgramData%\FrameAutopsy\store.db`, admin
write, users read. Written only by the worker. Health checks read from user-writable files go through
`fa_probe::store::read_baseline` rules (size cap, strict schema).

**D-6 Retention.** After each insert (and hourly), delete incidents older than 30 days, then oldest first until the file
(db + WAL) is under 2 GB; both configurable, defaults from `docs/decisions.md` section 2. Deleting rows does not shrink
the file: use `auto_vacuum=INCREMENTAL` plus `incremental_vacuum`, and measure size by `page_count x page_size` plus the
WAL. Test: fill with synthetic incidents beyond 2 GB (small cap in the unit test, one full-size run in the final group)
and assert the oldest are gone and the size is under the cap. No continuous raw trace exists on disk.

**D-7 Session summary.** A streaming accumulator per 10 minutes: average FPS, 1 % low, 0.1 % low (mean of the worst
1 % / 0.1 % frames, definition fixed in the test), P99 frametime, stutter count. The frametime histogram has fixed bins
(pre-sized); percentiles are bin-accurate, error stated.

**D-8 In-game probe sampling (O4).** The collector picks probe CPUs with a pure function (`fa-core`, unit-tested on
synthetic topologies) of the topology and per-CPU busy time of the game's threads derived from ETW context-switch events
(lot 5): no process handle, ever. A CPU is eligible only if it hosts no hot game thread AND its SMT sibling is not busy
(any process): measured on the target, the frequency probe reads about 9 % low with a busy sibling (3.71 vs 4.08 GHz PDH;
free sibling 0.17 % error, `docs/decisions.md` section 5). Arithmetic: the frequency chain lasts about 50 us and 0.01 %
of 1 s is 100 us, so at most about 2 samples per second per CPU (calibrate). Samples go in the window and in snapshots.
Existing `fa-probe` probes are reused; lot 10 decides keep/remove of the frequency probe. If no CPU qualifies, no sample
is taken. Probes stop when O4 is not allowed for the game.

**D-9 Dependencies.** `rusqlite` (bundled), `rmp-serde`, `zstd`, `serde`; no unsafe in our code. `fa-core` and
`fa-storage` keep `#![forbid(unsafe_code)]`.

## Risks / Trade-offs

- [Non-elevated UI must read a WAL db read-only] SQLite (3.22 and later) opens WAL read-only only when `-wal` and `-shm`
  exist: unverified. Lot 9 spikes it; the schema keeps list columns separate from blobs, and the collector can instead
  publish read snapshots.
- [Stream VByte never earns its place] expected; the measurement and its numbers are the deliverable.
- [Pool exhausted during a stutter storm] drop and count; reported in status.
- [Probe duty cycle perturbs the game] at most 0.01 % per CPU, non-game CPUs only; calibrate on Lab and anti-cheat test
  (user only).
- [Hot-thread inference from context switches is noisy] fails closed: no sample when unsure.
- [Trigger thresholds are guesses] calibrate in lot 10.
- [2 GB cap vs WAL growth] measure db + WAL size; checkpoint after retention.
