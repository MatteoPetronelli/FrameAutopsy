# FrameAutopsy — Compliance (DRAFT)

> **Status: draft from the 2026-11-20 brainstorming.** Sources below were consulted on 2026-11-20. Lot 1
> (`compliance-matrix`) completes the reading of the full official texts and turns this document into
> `compliance.toml`. Only the user can run the anti-cheat tests (section 6); no verdict becomes `allowed` without a
> source and, for games with an anti-cheat, a clean test record.
>
> Verdicts: **allowed** · **gray** · **forbidden** · **unverified**. Never infer a rule from memory.

## 1. Forbidden for every game (product boundary)

- DLL injection; hooks on DirectX, Vulkan or any API in the game process.
- Reading or writing game memory; opening any handle on a game process (not needed: games are seen through ETW).
- Modifying game, launcher or mod files; changing the game's priority, affinity or CPU Sets.
- Kernel drivers, including third-party monitoring drivers; MSR or `rdpmc` access.
- `SeDebugPrivilege`.
- Input automation, macros, anti-cheat bypass or detection.
- Intercepting or modifying game network traffic.
- Any live competitive information; no game data shown during a match.
- Function-level symbolization or disassembly of game modules.

## 2. Sources consulted (2026-11-20)

| Publisher / game | Source | What it says (relevant to us) |
|---|---|---|
| Riot (LoL, TFT, VALORANT) | [Game memory access removed for third-party apps](https://support.riotgames.com/en-us/riot/performance/game-memory-access-removed-for-third-party-apps) (posted 2026-09-29) | From 2026-10-06 Riot proactively prevents game memory access by default for all unknown third-party applications, for all titles; grace period until April 2027 for known approved tools, which must move to official APIs. Nothing about ETW, PresentMon-like tools, separate overlays or other processes' priorities. |
| Riot | [Vanguard updates (DevRel)](https://www.riotgames.com/en/DevRel/vanguard) (2024-02-07) | "External tools reading memory will not be allowed." Expected impact on ~1 % of third-party apps. |
| Riot (VALORANT) | [Third Party Applications](https://support.riotgames.com/en-us/valorant/account/third-party-applications) (2025-02-10) | Forbids apps that expose obfuscated information, act on the player's behalf (botting, scripting) or alter the field of intelligence. No list of safe apps. Nothing about system monitoring. |
| Riot (LoL) | [Disabling overlays](https://support.riotgames.com/en-us/league-of-legends/performance/disabling-overlays) (2026-05-20) | Troubleshooting guide: overlays can cause performance or compatibility problems; not a prohibition. |
| Corsair (about Vanguard) | [iCUE and Riot Vanguard](https://www.corsair.com/explorer/diy-builder/blogs/corsair-icue-and-riot-vanguard/) | Vanguard's kernel-level access has caused problems with hardware monitoring/control software. Supports "no driver" and driverless probes. |
| Epic (Fortnite) | [Easy Anti-Cheat game issues](https://www.epicgames.com/help/c-Category_Fortnite/a000084712?lang=en-US) | Some programs interfere with Easy Anti-Cheat; no list. EULA (full text to read in lot 1) bars unauthorized software giving an advantage, cheats and anti-cheat circumvention. |
| Blizzard (Overwatch 2) | [Defense Matrix](https://overwatch.blizzard.com/en-us/news/24173281/) | Cheat software = third-party code that reads and processes game information for an advantage; against the In-Game Code of Conduct and the Battle.net EULA. Current support article on third-party software not retrieved: to read in lot 1. |
| Mojang (Minecraft Java) | [Minecraft EULA](https://www.minecraft.net/en-us/eula) | Mods allowed (not sold, no substantial Mojang code); distributing modified clients/servers is not. JVM arguments and system monitoring not mentioned (full text to read in lot 1). |
| Windows (context) | [KASLR leaks restriction](https://windows-internals.com/kaslr-leaks-restriction/) | From Windows 11 24H2, several APIs return kernel addresses only with `SeDebugPrivilege` → spike at the start of lot 11. |

**Not yet verified (lot 1):** full EULA texts (Riot ToS, Epic Fortnite EULA, Battle.net EULA and Blizzard support
article on third-party software, Minecraft EULA and usage guidelines); Riot developer policies for local API use; the
date Vanguard became mandatory for League of Legends; which anti-cheats Fortnite runs (EAC, and possibly BattlEye);
the name of Overwatch 2's client anti-cheat component.

## 3. Anti-cheat per game

| Game | Anti-cheat | Level | Notes |
|---|---|---|---|
| League of Legends | Riot Vanguard (to confirm the LoL rollout date from an official source) | kernel, boot-time | memory access blocked for unknown apps since 2026-10-06 |
| VALORANT | Riot Vanguard | kernel, boot-time | no official local game-data API |
| Fortnite | Easy Anti-Cheat (BattlEye to confirm) | kernel | – |
| Overwatch 2 | Blizzard anti-cheat (component name to confirm) | to confirm | – |
| Minecraft Java | none on the client | – | servers may run plugin anti-cheats; servers out of scope |

## 4. Enablement semantics (Claude's call, see `specifications.md` › compliance-matrix)

- **Baseline observers** (O1, O2, O4, O5): enabled for every game unless `forbidden`. They are the PresentMon family
  (system ETW, our own threads, OS counters) and never interact with the game. The anti-cheat smoke test at the end of
  lot 7 confirms them game by game; a failed test makes them `forbidden` for that game.
- **Everything else** (O3, O6, O7, all actions, overlay): enabled only when `allowed`.
- **Unknown games**: baseline observers only (handoff: observe-only on any game).
- **Platform functions outside the matrix**: game detection by full image name (Microsoft-Windows-Kernel-Process ETW
  events, Toolhelp snapshot) and by window title for Minecraft. They open no handle on any process, the same
  information Task Manager shows. Classic kernel process events truncate names to 15 bytes and are not used for
  identification; the `TargetProcess` guard fails closed on a truncated name.

## 5. Capability matrix (draft verdicts)

Legend: **B** = baseline-on (unverified until the lot 7 smoke test); **U** = unverified, off; **A** = allowed;
**F** = forbidden; **n/a** = does not apply.

| Item | Description | lol | valorant | fortnite | overwatch2 | minecraft-java | fa-lab | unknown |
|---|---|---|---|---|---|---|---|---|
| O1 | ETW present events (DxgKrnl/DXGI), system session | B | B | B | B | B | A | B |
| O2 | ETW kernel events (context switch, DPC/ISR, page faults, disk I/O, processes, image loads) | B | B | B | B | B | A | B |
| O3 | ETW kernel stack walks (driver attribution) | U | U | U | U | U | A | U |
| O4 | In-game driverless probes (own threads, non-game CPUs, ≤ 0.01 % duty) | B | B | B | B | B | A | B |
| O5 | System counters: PDH, WMI, NVML, power status | B | B | B | B | B | A | B |
| O6 | Game data source | U (Live Client Data API, official, loopback-only client, post-match only) | F (no official local API; unofficial client API excluded) | F | F | n/a | n/a | F |
| O7 | JVM data: hsperfdata, JFR files | n/a | n/a | n/a | n/a | U (no client anti-cheat; low risk) | n/a | n/a |
| A1 | EcoQoS + below-normal priority of background processes | U | U | U | U | U | A | F |
| A2 | CPU Sets keeping background processes off the game's CPUs | U | U | U | U | U | A | F |
| A3 | Power overlay to best performance (AC) | U | U | U | U | U | A | F |
| A4 | I/O and memory priority of background processes (later) | U | U | U | U | U | A | F |
| A5 | Standby-list purge (later) | U | U | U | U | U | A | F |
| A6 | Timer resolution — **dropped**: per-process since Windows 10 2004, cannot affect a game from another process | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| A7 | Overlay window (separate, topmost, no injection) | U | U | U | U | U | A | F |
| A8 | JVM argument proposal (text the user applies in CurseForge) | n/a | n/a | n/a | n/a | U | n/a | n/a |

## 6. Anti-cheat test protocol (run by the user)

Before an item moves from **U** to **A** (or a **B** item is confirmed) for a game:

1. Use a FrameAutopsy build from a known commit; note the items enabled for the test.
2. Play at least **3 unranked matches** with the anti-cheat active.
3. Record below: date, game version, anti-cheat version if visible, FrameAutopsy commit, items, result.
4. Clean result → update `compliance.toml` with a link to the record. Any warning, kick, error or notice → mark the
   items **F** for that game, revert, and stop the rollout until understood.

| Date | Game | Game version | FrameAutopsy commit | Items tested | Matches | Result | Notes |
|---|---|---|---|---|---|---|---|
| – | – | – | – | – | – | – | – |
