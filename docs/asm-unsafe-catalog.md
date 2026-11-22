# FrameAutopsy — asm and unsafe catalog ("earn its place" ledger)

> Every asm or unsafe module of FrameAutopsy is listed here **before** it is written, with the reference it must
> beat. Each lot fills in the "Result" and "Verdict" columns from its benchmark or comparison. A module that does not
> beat its reference is deleted and its line kept with the numbers, so the decision stays visible.
> Rule: `unsafe-policy` in `specifications.md`. Decision D1 in `docs/decisions.md`.

## 1. Tiers

| Tier | Meaning | Expected outcome |
|---|---|---|
| **asm-mandatory** | No other way on stable Rust, or the compiler would change the semantics (folding a dependency chain, reordering a timed region, unstable intrinsic) | Ships if it does its job; reference is the closest non-asm fallback |
| **asm-candidate** | Asm competes with intrinsics and safe Rust; the benchmark decides | Often loses to intrinsics; kept only on a measured win |
| **intrinsics** | `core::arch` intrinsics (unsafe because of `target_feature`), no asm planned | Must beat auto-vectorized Rust |
| **unsafe-FFI** | Windows API calls; no safe alternative except wrapper crates | Exempt from benchmarks unless a wrapper crate meets the requirements |
| **unsafe-sync** | Hand-written concurrency over raw memory | Must match or beat the reference crate; Miri-verified |

**Honest share.** Asm stays a minority of the lines. After the builder review, only one module is asm-mandatory
(the frequency probe, ASM-02), and it must still beat the PDH counter at lot 10. Every other asm line is a candidate
that ships only if it beats intrinsics on the target machine. The `unsafe` share (FFI, sync, SIMD intrinsics) is
substantial and concentrated in three small, separately verified crates. This is the price of rule D1: less asm, but
every line that remains is load-bearing and proven.

## 2. Benchmark protocol

- Machine state: on AC power, best-performance power overlay, no game running, background apps closed, laptop on a
  hard surface; record CPU model, microcode if visible, Windows build, power plan.
- Thread pinned to one P-core (or the fastest core per `cpu-topology`); 3 criterion runs; report median and spread.
- Same input data for reference and candidate; parity test run first (a bench of a wrong kernel is void).
- Record the result in the table below, with date and commit; `FA_FORCE_SCALAR=1` run included for fallbacks.
- A "win" must exceed the run-to-run spread; a win under 5 % is treated as a tie, and on a tie the simpler code ships
  (reference).

## 3. Target machine

Intel Core i5-11400H (Tiger Lake-H): 6 identical cores / 12 threads, one LLC; SSE4.2, AVX2, FMA, **AVX-512**
(AVX-512F confirmed by CPUID on 2026-11-21; VL/BW/DQ not checked); **no WAITPKG** and **no MWAITX**, both confirmed
absent by CPUID on 2026-11-21 (7.0 ECX[5] = 0, 0x80000001 ECX[29] = 0). Invariant TSC and `rdtscp` present; CPUID
`0x15` reports a 38.4 MHz crystal with ratio 140/2, i.e. a nominal 2 688 000 000 Hz (to be checked against the QPC
calibration, lot 2); leaf `0x16` returns zeros. `__cpuid` is an `unsafe` function in Rust 1.92. A module whose only
win would be on hardware absent from this machine is not built (D1: it could never be proven). Memory latency measured 2026-11-21 (`fa probe curve`, CPU 2, 4 KiB pages, AC, quiet system, 3 runs): L1 about 1.25 ns up to 48 KiB, L2 about 3.5 ns up to about 1 MiB, then 10 to 25 ns around 1.5 to 3 MiB, and RAM at 100 to 110 ns from 4 to 6 MiB on, although the topology reports a 12 MiB L3. The curve is noisier when a background build runs. Cause of the early rise not established (candidates: TLB reach with 4 KiB pages, L3 shared with the iGPU or partitioned); large pages would separate the TLB effect but are not available (WIN-09).

## 4. Ledger

### `fa-asm`

| Id | Module | Lot | Tier | Reference | Metric | Parity test | Fallback / CPU feature | What would break it | Result | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| ASM-01 | `tsc::now`, `now_serialized` (`lfence; rdtsc; lfence`, `rdtscp; lfence`) | 2 | asm-candidate | `_rdtsc` / `__rdtscp` + `_mm_lfence` | read overhead (cycles) and jitter (stdev) | same tick within overhead window; monotonic | QPC if no invariant TSC (CPUID 0x80000007) | missing `lfence` lets the CPU reorder the read around the timed code | 2026-11-21, commit 832a6cf, i5-11400H on AC (HP recommended plan, power overlay not verified), process pinned to CPU 4, criterion 1 s warm-up + 3 s, 3 runs + 1 `FA_FORCE_SCALAR=1` run. Median ns per read, reference vs asm: `now` 17.04/16.65/17.26 vs 16.13/15.85/17.00 (-5.3/-4.8/-1.5 %, forced run -4.1 %); `now_serialized` 17.14/17.46/17.29 vs 16.34/16.97/16.49 (-4.7/-2.8/-4.6 %, forced run -5.9 %). Std dev 0.3 to 2.7 ns per run; the gain is 0.5 to 0.9 ns | **tie** (median gain under 5 % and inside the spread): intrinsics ship, asm deleted (code kept in commit ad13944) |
| ASM-02 | frequency probe (dependent `add` chain) | 3 | asm-mandatory | PDH `% Processor Performance` × nominal | agreement ±3 % at steady state + information PDH lacks (instant per-core value) | Rust `black_box` chain gives the same GHz within 3 % | PDH counter | compiler folding the chain (why asm); preemption mid-sample (rejected by median rule) | 2026-11-21, i5-11400H on AC, 60 s each, `fa probe freq` (data in `lab/fixtures/probe/`). One core busy (probe alone): probe mean 4.084 GHz, PDH 4.096 GHz, per-second gap mean 0.38 % (max 2.09 %), 1.2 % of samples rejected. All 12 CPUs loaded: probe 3.407 GHz, PDH 3.387 GHz, per-second gap mean 2.35 % (max 7.87 %), mean-vs-mean gap 0.58 %, 10 % rejected (thermal/power throttling steps down to 3.0 GHz). Reference chain parity: within 3 % in the unit test (3 `imul` cycles per step; the first two reference forms failed, see `fa-asm::freq` header). | accuracy measured; HWiNFO comparison NOT done: HWiNFO (Microsoft Store and portable builds) closes right after the Sensors-only choice on the target machine, probably because its kernel driver is refused (cause not investigated); user decision 2026-11-22: close lot 3 without it, so ASM-02 accuracy rests on PDH alone. Last user run (60 s, CPU 4, other physical cores loaded, sibling idle): probe about 4.088 GHz, PDH 4.0817 GHz, gap about 0.15 %; seconds 55-57 and 60 dip to 3.98-4.05 GHz on the probe only (more rejected samples), cause not investigated. SMT sensitivity found 2026-11-21 (user run, 60 s, CPU 4): with the sibling CPU 5 busy the probe median reads 3.712 GHz while PDH reads 4.082 (9.1 % low; the probe max column, 4.09, matches PDH); with the sibling idle (every other physical core loaded) 4.089 vs 4.082 (0.17 %). The add chain loses issue slots to a busy sibling, so a probe on a core whose SMT sibling is busy under-reads. One-core run, sibling idle: probe about 4.3 GHz, PDH about 4.29 GHz, within 1 %. curves accepted and checkpoint "compris" by the user 2026-11-21; keep/remove decided at lot 10. Power: Windows performance mode, no overclock (the OMEN gaming mode plus overclock overheats the machine), not the "Best performance" overlay |
| ASM-03 | pointer chase (memory latency) | 3 | asm-candidate | Rust `read_volatile` chain | coefficient of variation of ns/load | same ns/load ±2 % | Rust chain | prefetchers if the permutation is not a single random cycle | 2026-11-21, AC, pinned on CPU 4, 3 runs x 100 000 loads (median of 100 samples): L1 32 KiB ring: reference 126.5 / 124.4 / 126.8 us, asm 129.1 / 125.7 / 128.4 us; L2 512 KiB: 439.6 / 439.6 / 444.7 vs 440.8 / 440.3 / 436.8 us; RAM 64 MiB: 12.7 / 11.2 / 11.3 ms vs 11.3 / 12.1 / 11.2 ms (about 1.3 ns, 4.4 ns and 112 ns per load). Differences are inside the spread and change sign between runs. | **tie: Rust `read_volatile` chain ships**; asm and bench removed (git history, commit before this note) |
| ASM-04 | bandwidth (AVX2 loads, `vmovntdq` NT stores, copy) | 3 | asm-candidate | intrinsics (`_mm256_stream_si256`…) | GB/s | same bytes written/read, checksum | SSE2 intrinsics → scalar | missing `sfence` after NT stores | 2026-11-21, AC, pinned on CPU 4, 3 runs (criterion median, GiB/s, intrinsics vs asm): L2 512 KiB read 108-124 vs 123-125, write 27.6-30.9 vs 29.4-31.4, copy 29.2-30.6 vs 27.8-29.9; RAM 64 MiB read 18.1-19.8 vs 19.9-20.3, write 30.1-32.3 vs 29.3-32.4, copy 9.0-13.2 vs 12.8-13.2. Median gain of asm per operation: copy L2 -2.0 %, read L2 -0.3 %, write L2 +5.9 % (intrinsics runs vary 27.6 to 30.9), copy RAM +0.3 %, read RAM +4.5 %, write RAM +1.4 %. None beats the spread consistently. Multi-thread (`fa probe bw`, 64 MiB per thread, AVX2 intrinsics): read 18 GB/s on 1 core, about 32 on 2 cores and flat from there; write about 30 flat; copy about 25 to 31. Saturation from 2 cores on. Scalar fallback: read 36, write 13.6, copy 31 GB/s on 5-6 cores (the compiler vectorizes the scalar loops; write is slower without non-temporal stores). | **tie: `core::arch` intrinsics ship** (AVX2, SSE2, scalar tiers); asm and bench removed (git history) |
| ASM-05 | `tpause`/`umwait` (WAITPKG), `monitorx`/`mwaitx` (AMD) | – | – | `pause` spin | – | – | – | – | target CPU has neither feature (CPUID-checked by a test in `fa-asm::cpuid`, 2026-11-21) | **dropped** (cannot be proven on the target machine) |
| ASM-06 | FMA power load (health-check soak, optional Lab load): 8 independent FMA accumulator chains saturating both FMA ports (4-cycle latency x 2 ports needs at least 8), AVX2 `ymm` and AVX-512 `zmm` variants, run on one logical CPU of every physical core except the sampler's core and its SMT sibling | 4 (reused in 6) | asm-candidate | intrinsics (`_mm256_fmadd_ps`, `_mm512_fmadd_ps`) | FLOP per TSC tick (closeness to 2 FMA/cycle) and resulting package heat (frequency drop curve) | same FLOP count and final accumulator values | intrinsics; AVX-512 → AVX2 → scalar `f32::mul_add` by dispatch (`FA_FORCE_SCALAR=1` forces the scalar one) | too few independent chains → latency-bound, not port-bound; AVX-512 can lower the clock on its own (the report names the ISA used) | 2026-11-22, AC, pinned on CPU 4 (sibling idle), 3 runs of 100 000 iterations (criterion median, GFLOP/s; intrinsics vs asm): AVX-512 129.3 / 128.2 / 130.2 vs 136.0 / 130.1 / 131.3 (48 FLOP per TSC tick, about 31 per core cycle: one 512-bit FMA unit saturated); AVX2 122.9 / 116.1 / 116.7 vs 110.2 / 115.7 / 116.0 (43 to 46 per TSC tick). Median gain of asm: AVX-512 +1.5 %, AVX2 -0.9 %. Parity: every variant (AVX-512, AVX2, scalar `f32::mul_add`, asm included while it existed) ended on the same bits; a mutation in one asm instruction made the parity test fail. | **tie: intrinsics ship** (AVX-512, AVX2, scalar); asm and bench removed (git history, commit before this note). The load itself is delivered for `fa check` and the Lab |
| ASM-07 | `clflushopt` sweeps (Lab `cache-thrash`) | 6 | intrinsics (asm only if the intrinsic is unstable) | `_mm_clflushopt` / `_mm_clflush` | n/a (behavioral: measured latency jump after flush) | n/a | `clflush`; CPUID 7.0:EBX[23] | missing fence before timing | – | pending |
| SIMD-01 | Stream VByte encode/decode (`pshufb`) | 8 | intrinsics + asm-candidate | scalar varint; and "delta + zstd" for the stage itself | decode and encode GB/s; ratio after zstd | round-trip equality on random and real timestamps | scalar; SSSE3/AVX2 | wrong shuffle table entry corrupts silently (round-trip test catches it) | – | pending |
| SIMD-02 | lagged cross-correlation (FMA) | 10 | intrinsics + asm-candidate | scalar Rust; FFT with `rustfft` | time for 1,000 series × 10,000 bins × 401 lags | results within 1e-4 relative of scalar f64 | scalar; FMA + AVX2 | accumulation order changes results (tolerance-based parity) | – | pending |
| SIMD-03 | f32 selection / sort network | 13 | intrinsics + asm-candidate | `select_nth_unstable_by` | ns per element at 36k–100k frames | identical selected value | scalar; AVX2 | NaN handling (frametimes validated non-NaN upstream) | – | pending |
| SIMD-04 | xoshiro256++ ×4 (AVX2), ×8 (AVX-512) | 13 | intrinsics | scalar xoshiro256++ (e.g. `rand_xoshiro`) | numbers per second | exact stream equality per lane for the same seeds | scalar; AVX2 / AVX-512F | lane seeding mistakes break reproducibility (exact parity catches it) | – | pending |

### `fa-sync`

| Id | Module | Lot | Tier | Reference | Metric | Verification | What would break it | Result | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| SYNC-00 | counting global allocator (`test-support` feature, `unsafe impl GlobalAlloc`) | 0 | unsafe (test only) | none (needed by the zero-allocation tests) | n/a | self-test: a known allocation is counted (per-thread `Cell` counter) | n/a | counting in the allocator itself allocating | – | needed |
| SYNC-01 | SPSC ring (padding, Acquire/Release, `MaybeUninit`); 2026-11-22 line written before the code | 5 | unsafe-sync | `rtrb` (expected outcome: may tie, 4 of 5 candidates so far tied; a win under 5 % ships `rtrb`) | throughput (records/s) and one-way latency, 32-byte records, producer and consumer threads on two different physical cores, 3 runs, median and spread | parity test against `rtrb` (same sequence in, same out); Miri two-thread stress (also with several seeds); criterion | Relaxed where Acquire is needed → stale reads; missing padding → false sharing; a slot written after the tail is published → torn read | Measured 2026-11-22, AC power, producer on CPU 2 and consumer on CPU 4 (different physical cores), 1024 slots, 3 runs, medians (spread in brackets). Throughput, 2 M records: ours 124.4 / 123.1 / 85.7 Melem/s [79 to 130] against `rtrb` 101.2 / 71.8 / 64.5 Melem/s [61 to 104]: ours ahead in all 3 runs (+23 %, +71 %, +33 %), ranges never overlapping; run 3 was slow for both (machine noise). One-way latency (half a round trip, 200 k round trips): ours 64.9 / 65.0 / 65.0 ns, `rtrb` 70.2 / 70.4 / 61.6 ns: a tie (gap 8 % then -5 %, order reverses in run 3). Explanation checkpoint: user answered "compris" | **Kept**: throughput wins by 5 % or more in every run, latency ties. Likely cause: power-of-two capacity (mask instead of modulo); not isolated |
| SYNC-02 | cross-process seqlock over a fixed-layout mapping (relaxed atomic 64-bit words + Acquire/Release fences; no volatile) | 7 | unsafe-sync | none exists for cross-process fixed layouts (justification) | correctness (no torn read), reader retry rate | Miri in-process; cross-process test with checksums | forming `&T` into shared memory (UB); volatile instead of atomics (data race); missing fences | – | pending |
| SYNC-03 | shared-region reader for hsperfdata (relaxed atomic copies) | 15 | unsafe-sync | plain file reads (`std::fs`) at 10 Hz | sampling cost; consistency | comparison test against file reads | same as SYNC-02 | – | pending (likely loses → file reads) |

### `fa-win` (FFI, exempt from benchmarks unless a wrapper crate qualifies)

| Id | Module | Lot | Reference crate considered | Why hand-written (or not) |
|---|---|---|---|---|
| WIN-01 | ETW session and consumer (private system-logger session, `ClientContext = 3`, raw timestamps, event-id filter on DxgKrnl); comparison protocol written 2026-11-22 before the code: parity = both consumers give the same records for the same events, metrics = allocations per event and events/s, 3 runs, median and spread; `ferrisetw` resolves schemas through TDH, so a synthetic stream without registered schemas may not be comparable, in which case the comparison runs on a live capture and the limit is recorded | 5 | `ferrisetw` | zero allocation per event, raw TSC timestamps (`ClientContext = 3` + `PROCESS_TRACE_MODE_RAW_TIMESTAMP`), stack-walk setup. Single rule: the hand-written consumer stays for hot providers only if it shows 0 allocations per event **and** throughput ≥ `ferrisetw`; otherwise `ferrisetw` is used. Non-hot providers use `ferrisetw` if it is adopted at all. |
| WIN-02 | process, priority, EcoQoS, CPU Sets, power overlay, `ntdll` native calls | 12 | `windows` / `windows-sys` bindings only | thin RAII wrappers; no wrapper crate covers the set |
| WIN-03 | topology (`GetSystemCpuSetInformation`, `GetLogicalProcessorInformationEx`) | 2 | – | variable-length records need careful pointer walking |
| WIN-04 | shared memory and named pipes with DACLs (SDDL) | 7 | – | security descriptors are the trust boundary |
| WIN-05 | D3D11 render loop for the Lab | 6 | `windows` crate COM | COM calls are unsafe by nature |
| WIN-06 | Direct2D + DirectComposition overlay window | 16 | `windows` crate | same |
| WIN-07 | thread pinning (own thread only: `SetThreadAffinityMask` on the current-thread pseudo-handle, RAII guard that restores the mask); high-resolution waitable timers | 2 (pinning), 3 (timers) | – | small; pinning moved to lot 2 (user decision 2026-11-21) so the clock tests and the probes share one base. Timers measured 2026-11-21 (`fa probe wake`, 2000 waits of 1 ms, AC, performance mode): lateness p50 524 us, p99 3.2 ms, max 10.5 ms (CPU 7: 522 us / 3.3 ms / 11.6 ms). The high-resolution timer therefore does not wake on time to the microsecond on this machine: expect about half a millisecond of lateness. Clock domain (`fa probe domain`, core 2 probed, core 4 spinning, 5 rounds): 4.089 GHz idle and loaded, **independent**; this only shows that one extra busy core does not move core 2, so it cannot separate a shared clock from a shared turbo limit |
| WIN-11 | QPC read and frequency (`QueryPerformanceCounter`, `QueryPerformanceFrequency`) for `fa-win::clock` | 2 | – | small |
| WIN-08 | PDH reader (`PdhOpenQueryW`, `PdhAddEnglishCounterW`, `PdhCollectQueryData`, `PdhGetFormattedCounterValue`): `\Processor Information(*)\% Processor Performance` for the ASM-02 comparison | 3 | – | FFI with out-parameters; English names because localized names break on other Windows languages. Corrects the older "PDH: no unsafe" line: no safe PDH crate is planned. Measured 2026-11-21: `% Processor Performance` x `Processor Frequency` (2688 MHz) gives 4.08 GHz one core busy and agrees with the ASM-02 probe within 0.2 % (sibling idle); unit test checks a plausible value |
| WIN-09 | large-page allocation (`VirtualAlloc` + `MEM_LARGE_PAGES`, 2 MiB) with 4 KiB fallback, RAII free | 3 | – | needs `SeLockMemoryPrivilege`, granted only by the user by policy (we never grant it); the result reports the page size used. Measured 2026-11-21 on the target machine: `MEM_LARGE_PAGES` is refused (no privilege held, or not enabled in the token), the probe falls back to 4 KiB pages and reports it. Enabling the privilege needs `OpenProcessToken`, forbidden by the project rule outside `open_target`. **Rejected** (user decision 2026-11-22): 4 KiB pages everywhere, see section 5 |
| WIN-10 | read-only system queries for `fa check`: Toolhelp process-name snapshot (`CreateToolhelp32Snapshot` with `TH32CS_SNAPPROCESS`, `Process32FirstW/NextW`, RAII close; **no `OpenProcess`**), `GetSystemPowerStatus` (AC or battery), `PowerGetActiveScheme` and its friendly name, effective power overlay, and window enumeration for Minecraft detection (`EnumWindows`, `GetWindowThreadProcessId`, `GetWindowTextW`; no process handle) | 4 | – | thin FFI with out-parameters and a snapshot handle to close; a snapshot handle is not a process handle (same information as Task Manager). Nothing is written: power plan and overlay are read, never set. Claude's call, pending: no benchmark, exempt as the other FFI wrappers |
| — | WMI, NVML, scheduled task | 3–7 | `wmi`, `nvml-wrapper`, `schtasks.exe` + XML | safe crates or a process call: **no unsafe written** |

## 5. Rejected, with reasons

| Idea | Why rejected |
|---|---|
| JIT compiler for TOML rules | Rules fire a few times per minute; no speed need. Pure exercise. |
| Large pages (WIN-09, `MEM_LARGE_PAGES`) | Need the lock-memory privilege granted by policy and `OpenProcessToken`, forbidden outside `open_target`; only the probes' cache curve would gain (separating the TLB effect). User decision 2026-11-22: 4 KiB pages everywhere, the fallback stays. |
| MSR reads (APERF/MPERF, thermal), `rdpmc` | Require a kernel driver (forbidden; Vanguard blocks vulnerable monitoring drivers). Replaced by ASM-02 + PDH. |
| WinRing0-style third-party drivers | Same; known blocklisted drivers. |
| `fa-pe` hand-written zero-copy PE parser | `goblin` parses `.pdata`, unwind codes and CodeView safely; `pdb` reads symbols. No measurable gain. |
| Bump arena for incident snapshots | Reusing pre-sized `Vec`s gives zero allocation after warm-up, with no unsafe. |
| Hand-written TDH replacement for every provider | Manual layouts only for hot providers; TDH in a developer tool for the rest. |
| `SeDebugPrivilege` to read kernel addresses | The privilege cheats use to open game processes; anti-cheat gray zone. |
| Function-level symbolization of game modules | Reverse-engineering territory. Module name only. |
| PMC sampling through ETW (`TracePmcCounterListInfo`) | Possible without a driver but unverified with anti-cheats; deferred. |
| `tpause`/`umwait`/`mwaitx` waits (ASM-05) | Target CPU has neither WAITPKG nor MWAITX: could never be proven. |
| "Cycle-exact" asm burn loops | Core cycles have no fixed duration when the clock moves; injectors need durations, which a safe Rust loop to a TSC deadline gives. Replaced by ASM-06 (FMA power load, a real asm candidate). |
| Timer-resolution action on the system | Since Windows 10 2004 the resolution only affects the requesting process; it cannot help the game. |

AVX-512 is available on the target machine: AVX-512 variants are built only as asm candidates or intrinsics behind
runtime dispatch, never as the only path.

## 6. Learning path (in roadmap order)

| Lot | Concepts |
|---|---|
| 0 | `forbid`/`deny` lints, test allocator, profiles |
| 2 | `asm!` operands, clobbers and `options`; `rdtsc` vs `rdtscp`; `lfence`; CPUID; invariant TSC; why QPC (10 MHz) is too coarse |
| 3 | latency vs throughput; dependency chains; prefetchers and Sattolo permutations; NT stores and `sfence`; large pages; clock domains |
| 4 | saturating FMA ports, AVX2 vs AVX-512 (ASM-06); turning low-level measurements into honest verdicts |
| 5 | FFI callbacks and lifetimes of borrowed C pointers; `catch_unwind` at `extern "system"`; `MaybeUninit`; Acquire/Release; false sharing; Miri |
| 6 | `clflushopt`; TLB behavior; COM/D3D11 |
| 7 | seqlock with relaxed atomics and fences (and why volatile would be a data race); why no `&T` into memory another process writes; Windows security descriptors |
| 8 | `pshufb` and shuffle tables; variable-length integer coding; delta-of-delta |
| 10 | FMA; horizontal reductions; why Rust does not vectorize float sums; when FFT beats SIMD |
| 11 | PE format; `.pdata`, `RUNTIME_FUNCTION`, x64 unwind codes; symbol servers; KASLR |
| 12 | native `ntdll` calls; RAII handles; write-ahead journals and flush durability |
| 13 | lane-parallel PRNGs; sorting networks; block bootstrap |
| 16 | layered windows; DWM composition; Direct2D |

## 7. Explanation checkpoint template

For each new block (asm or unsafe), in French, before merge:

1. **Ce que ça fait** — instruction by instruction for asm, operation by operation for unsafe.
2. **Pourquoi c'est correct** — the invariant from the `// SAFETY:` comment, and who guarantees it.
3. **Ce qui le casserait** — the concrete change (a removed fence, a wrong ordering, a missing feature check) and its
   symptom.
4. **Comment on le vérifie** — parity test, Miri, bench, behavioral test.

The user answers "compris" (or asks questions) before the block is merged.
