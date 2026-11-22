# Proposal

## Why

Lots 0 to 4 need no admin. From lot 5 on, observation needs ETW, which needs an elevated process, while the UI must
never be elevated (D5). This lot builds the privilege boundary: an elevated, headless `fa-collector` started at logon,
an installer that keeps its binary out of reach of unprivileged processes, a read-only shared memory the UI maps to see
live data, and a command pipe whose every message is validated against a whitelist. It is also where the first
anti-cheat test of the project happens, run by the user, because the baseline observers stay `unverified` until then.

Capability purpose (from `specifications.md`): an elevated headless collector and a non-elevated UI that share live data
safely, with a strictly validated command channel across the privilege boundary.

## What Changes

- New binary `fa-collector` (elevated, headless, single instance by named mutex). It hosts the lot 5 capture in
  observation only and publishes live frametimes, latest probe values and a status block into shared memory. It has no
  action, no governor and no watchdog yet.
- `fa install` / `fa uninstall` (one UAC prompt each): copy binaries to `%ProgramFiles%\FrameAutopsy`, create
  `%ProgramData%\FrameAutopsy` (write for SYSTEM and Administrators only, users read), register or remove the scheduled
  task "FrameAutopsy Collector" through `schtasks.exe` with an XML that points only at the Program Files copy.
- `fa-win::shm`: named file mapping in the session namespace, DACL by SDDL, UI maps `FILE_MAP_READ` only, the collector
  never reads the region back.
- `fa-win::pipe`: named pipe, DACL by SDDL (interactive user + Administrators), messages at most 64 KiB, length-prefixed
  MessagePack, whitelist schema, collector-issued ids only (never PIDs, paths or raw arguments).
- `fa-sync::seqlock` (SYNC-02): cross-process seqlock with relaxed atomic words and fences, under Miri.
- Validation of user-writable inputs read by the collector: `%LOCALAPPDATA%\FrameAutopsy\health\baseline.json` (written
  by the non-elevated `fa check`) only through `fa_probe::store::read_baseline` (size cap, strict schema), overrides only
  through `fa_core::matrix::Matrix::with_override` (size cap, may only disable).
- Self-measured overhead (CPU and private working set) published in the status block.
- Threat model of the command channel first, shown to the user before any code. First anti-cheat test, run by the user.

## Capabilities

### New Capabilities
- `collector-ipc`: elevated collector, install location, untrusted user-writable inputs, shared memory, cross-process
  seqlock, command channel, overhead budget, anti-cheat smoke test.

### Modified Capabilities

None. `compliance-matrix`, `health-check` and `etw-capture` are consumed, not changed.

## Impact

- **Compliance.** `compliance.toml` is not changed by the code. It changes only after the first anti-cheat test and only
  on the user's word: baseline observer cells (O1 O2 O4 O5) stay `unverified` until then, and a clean result is
  recorded in `docs/compliance.md` section 6 before any cell moves. The collector enables only baseline observers
  (unless `forbidden`) and nothing else: no O3, O6, O7, no action, no overlay. The "never touch the game" boundary is
  unchanged: no handle on any process (`fa_win::process::open_target` does not exist yet, lot 12; this lot must not
  need it, and `crates/fa-core/tests/no_open_process.rs` stays green, extended to the new modules), no injection, no
  game memory, no game files. Nothing here says Riot or any publisher approved anything: the Riot registration is a
  personal project registration only.
- **Catalog modules** (`docs/asm-unsafe-catalog.md`): **SYNC-02** cross-process seqlock (`fa-sync`, unsafe-sync, no
  reference crate, Miri plus cross-process checksum test); **WIN-04** shared memory and named pipes with DACLs
  (`fa-win`, thin FFI, security descriptors are the trust boundary). Both lines are completed before the code. A
  small extra `fa-win` line for install-side calls, if any unsafe is needed, is added the same way.
- **Code:** `fa-sync`, `fa-win`, `fa-core` (command schema and whitelist, safe), `fa` (`install`, `uninstall`), new
  binary `fa-collector`.
- **Dependencies:** `rmp-serde` (MessagePack), `serde`; no async runtime. Miri (`cargo +nightly miri`) is installed in
  lot 5.
- **Storage:** `%ProgramFiles%\FrameAutopsy\` (binaries), `%ProgramData%\FrameAutopsy\` (admin-write, `logs\`), reads
  from `%LOCALAPPDATA%\FrameAutopsy\` as untrusted input.
- **Docs:** `docs/compliance.md` (test record), `docs/decisions.md` section 5 (recalibrated budget values),
  `docs/asm-unsafe-catalog.md` (SYNC-02, WIN-04).
