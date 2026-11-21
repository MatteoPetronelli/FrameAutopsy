# Proposal

## Why

Encode the games' rules in the product: per game, which observers and actions may run, and make it impossible for the
governor to act on a game, an anti-cheat or a critical process (capability purpose, `specifications.md`).

FrameAutopsy's value depends on never being mistaken for a cheat. Today the rules live only in `docs/compliance.md`, a
draft sourced on 2026-11-20 with several official texts still unread (Fortnite EULA, Blizzard EULA and third-party
software article, Minecraft EULA in full, Riot developer policy). The matrix must exist, with a verdict and a dated
source per cell, before any capture (lot 5), collector (lot 7) or action (lot 12) code can be written. Discipline is not
enough: the rule must be enforced by types and tests.

## What Changes

- Complete the official readings listed in `docs/compliance.md` §2 ("Not yet verified") and show the resulting verdicts
  to the user **before** any code is written; update `docs/compliance.md` with sources and consultation dates.
- Add `compliance.toml` at the repository root: 7 columns (`lol`, `valorant`, `fortnite`, `overwatch2`,
  `minecraft-java`, `fa-lab`, `unknown`), one verdict per observer and action, each with source URLs and a date; embedded
  at build time.
- Add the matrix loader and enablement semantics in `fa-core` (baseline observers on unless `forbidden`; everything else
  only when `allowed`; unknown game = baseline observers only). A user override can only disable.
- Add the `TargetProcess` type in `fa-core` (sole way for the governor to name a process to act on), failing closed on
  games, anti-cheats, critical processes, FrameAutopsy's own processes and truncated 15-byte kernel names.
- Add a source test that fails if `OpenProcess` appears anywhere except `fa-win::process::open_target`.
- Add matrix tests (entry per game and item, `allowed` needs a source and a date) and `TargetProcess` guard tests,
  verified by mutation.
- Create the project skill `frameautopsy-tos-guard` (PASS or BLOCK verdict for any lot touching a data source, an
  action, the UI or the overlay).
- Record the rule that no game data is displayed or signalled during a match (documented and checked by the skill; no
  code path exists yet to enforce it).

## Capabilities

### New Capabilities
- `compliance-matrix`: the per-game capability matrix (`compliance.toml`), enablement semantics, override rule,
  `TargetProcess` guard, the no-handle-on-game rule and the no-game-data-during-play rule.

### Modified Capabilities

None.

## Impact

- **`compliance.toml`**: created by this lot. Every `allowed` cell requires a source URL and a date; games with an
  anti-cheat also require a test record, which only the user can produce. Draft verdicts (`docs/compliance.md` §5)
  stay `unverified` unless an official source settles them; `gray` and `unverified` are never implemented.
- **Never-touch-the-game boundary**: this lot creates its mechanical enforcement (`TargetProcess` fails closed; source
  test on `OpenProcess`). No new capability to touch a game is added; no code opens any process handle.
- **Code**: `fa-core` (matrix, loader, `TargetProcess`, tests); new `compliance.toml`; a test under `fa-core/tests/`
  scanning the workspace sources; new dependencies `serde` and `toml` in `fa-core` (no `unsafe`; `fa-core` keeps
  `#![forbid(unsafe_code)]`).
- **Catalog**: no `fa-asm`, `fa-win` or `fa-sync` module is added or changed, so no ASM, SIMD, SYNC or WIN entry in
  `docs/asm-unsafe-catalog.md` and no explanation checkpoint. `fa-win::process::open_target` itself arrives with lot 12.
- **Docs and skills**: `docs/compliance.md` (sourced, dated), `.claude/skills/frameautopsy-tos-guard/`.
- **Later lots**: 4, 5, 7, 12 and 14 to 16 depend on the matrix and consult `frameautopsy-tos-guard`.
