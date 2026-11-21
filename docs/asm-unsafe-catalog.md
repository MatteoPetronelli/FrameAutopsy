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
(F/VL/BW/DQ — confirm with CPUID at lot 2); **no WAITPKG** (to confirm), **no MWAITX** (AMD only). A module whose only
win would be on hardware absent from this machine is not built (D1: it could never be proven).

## 4. Ledger

### `fa-asm`

| Id | Module | Lot | Tier | Reference | Metric | Parity test | Fallback / CPU feature | What would break it | Result | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| ASM-01 | `tsc::now`, `now_serialized` (`lfence; rdtsc; lfence`, `rdtscp; lfence`) | 2 | asm-candidate | `_rdtsc` / `__rdtscp` + `_mm_lfence` | read overhead (cycles) and jitter (stdev) | same tick within overhead window; monotonic | QPC if no invariant TSC (CPUID 0x80000007) | missing `lfence` lets the CPU reorder the read around the timed code | – | pending |
| ASM-02 | frequency probe (dependent `add` chain) | 3 | asm-mandatory | PDH `% Processor Performance` × nominal | agreement ±3 % at steady state + information PDH lacks (instant per-core value) | Rust `black_box` chain gives the same GHz within 3 % | PDH counter | compiler folding the chain (why asm); preemption mid-sample (rejected by median rule) | – | pending: accuracy at lot 3, keep/remove decided at lot 10 |
| ASM-03 | pointer chase (memory latency) | 3 | asm-candidate | Rust `read_volatile` chain | coefficient of variation of ns/load | same ns/load ±2 % | Rust chain | prefetchers if the permutation is not a single random cycle | – | pending |
| ASM-04 | bandwidth (AVX2 loads, `vmovntdq` NT stores, copy) | 3 | asm-candidate | intrinsics (`_mm256_stream_si256`…) | GB/s | same bytes written/read, checksum | SSE2 intrinsics → scalar | missing `sfence` after NT stores | – | pending |
| ASM-05 | `tpause`/`umwait` (WAITPKG), `monitorx`/`mwaitx` (AMD) | – | – | `pause` spin | – | – | – | – | target CPU has neither feature | **dropped** (cannot be proven on the target machine) |
| ASM-06 | FMA power load (health-check soak, optional Lab load): independent FMA chains saturating both FMA ports, AVX2 and AVX-512 variants | 4 (reused in 6) | asm-candidate | intrinsics (`_mm256_fmadd_ps`, `_mm512_fmadd_ps`) | FLOP per TSC tick (closeness to 2 FMA/cycle) and resulting package heat (frequency drop curve) | same FLOP count and final accumulator values | intrinsics; AVX2 → AVX-512 by dispatch | too few independent chains → latency-bound, not port-bound | – | pending |
| ASM-07 | `clflushopt` sweeps (Lab `cache-thrash`) | 6 | intrinsics (asm only if the intrinsic is unstable) | `_mm_clflushopt` / `_mm_clflush` | n/a (behavioral: measured latency jump after flush) | n/a | `clflush`; CPUID 7.0:EBX[23] | missing fence before timing | – | pending |
| SIMD-01 | Stream VByte encode/decode (`pshufb`) | 8 | intrinsics + asm-candidate | scalar varint; and "delta + zstd" for the stage itself | decode and encode GB/s; ratio after zstd | round-trip equality on random and real timestamps | scalar; SSSE3/AVX2 | wrong shuffle table entry corrupts silently (round-trip test catches it) | – | pending |
| SIMD-02 | lagged cross-correlation (FMA) | 10 | intrinsics + asm-candidate | scalar Rust; FFT with `rustfft` | time for 1,000 series × 10,000 bins × 401 lags | results within 1e-4 relative of scalar f64 | scalar; FMA + AVX2 | accumulation order changes results (tolerance-based parity) | – | pending |
| SIMD-03 | f32 selection / sort network | 13 | intrinsics + asm-candidate | `select_nth_unstable_by` | ns per element at 36k–100k frames | identical selected value | scalar; AVX2 | NaN handling (frametimes validated non-NaN upstream) | – | pending |
| SIMD-04 | xoshiro256++ ×4 (AVX2), ×8 (AVX-512) | 13 | intrinsics | scalar xoshiro256++ (e.g. `rand_xoshiro`) | numbers per second | exact stream equality per lane for the same seeds | scalar; AVX2 / AVX-512F | lane seeding mistakes break reproducibility (exact parity catches it) | – | pending |

### `fa-sync`

| Id | Module | Lot | Tier | Reference | Metric | Verification | What would break it | Result | Verdict |
|---|---|---|---|---|---|---|---|---|---|
| SYNC-00 | counting global allocator (`test-support` feature, `unsafe impl GlobalAlloc`) | 0 | unsafe (test only) | none (needed by the zero-allocation tests) | n/a | self-test: a known allocation is counted (per-thread `Cell` counter) | n/a | counting in the allocator itself allocating | – | needed |
| SYNC-01 | SPSC ring (padding, Acquire/Release, `MaybeUninit`) | 5 | unsafe-sync | `rtrb` | throughput and latency, 32-byte records | Miri two-thread stress; criterion | Relaxed where Acquire is needed → stale reads; missing padding → false sharing | – | pending |
| SYNC-02 | cross-process seqlock over a fixed-layout mapping (relaxed atomic 64-bit words + Acquire/Release fences; no volatile) | 7 | unsafe-sync | none exists for cross-process fixed layouts (justification) | correctness (no torn read), reader retry rate | Miri in-process; cross-process test with checksums | forming `&T` into shared memory (UB); volatile instead of atomics (data race); missing fences | – | pending |
| SYNC-03 | shared-region reader for hsperfdata (relaxed atomic copies) | 15 | unsafe-sync | plain file reads (`std::fs`) at 10 Hz | sampling cost; consistency | comparison test against file reads | same as SYNC-02 | – | pending (likely loses → file reads) |

### `fa-win` (FFI, exempt from benchmarks unless a wrapper crate qualifies)

| Id | Module | Lot | Reference crate considered | Why hand-written (or not) |
|---|---|---|---|---|
| WIN-01 | ETW session and consumer | 5 | `ferrisetw` | zero allocation per event, raw TSC timestamps (`ClientContext = 3` + `PROCESS_TRACE_MODE_RAW_TIMESTAMP`), stack-walk setup. Single rule: the hand-written consumer stays for hot providers only if it shows 0 allocations per event **and** throughput ≥ `ferrisetw`; otherwise `ferrisetw` is used. Non-hot providers use `ferrisetw` if it is adopted at all. |
| WIN-02 | process, priority, EcoQoS, CPU Sets, power overlay, `ntdll` native calls | 12 | `windows` / `windows-sys` bindings only | thin RAII wrappers; no wrapper crate covers the set |
| WIN-03 | topology (`GetSystemCpuSetInformation`, `GetLogicalProcessorInformationEx`) | 2 | – | variable-length records need careful pointer walking |
| WIN-04 | shared memory and named pipes with DACLs (SDDL) | 7 | – | security descriptors are the trust boundary |
| WIN-05 | D3D11 render loop for the Lab | 6 | `windows` crate COM | COM calls are unsafe by nature |
| WIN-06 | Direct2D + DirectComposition overlay window | 16 | `windows` crate | same |
| WIN-07 | high-resolution waitable timers, thread pinning | 3 | – | small |
| — | WMI, NVML, PDH, scheduled task | 3–7 | `wmi`, `nvml-wrapper`, PDH via `windows`, `schtasks.exe` + XML | safe crates or a process call: **no unsafe written** |

## 5. Rejected, with reasons

| Idea | Why rejected |
|---|---|
| JIT compiler for TOML rules | Rules fire a few times per minute; no speed need. Pure exercise. |
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
