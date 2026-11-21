# Probe calibration against HWiNFO (lot 3, manual, off-game)

Purpose: check that the effective-frequency probe (ASM-02) and the Windows PDH counter agree with HWiNFO's *Effective
Clock*, and record the three numbers side by side. Only you run HWiNFO; Claude never touches it. Target agreement: within
3 % (calibrate; see `docs/decisions.md` section 5 if it has to change).

This is the only calibration step of lot 3. The keep/remove decision on the frequency probe is taken in lot 10.

## Before you start

1. **No game running**, no launcher, no anti-cheat service busy. Close browsers, chat apps, backups, updaters.
2. **AC power**, same mode as the earlier measurements: Windows performance mode and OMEN performance mode, **no
   overclock** (it overheats the machine).
3. Let the PC cool for a few minutes after any heavy work (a build, a game): throttling steps change the numbers.
4. Build once, from the repo root: `cargo build --release -p fa` (release is not required but gives a quieter run).
   The binary is `target\release\fa.exe`.
5. Open **HWiNFO64** in *Sensors-only* mode. In the sensor list find the per-core **Core N T0 Effective Clock** rows
   (and the *Core N Clocks* rows if you want the plain clock too). Core numbering in HWiNFO may differ from Windows CPU
   numbers, so identify the right row in step A below.

## Step A: one core busy (probe alone)

The probe keeps its own core busy while it samples, so this phase needs no extra load.

1. In HWiNFO, reset the min/max/avg counters (the clock icon at the bottom of the Sensors window).
2. In a terminal (not elevated is fine), run:

   ```powershell
   target\release\fa.exe probe freq --seconds 60 --cpu 4 | Tee-Object probe_one_core.csv
   ```

3. While it runs, look at HWiNFO: exactly one core's Effective Clock sits near the probe value. That row is the one
   to read. Write its **Avg** column when the 60 s end.
4. In `probe_one_core.csv` compute the mean of `probe_ghz` and of `pdh_ghz` (the last column) over the 60 lines.

## Step B: all cores busy

1. Reset the HWiNFO counters again.
2. Run:

   ```powershell
   target\release\fa.exe probe freq --seconds 60 --cpu 4 --load-all | Tee-Object probe_all_cores.csv
   ```

   (`--load-all` spins one of our own threads on every other logical CPU: it is a plain compute loop, nothing else.)
3. Read the **Avg** of the same HWiNFO core row, and again the means of `probe_ghz` and `pdh_ghz`.

## Results to give Claude

| Step | Probe mean (GHz) | PDH mean (GHz) | HWiNFO Avg Effective Clock (GHz) | Probe vs HWiNFO | PDH vs HWiNFO |
|---|---|---|---|---|---|
| A: one core busy | | | | | |
| B: all cores busy | | | | | |

Send the two numbers per row (or the two CSV files and the HWiNFO averages). Claude records them in
`docs/asm-unsafe-catalog.md` (ASM-02 line) and notes whether each gap is under 3 %.

## How to read the result

- Probe and PDH close to each other but both far from HWiNFO: check you read the right HWiNFO core row, and that HWiNFO
  shows *Effective* clock, not the requested clock.
- Large gaps only in step B: expected to be wider, because the clock steps down during the minute (power or thermal
  limit) and the three tools average over different windows. Compare means over the whole minute, not single seconds.
- If the probe is fine in step A and in the means of step B, ASM-02 is accurate enough for lot 3. It does **not** prove
  the probe is worth keeping over PDH: that is decided at lot 10 with Lab soaks.

## What this procedure does not cover

- Idle: the probe keeps its core busy while sampling, so an idle-core reading is not available before lot 8
  (duty-cycled sampling).
- Battery, other power plans, a cold start, or a hot machine.
