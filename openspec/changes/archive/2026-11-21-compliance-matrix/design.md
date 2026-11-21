# Design

## Context

Lot 0 provides the workspace, `fa-core` (`#![forbid(unsafe_code)]`), the gate and the `frameautopsy-invariants` skill.
Nothing detects games or opens processes yet. The matrix draft is in `docs/compliance.md` §5 and the enablement
semantics (Claude's call, reconciling handoff §2 with §4.2) in §4. The elevated collector will later read the
overrides from a user-writable location, so they are an untrusted input (architecture §3). See proposal.md for the
motivation and `specs/compliance-matrix/spec.md` for the requirements.

## Goals / Non-Goals

**Goals:**
- Make "act on a game, anti-cheat or critical process" unrepresentable: no `TargetProcess` value can exist for them.
- A verdict, a source and a date for every cell, checked by tests, not by review.
- A matrix that fails safe: unknown, missing or malformed means disabled.

**Non-Goals:**
- Game detection (ETW, Toolhelp, `EnumWindows`) and `fa-win::process::open_target`: lots 5 and 12. This lot only
  fixes the rule and the test that guards it.
- Running anti-cheat tests, or turning any cell to `allowed` without a clean user-run record.
- Runtime UI messages ("en attente de vérification de conformité"): lots 9 and 12 consume the matrix API.

## Decisions

**Official readings first (blocking).** Before any code: read in full the Fortnite/Epic EULA (and which anti-cheats
Fortnite runs), the Battle.net EULA and Blizzard's third-party software article (and Overwatch 2's anti-cheat
component), the Minecraft EULA and usage guidelines, the Riot ToS and developer policy for local API use, and the date
Vanguard became mandatory for LoL. Record each in `docs/compliance.md` with URL and consultation date, then present
the proposed verdict per cell to the user. The user validates; only then does `compliance.toml` get its verdicts. A
text that cannot be read or does not settle a point yields `unverified`, never an inference from memory. `gray` and
`unverified` mean **not implemented: ask the user** (they are off, and the skill reports BLOCK if a lot needs them).

**`compliance.toml` format.** One file at the repository root, embedded with `include_str!` (no runtime file, not
replaceable by a non-admin user). Seven game tables (`lol`, `valorant`, `fortnite`, `overwatch2`, `minecraft-java`,
`fa-lab`, `unknown`). Each game has `name`, `detect` (image names; `window_title_prefix` for Minecraft; none for
`unknown`/`fa-lab`), `anti_cheat` (process names and label), and one entry per item O1 to O7, A1 to A5, A7, A8 (A6 is
dropped and absent from the file). An entry is `{ verdict, sources = [urls], date, test_record? }`. Not-applicable cells
from the draft table are written `forbidden` with a note, the only fail-safe value the four-verdict spec allows.
Rejected alternative: a separate `n/a` verdict, which would change the spec.

**Loader in `fa-core`.** `serde` + `toml` with `deny_unknown_fields`; `Matrix::embedded()` parses once and returns a
typed structure keyed by enums (`GameId`, `Item`, `Verdict`) so an item or game cannot be misspelled in code. A
missing cell is a load error. `Matrix::enabled(game, item)` implements the semantics: baseline set (O1, O2, O4, O5) on
unless `forbidden`; every other item only when `allowed`; `unknown` resolves to its own column (baseline observers
only). Baseline membership is a constant in code, tested against the table. Rejected alternative: a boolean per cell,
which would lose the audit trail the spec requires.

**Override can only disable.** Read from the user config path, with a size cap and strict schema (same discipline as
pipe messages, architecture §3), never trusted to widen. The only accepted value is `forbidden` for a known
`game.item`; any other value, unknown game or item, or an attempt to enable (for example `ecoqos = "allowed"` over
`unverified`) is rejected with an error naming the item, and the whole override file is ignored (fail closed, embedded
matrix unchanged). Rejected alternative: accept any value lower than the embedded one, which would let `gray` leave a
baseline observer enabled while claiming to restrict it.

**`TargetProcess`.** In `fa-core`, private fields, constructor `TargetProcess::try_new(pid, image_name, creation_time,
&Matrix) -> Result<TargetProcess, TargetError>`. Refusals, all case-insensitive on the bare image name:
- empty name, or a name containing a path separator (unknown form);
- a name of exactly 15 bytes, possibly a classic kernel truncation (`VALORANT-Win64-`), so it fails closed (cost: a
  legitimate 15-byte background name is not actionable);
- any `detect` name of a matrix game;
- any anti-cheat process listed in the matrix (`vgc.exe`, `vgtray.exe`, `EasyAntiCheat.exe`, `EasyAntiCheat_EOS.exe`,
  `BEService.exe`, plus names confirmed by the readings);
- the critical set (`System`, `smss.exe`, `csrss.exe`, `wininit.exe`, `winlogon.exe`, `services.exe`, `lsass.exe`,
  `dwm.exe`, `audiodg.exe`);
- FrameAutopsy's own binaries and own PID;
- `java.exe` and `javaw.exe`: Minecraft is identified by window title, which the type does not carry, so any bare
  Java launcher is refused (over-refusal accepted, the safe side).

The error states the reason for the log and the user. Adding a refusal is a code change, never a config change.

**Source test on `OpenProcess`.** An integration test in `fa-core/tests/` walks `crates/**/*.rs` (and `build.rs`),
strips comments and strings, and fails on any `OpenProcess` token (and sibling handle openers `OpenProcessToken`,
`NtOpenProcess`, `OpenThread`) outside `crates/fa-win/src/process.rs`, and within it outside the function
`open_target`. Until lot 12 creates that file, the test passes with zero hits and its allowed location is a constant.
Rejected alternative: a clippy `disallowed-methods` entry alone, which does not see raw `windows-sys` re-exports or
macros; it may be added later as a second layer.

**Matrix tests.** (a) every game has an entry for every item; (b) every `allowed` has at least one source URL and an
ISO date; (c) an action enabled for a game without an entry fails; (d) the `unknown` column enables only the baseline
observers; (e) semantics table-driven over the four verdicts; (f) override scenarios. Guard tests (matrix,
`TargetProcess`, thresholds) are verified with `mutation-verified-testing`: break the code (invert a verdict
comparison, drop a critical name, change `== 15` to `> 15`, let an override enable), see the test fail, restore.

**Skill `frameautopsy-tos-guard`.** `.claude/skills/frameautopsy-tos-guard/SKILL.md`, modelled on the conventions of
the private reference repo (`D:\Matteo\RiftMind`, read-only, nothing copied into this public repo). Input: a lot or
diff touching a data source, an action, the UI or the overlay. Checklist: forbidden list (`docs/compliance.md` §1);
every touched item has an `allowed` verdict, a source and a date (baseline observers: not `forbidden`); no `gray` or
`unverified` item implemented; no game data shown during a match; network limited to `fa-attrib`. Output: `PASS`, or
`BLOCK` with the failed rule and the question to ask the user.

**Dependencies.** `serde` and `toml` only, pinned in the workspace; no new crate in `fa-win`, `fa-asm` or `fa-sync`.

## Risks / Trade-offs

- Official texts are ambiguous or unreachable → cells stay `unverified` (off); the user decides, nothing is implied.
- A baseline observer is later judged risky by an anti-cheat → the lot 7 smoke test flips it to `forbidden`; the
  embedded file and the override path both support that without code change.
- The 15-byte rule and the `java(w).exe` refusal over-refuse → accepted: refusing costs a skipped optimisation, acting
  on a game is the failure to avoid.
- The source test is textual and can miss obfuscated calls (macro-generated names) → `unsafe` is confined to three
  crates (lot 0 policy) and the `frameautopsy-tos-guard` review covers the gap.
- Public repo: sources and verdicts are public; `profile.md` is never quoted.

## Migration Plan

New files only. Order: readings and user validation, then matrix and loader, then `TargetProcess`, then source test,
then skill and docs. Rollback: revert the branch; nothing depends on it until lots 4, 5 and 7.
