# `fa check`: machine health check

`fa check` gives, in about 75 s and never more than 90 s, a readable verdict on the machine itself: laptop throttling,
RAM configuration, scheduling interference. It needs no admin rights, no ETW and no game, and it **changes no system
setting** (the power plan and the power mode are read, never written).

```powershell
target\release\fa.exe check          # or target\debug\fa.exe check
```

## What it does

| Phase | Typical time | What |
|---|---|---|
| Refusal checks | under 1 s | Refuses to start while a matrix game runs (see below); warns on battery. |
| Single-core peak | about 4 s | Clock of each physical core in turn, with only that core busy. |
| Memory | about 3 s | Latency up to 64 MiB, cache sizes against the topology, all-core read/write/copy bandwidth, memory modules (WMI). |
| Wake-up | about 3 s | Lateness of a 1 ms high-resolution timer: p50, p99, max. |
| Load | 60 s | FMA load (AVX-512, else AVX2, else scalar) on one logical CPU of every physical core but one; the clock of that one core is read every second. Its SMT sibling stays idle on purpose (a busy sibling makes the probe read about 9 % low). |

The cold phases run first so that our own load threads cannot disturb them. The load is shortened if the 85 s deadline
would otherwise be missed. After the load, a GPU read through NVML (once, read-only: it may wake a sleeping GPU).

## Refusal while a game runs

`fa check` exits with code 3 and a message when one of these is running:

- a game of the compliance matrix found by its full image name (case-insensitive, never a prefix), from a process
  snapshot (no handle on any process);
- Minecraft Java, found by a visible window of class `GLFW30` owned by `java.exe` or `javaw.exe` (the window title
  varies per modpack and is not used). IntelliJ (`idea64.exe`) and VS Code (`Code.exe`) do not match.

Games outside the matrix are not detected: close them yourself, they would disturb the measures.

## Output

- The French report on the terminal: power, CPU, frequency (one line per 5 s of the load), memory, scheduling, GPU, then
  the verdicts. A verdict that fires shows its evidence numbers and a confidence. When nothing fires the report says so
  and lists what was checked.
- `%LOCALAPPDATA%\FrameAutopsy\health\<YYYYMMDDTHHMMSSZ>.json` (UTC): the same data with numbers, for later comparisons.
- `health\baseline.json`: the latest run **on AC power**. A battery run never overwrites it. Anything that reads the
  baseline later must treat it as untrusted input (size cap, strict schema): `fa_probe::store::read_baseline` does.
- At the end, the duration of each phase.

## Verdicts (starting thresholds, `(calibrate)`)

| Verdict | Fires when |
|---|---|
| Throttling | the median clock over a 3 s window, after the first 5 s, falls more than 10 % below the median of the first 5 s. Text: "bridage après X s, −Y %". A drop already under way in the first 5 s is not seen. |
| Single-channel RAM | WMI shows exactly one memory module. Confidence high when the all-core read bandwidth is also under 60 % of the dual-channel theoretical value, medium otherwise. Without WMI there is no verdict. |
| Scheduling interference | wake-up p99 above 5 ms. |

## Exit codes

0 done, 1 error, 2 usage, 3 refused (a game is running), 130 interrupted with Ctrl-C (the load stops within a few
milliseconds, nothing is written).
