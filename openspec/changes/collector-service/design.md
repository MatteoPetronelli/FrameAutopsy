# Design

## Context

Lot 5 provides the ETW capture (`etw-capture`) and the hot-path ring; lots 0 to 4 provide the matrix, `TargetProcess`,
`read_baseline` (`crates/fa-probe/src/store.rs`) and `Matrix::with_override` (`crates/fa-core/src/matrix.rs`, 16 KiB
cap). This lot adds the process and privilege structure of `docs/architecture.md` sections 3, 6 and 7. Target machine:
Windows 11 build 26300, i5-11400H. See `proposal.md` for motivation and compliance effect.

## Goals / Non-Goals

**Goals:**
- A boundary a medium-integrity process cannot use to run code or act as admin.
- Torn-read-free live data from collector to UI, provable under Miri and across processes.
- Overhead under budget during real play, measured by the user.

**Non-Goals:**
- No action, governor, journal or watchdog. The command schema is fixed now, but only `GetStatus` and `EmergencyStop`
  have an effect; the other commands are schema-valid and answered "not available" (no id can exist yet), so the
  forged-id path is testable. (Claude's call.)
- No process handle of any kind: `open_target` does not exist (lot 12) and this lot does not need it. The watchdog
  (abandoned-mutex detection, `docs/decisions.md` section 2) belongs to lot 12, the governor lot, per the roadmap; this
  lot only creates the collector's named mutex.
- No large pages: rejected by the user (2026-11-22, catalog WIN-09); 4 KiB pages everywhere.
- No dashboard (lot 9), no store (lot 8): the UI side of the shared memory is a test reader here.

## Decisions

**D-1 Threat model first.** Group 1 runs the `threat-modeling` skill on the command channel and the trust boundary
(pipe, shared memory, install location, user-writable inputs), starting from the table in `docs/architecture.md`
section 3. The model is shown to the user and approved before any code; it is then recorded as a section of this file.

**D-2 Install (`fa install`, elevated).** Copy binaries to `%ProgramFiles%\FrameAutopsy` (inherits the admin-only write
ACL), create `%ProgramData%\FrameAutopsy` with an explicit ACL: SYSTEM and Administrators full control, Users read and
execute. Register the task "FrameAutopsy Collector" with `schtasks.exe /Create /XML`: trigger at logon, run level
highest, current user, action pointing only at the Program Files copy. `fa install` writes the XML itself, with no
user-supplied path in it. `fa uninstall` stops and removes the task and the binaries; collector data is removed only on
request. Gotcha: Windows locks running executables, so stop the collector before reinstalling, and a rebuild reaches the
collector only after `fa install` again. Installing from `target\` is allowed (it copies); the task never points there.

**D-3 Single instance.** Named mutex `Global\FrameAutopsy.Collector`; a second instance exits at once. The lot 12
watchdog will wait on it.

**D-4 Shared memory (WIN-04).** `CreateFileMappingW` on a named mapping, size fixed at creation, SDDL
`D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;IU)` (full for SYSTEM and Administrators, read for the interactive user), applied
through `ConvertStringSecurityDescriptorToSecurityDescriptorW`. Layout: header (magic, version, size) then fixed slots
(frametime ring, latest probe values, status including self-measured overhead), each slot under a seqlock. The UI side
maps `FILE_MAP_READ`. The collector writes only; it never reads the region back, so a tampered header cannot steer it.
The exact SDDL strings are confirmed by the threat model (task 1.2).

**D-5 SYNC-02 seqlock (`fa-sync`).** Reference: none. We know of no crate offering a seqlock over a fixed-layout,
cross-process mapping with atomic-only access (existing seqlock crates hand out references and assume one address
space); the catalog row says so, and the catalog line is completed before code. Protocol: writer makes the sequence odd,
Release fence, payload as relaxed atomic 64-bit stores, sequence even with Release. Reader: sequence (Acquire), payload
with relaxed atomic loads, Acquire fence, sequence again, retry when changed or odd. No `volatile` (concurrent
non-atomic access is a data race, which Miri reports) and no `&T` or `&[u8]` into the region: only raw pointers cast to
`AtomicU64`. Verification: Miri on two threads in-process (`cargo +nightly miri test -p fa-sync`; Miri is a lot 5
prerequisite), then a cross-process test with a checksum payload for 10 s with zero torn snapshots, retry rate
recorded. Mutation check: weaken each ordering and each fence in turn and verify a test or Miri fails. Each memory
ordering gets its own French explanation checkpoint. Runtime fallback: none needed (atomics are universal on x86-64);
the catalog says so.

**D-6 Command channel (WIN-04 pipes).** Named pipe with SDDL `D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)`, remote clients
rejected (`PIPE_REJECT_REMOTE_CLIENTS`), first-instance flag against pipe squatting. Frame: 4-byte length then
MessagePack. The length is checked before any read of the body: above 64 KiB the connection is closed unparsed. The
schema is a Rust enum (`GetStatus`, `ApplyProposal{id}`, `RejectProposal{id}`, `SetAutonomy{rule_id, game_id, mode}`,
`StartExperiment{rule_id, game_id}`, `StopExperiment{id}`, `EmergencyStop`) with `deny_unknown_fields`. Ids are opaque,
collector-issued, looked up in a collector-side table; `rule_id` and `game_id` are checked against the embedded rules
and matrix. No field can carry a PID, path or raw argument. Every rejection is logged to
`%ProgramData%\FrameAutopsy\logs\` (size-bounded). Parser and whitelist live in `fa-core` (safe); only the pipe
plumbing is in `fa-win`. Property test: arbitrary bytes never panic and never produce a command with an unissued id.

**D-7 Untrusted user-writable inputs.** Baseline (`%LOCALAPPDATA%\FrameAutopsy\health\baseline.json`, written by
non-elevated `fa check`, so user-writable): `fa_probe::store::read_baseline` only (size cap, `deny_unknown_fields`,
schema version). Overrides: `%LOCALAPPDATA%\FrameAutopsy\overrides.toml` (path is Claude's call, to confirm) through
`Matrix::with_override` (16 KiB cap, may only set `forbidden`; an invalid override is ignored as a whole and logged).
Preferences and any other file read from a user-writable path follow the same rule: validate, size cap, strict schema.
A source test checks the collector reads no `%LOCALAPPDATA%` file by another route.

**D-8 Overhead.** The collector samples its own CPU time and private working set once per second (own process through the
pseudo-handle, so no `OpenProcess`) and publishes the 30-minute average in the status block. Budget: under 1 % of one
logical CPU and under 50 MB private working set, `(calibrate)`. The 30-minute measurement is made by the user during
play; any recalibrated value is logged in `docs/decisions.md` section 5.

**D-9 First anti-cheat test.** Protocol `docs/compliance.md` section 6. The user plays at least 3 unranked matches per
matrix game with the collector in observe-only mode, from a known commit. Claude never runs, triggers or simulates it.
Claude records the user's result in `docs/compliance.md` and edits `compliance.toml` only on the user's word; a warning,
kick, error or notice means `forbidden` for that game. Until then the baseline cells stay `unverified`.

## Risks / Trade-offs

- [Binary replacement gives silent admin at logon] -> Program Files location, task XML written by `fa install`, test that
  a non-elevated write is denied.
- [Wrong SDDL grants too much] -> tests read the DACL back and compare it to the expected entries; the threat model
  reviews them.
- [Seqlock ordering bug passes tests by luck, x86 being strongly ordered] -> Miri and per-ordering mutation checks.
- [Pipe client abuse (slow or endless connections)] -> one message at a time, read timeout, connection cap; the collector
  keeps observing if the pipe thread dies.
- [Overhead over budget] -> the budget is `(calibrate)`; measured by the user, changes logged.
- [Anti-cheat flags the collector] -> the user stops the rollout; the baseline cells become `forbidden` for that game.
- [Lot 5 API not final] -> the collector uses the capture through its public surface; names are confirmed when the lot 5
  change is validated.
