---
name: frameautopsy-tos-guard
description: FrameAutopsy compliance gate. Use before merging any lot or diff that touches a data source, an action on another process or the system, the UI, or the overlay. Returns PASS or BLOCK against compliance.toml and the never-touch-the-game rule.
---

# FrameAutopsy ToS guard

Source of truth: `compliance.toml` (verdicts, sources, dates), `docs/compliance.md` (forbidden list, sources, protocol).
Never infer a game rule from memory: a rule not in an official text read and dated in `docs/compliance.md` is unknown.

## Input
A lot or a diff that adds or changes: an observer (ETW provider, probe, counter, game data source, JVM data), an action
on another process or the system (priority, EcoQoS, CPU Sets, power, memory), the UI, or the overlay.

## Checklist (stop at the first failed rule)
1. **Forbidden list** (`docs/compliance.md` section 1): no injection, hook, game memory access, handle on a game process
   (`OpenProcess` only in `fa-win::process::open_target(&TargetProcess)`), file changes to game/launcher/mods, change of
   the game's own priority/affinity/CPU Sets, kernel driver, MSR/`rdpmc`, `SeDebugPrivilege`, input automation, anti-cheat
   bypass or detection, network interception, function-level symbolization of game modules.
2. **Every touched item** (O1..O7, A1..A5, A7, A8) is looked up in `compliance.toml` for each game it can run on, plus
   `unknown`:
   - baseline observers (O1, O2, O4, O5): the verdict is not `forbidden`;
   - every other observer, every action, the overlay: the verdict is `allowed`, with a source URL and an ISO date, and
     for a game with an anti-cheat a test record (`docs/compliance.md` section 6).
3. **`gray` or `unverified` means not implemented.** Do not code around it: ask the user.
4. Actions act only through `TargetProcess`; the matrix is read through `Matrix::enabled`, never re-implemented.
5. No game data shown or signalled during a match (post-match reports only).
6. Network: only `fa-attrib` reaches a non-loopback host; the LoL Live Client is `127.0.0.1` only. No telemetry.
7. A new override path can only set `forbidden`.

## Output
- `PASS`: one line per touched item with its verdict and source date.
- `BLOCK <rule number>`: the failed rule, the item and game, and the exact question to put to the user
  (for example: "A1 EcoQoS is `unverified` for valorant: do you want to run an anti-cheat test, or keep it off?").
  A BLOCK ends the review: no workaround, no "temporary" enablement.

## Examples
- Diff enabling A1 for `valorant` while `compliance.toml` says `unverified`: `BLOCK 2` (A1, valorant), ask the user.
- Diff adding a PDH counter read (O5), `lol` baseline `unverified` (not `forbidden`): `PASS`, no handle on a game process.
- Diff calling `OpenProcess` outside `open_target`: `BLOCK 1`; the source test `no_open_process` also fails.
