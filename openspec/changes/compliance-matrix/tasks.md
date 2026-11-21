# Tasks

## 1. Official readings and verdicts (blocking: no code before user validation)

- [x] 1.1 Read in full the Epic Fortnite EULA and Easy Anti-Cheat pages; confirm which anti-cheats Fortnite runs; add URL and date to `docs/compliance.md` §2 (verify: no "to read in lot 1" left for Fortnite)
- [x] 1.2 Read in full the Battle.net EULA and Blizzard's third-party software support article; name Overwatch 2's anti-cheat component; update `docs/compliance.md` (verify: same for Overwatch 2)
- [x] 1.3 Read the Minecraft EULA in full and the usage guidelines; update `docs/compliance.md` (verify: same for Minecraft)
- [x] 1.4 Read the Riot ToS and developer policy for local API use; confirm the date Vanguard became mandatory for LoL; update `docs/compliance.md` (verify: same for Riot)
- [x] 1.5 Present to the user, in French, a verdict per cell (7 columns; items O1 to O7, A1 to A5, A7, A8) with source and date, flagging every point that is `gray` or not settled by an official text; wait for explicit validation (verify: user approval in the conversation; `docs/compliance.md` §5 updated to the approved verdicts)
- [x] 1.6 Gate: `powershell -File scripts/gate.ps1` and `openspec validate compliance-matrix --strict`

## 2. Matrix file and loader (`fa-core`)

- [x] 2.1 Create `compliance.toml` from the approved verdicts (7 columns, source URLs, dates, anti-cheat names, detection rules; not-applicable cells as `forbidden` with a note) (verify: the file parses in 2.2)
- [x] 2.2 Implement `Matrix` with `include_str!`, `deny_unknown_fields`, typed `GameId`/`Item`/`Verdict` and `Matrix::enabled`; add `serde` and `toml` (verify: unit tests for the four verdicts, baseline vs non-baseline items, the `unknown` column)
- [x] 2.3 Implement the disable-only override loader (size cap, strict schema, fail closed) (verify: test for the "override tries to enable an action" scenario naming the item, plus unknown game, unknown item and oversize file)
- [x] 2.4 Add matrix tests: entry for every game and item, `allowed` needs a source URL and a date, no action enabled without an entry (verify: tests pass on the real `compliance.toml`)
- [x] 2.5 Verify the matrix tests by mutation (skill `mutation-verified-testing`): invert the verdict check, let an override enable, drop the source/date check, widen the baseline set; each must turn a test red, then restore (verify: mutation log in the commit or PR notes, tree restored)
- [x] 2.6 Gate: `powershell -File scripts/gate.ps1`

## 3. TargetProcess guard (`fa-core`)

- [ ] 3.1 Implement `TargetProcess::try_new(pid, image_name, creation_time, &Matrix)` with the refusal list from the design (verify: tests refuse the 5 games, the listed anti-cheats, the 9 critical processes, own binaries and own PID)
- [ ] 3.2 Cover the truncated-name case: `VALORANT-Win64-` and any 15-byte name refused; empty, path-bearing and case-variant names handled (verify: "Truncated name" and "Governor aims at the game" scenarios as tests)
- [ ] 3.3 Verify the guard tests by mutation: remove one critical name, one anti-cheat name, change the 15-byte rule, make the comparison case-sensitive; each must fail a test, then restore (verify: mutation log, tree restored)
- [ ] 3.4 Gate: `powershell -File scripts/gate.ps1`

## 4. OpenProcess source test

- [ ] 4.1 Add the integration test scanning `crates/**/*.rs` (comments and strings stripped) for `OpenProcess` and sibling openers outside `fa-win::process::open_target` (verify: passes on the current tree)
- [ ] 4.2 Verify by mutation: temporarily add an `OpenProcess` call in another crate and in `fa-win` outside `open_target`; the test must fail, then remove it (verify: tree restored, no `OpenProcess` left)
- [ ] 4.3 Gate: `powershell -File scripts/gate.ps1`

## 5. Skill and documentation

- [ ] 5.1 Create `.claude/skills/frameautopsy-tos-guard/SKILL.md` (checklist, PASS/BLOCK output, `gray`/`unverified` means ask the user) (verify: run it on a sample diff enabling an `unverified` action and get BLOCK, then on a baseline-observer diff and get PASS)
- [ ] 5.2 Finalise `docs/compliance.md` (remove DRAFT status, link `compliance.toml`) and update the status lines in `CLAUDE.md` if they mention this lot as pending (verify: no cell without a verdict and a dated source; `git diff` limited to these docs)
- [ ] 5.3 Gate: `powershell -File scripts/gate.ps1`, `graphify update .`, `openspec validate compliance-matrix --strict`

## Workflow follow-up

- Archive the change at the end of the lot (`openspec-archive-change`) once the gate is green and the user has approved the verdicts.
- No unsafe or asm block in this lot, so no explanation checkpoint is required.
