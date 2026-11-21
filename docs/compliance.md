# FrameAutopsy — Compliance

> **Status (2026-11-21): verdicts approved by the user in lot 1.** The machine-readable matrix is
> [`compliance.toml`](../compliance.toml) (verdict, sources, date per cell); this document holds the reasoning and
> sources. Only the user can run the anti-cheat tests (section 6); no verdict becomes `allowed` without a source and,
> for games with an anti-cheat, a clean test record.
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

### 2.1 Official texts read in lot 1 (2026-11-21)

None of these texts names ETW, PDH/WMI/NVML, process priority, CPU Sets, power plans, the standby list or non-injected
overlays: every observer and action item is "not addressed" by every publisher. Read results, not legal advice.

| Publisher | Texts read (date shown on the page) | Facts used |
|---|---|---|
| Epic (Fortnite) | [Terms of Service](https://legal.epicgames.com/epicgames/tos) (2026-09-10; the Fortnite EULA URLs redirect here); [Community Rules](https://legal.epicgames.com/epicgames/community-rules) (2026-02-26); [EAC player FAQ](https://www.easy.ac/support/articles); Epic help pages on EAC and BattlEye; [BattlEye FAQ](https://www.battleye.com/support/faq/) | ToS §4 bans cheats ("unfair advantages"), tampering with Gameplay Integrity Tools, and says Epic will not reveal what its tools detect. EAC "blocks software that interacts with games in ways that look identical to cheating techniques", possibly overlays and monitoring tools; blocking, not a ban. BattlEye FAQ: non-cheat overlays "generally supported". **Fortnite runs both EAC and BattlEye** (Epic help pages). Not read: legacy EULA PDF, EOS anti-cheat docs. |
| Blizzard (Overwatch 2) | Battle.net EULA (2024-03-21, [US](https://www.blizzard.com/en-us/legal/fba4d00f-c7e4-4883-b8b9-1b4500a402ea/blizzard-end-user-license-agreement), [EMEA](https://www.blizzard.com/en-gb/legal/08b946df-660a-40e4-a072-1fbde65173b1/blizzard-end-user-license-agreement)); [Anti-cheating Agreement](https://www.blizzard.com/en-gb/legal/cd5930c0-2784-420c-a23d-1e0d6ff8599b/anti-cheating-agreement) (2016-02-03); [Defense Matrix](https://overwatch.blizzard.com/en-us/news/24173281/) (2025-01-30) | EULA §1.C.vi bans software that "intercepts, collects, reads, or mines information generated or stored by the Platform"; §4 allows monitoring of out-of-process programs. The Anti-cheating Agreement says it scans the process list and "which processes have a handle to the Blizzard games process". **The client anti-cheat component is not named in any official text.** **Not read (login wall): In-Game Code of Conduct and Blizzard support articles on third-party software.** |
| Mojang (Minecraft Java) | [EULA](https://www.minecraft.net/en-us/eula) and [Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines) (no date on the pages); [Microsoft Services Agreement](https://www.microsoft.com/en-us/servicesagreement) (effective 2026-09-30, partly keyword-scanned); [Xbox Community Standards](https://www.xbox.com/en-US/legal/community-standards) §7 (2024-05) | EULA: "You may develop tools, plug-ins and services as long as they do not seem official or approved by us"; usage guidelines: "If something isn't covered ... that probably means we don't want you to do it". No text about system monitoring, JVM files, JVM arguments or overlays. **No official text found on client anti-cheat.** |
| Riot (LoL, VALORANT) | [Terms of Service](https://www.riotgames.com/en/terms-of-service) (2024-12-01); [Developer General Policies](https://developer.riotgames.com/policies/general) (2025-05-29); [LoL docs](https://developer.riotgames.com/docs/lol) and [VALORANT docs](https://developer.riotgames.com/docs/valorant) (no date); [Third Party Applications](https://support.riotgames.com/en-us/riot/events/third-party-applications) (2025-02-10); [Vanguard FAQ for third-party apps](https://www.riotgames.com/en/DevRel/vanguard-faq) (2024-04-01) | **Registration clause: "If your product serves players, you must register it with us regardless of whether or not your product uses official documented APIs."** ToS §7.1(11) bans "unauthorized third party programs" that interact with the services, undefined. LoL game policy approves only overlays with "static data that is available prior to the game"; VALORANT lists real-time overlays and "apps ... designed for personal use only" as unapproved. VALORANT documents no local client API. |

Vanguard mandatory for League of Legends: **patch 14.9, 2024-04-30** ("You will be unable to queue for or play League
until Vanguard is installed and active"; live in the Philippines with 14.8 on 2024-04-16). No official page confirms the
exact worldwide enforcement day.

**Riot Developer Portal (checked by the user, 2026-11-21):** "Register Product" only offers a Production API key or a
Personal API key; FrameAutopsy uses no Riot API, so neither fits. The question is sent to DevRel (draft in
`RIOT_DEVREL_MESSAGE.md`, local). **Blizzard:** the user confirmed in a logged-in session that the support texts add
nothing beyond the Anti-cheating Agreement and the EULA already read.

**Registration done (2026-11-21):** the product was registered on the Riot Developer Portal as a personal project; the
Portal answered "Your application has been created and is approved!" after an automatic check of the description. This
is a registration status, not an endorsement: it must never be presented as approval by Riot (developer policy: do not
"refer to your project to be in a partnership or approved by Riot"). It answers no technical question (ETW, stack
walks): the matrix is unchanged, Riot games stay baseline-only.

**Open points:** Riot DevRel should be asked whether registration applies to FrameAutopsy and whether kernel stack
walks count as "game memory access" (S7, 2026-10-06). Blizzard's Code of Conduct and support articles remain unread.

## 3. Anti-cheat per game

| Game | Anti-cheat | Level | Notes |
|---|---|---|---|
| League of Legends | Riot Vanguard (mandatory since patch 14.9, 2024-04-30) | kernel, boot-time | memory access blocked for unknown apps since 2026-10-06 |
| VALORANT | Riot Vanguard | kernel, boot-time | no official local game-data API |
| Fortnite | Easy Anti-Cheat **and** BattlEye (Epic help pages, 2026-11-21) | kernel | – |
| Overwatch 2 | Blizzard anti-cheat (component not named in official texts) | not stated | – |
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

## 5. Capability matrix (verdicts approved by the user, 2026-11-21)

Approved in lot 1: baselines only for Riot (LoL, VALORANT) until the product is registered on the Riot Developer Portal and
Riot DevRel has answered (section 2.1); Minecraft O7 and A8 `allowed` (EULA permits third-party tools that do not pose as
official); A7 `unverified` everywhere except `fa-lab`. Baseline items stay unconfirmed until the lot 7 anti-cheat test.

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
| O7 | JVM data: hsperfdata, JFR files | n/a | n/a | n/a | n/a | A (EULA allows third-party tools; read-only file reads) | n/a | n/a |
| A1 | EcoQoS + below-normal priority of background processes | U | U | U | U | U | A | F |
| A2 | CPU Sets keeping background processes off the game's CPUs | U | U | U | U | U | A | F |
| A3 | Power overlay to best performance (AC) | U | U | U | U | U | A | F |
| A4 | I/O and memory priority of background processes (later) | U | U | U | U | U | A | F |
| A5 | Standby-list purge (later) | U | U | U | U | U | A | F |
| A6 | Timer resolution — **dropped**: per-process since Windows 10 2004, cannot affect a game from another process | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| A7 | Overlay window (separate, topmost, no injection) | U | U | U | U | U | A | F |
| A8 | JVM argument proposal (text the user applies in CurseForge) | n/a | n/a | n/a | n/a | A | n/a | n/a |

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
