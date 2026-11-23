# Tasks

Gate = `powershell -File scripts/gate.ps1`. Each group ends with it, green. Dev commands run from an elevated terminal.

## 1. Catalog, gate and layout

- [x] 1.1 Fill WIN-05 and ASM-07 and add WIN-12 (Lab system calls) in `docs/asm-unsafe-catalog.md` before any code; verify the lines are present with "pending".
- [x] 1.2 Create the module directory `crates/fa/src/lab/mod.rs` and change the gate trigger in `scripts/gate.ps1` to `crates/fa/src/lab/score.rs`; verify the gate runs in lot 6 without a `lab score` step.
- [x] 1.3 Run the gate and verify it is green.

## 2. Render loop (`fa-win` WIN-05, `fa`)

- [ ] 2.1 Implement window, D3D11 device and `FLIP_DISCARD` swap chain with a safe API and `// SAFETY:` comments; verify it opens and renders the animated scene at 60 FPS and records the adapter name.
- [ ] 2.2 Implement pacing (swap-chain present interval for vsync; coarse timer sleep then TSC spin for 144 FPS; uncapped); verify the frame-time p99 stays within the baseline jitter budget over 60 s at 60 and 144 FPS (budget `(calibrate)`: measured on the idle loop in 2.3, then frozen and logged in `docs/decisions.md` section 5).
- [ ] 2.3 Record the loop's baseline jitter in the journal header; verify with a test on the header writer and a real run.
- [ ] 2.4 Explanation checkpoint (French) for the D3D11/COM unsafe blocks (WIN-05); verify the user answered "compris".
- [ ] 2.5 Run the gate and verify it is green.

## 3. Journal, scenarios, guards

- [ ] 3.1 Implement the JSONL journal (header, start and end lines, TSC ticks); verify with a golden-file test and a killed-run test showing a start line without an end line.
- [ ] 3.2 Implement the scenario parser (`deny_unknown_fields`, cap validation); verify with tests for a valid file, an unknown field and an injection over 30 s.
- [ ] 3.3 Implement the guards: game refusal via `fa_probe::plan::running_games`, caps, Ctrl+C cleanup; verify with a fake process list for `Overwatch.exe` (refusal names it), a mutation check on each cap (`mutation-verified-testing`) and a manual Ctrl+C run leaving no temporary file.
- [ ] 3.4 Run the gate and verify it is green.

## 4. `frame-stall` and capture check

- [ ] 4.1 Implement `frame-stall` (burn N ms before `Present` at frame k); verify the journal holds the exact start and end ticks.
- [ ] 4.2 Run `fa lab run` with the lot 5 capture from an elevated terminal; verify the captured frame time at frame k equals the injected stall within the frozen baseline jitter budget (see 2.2).
- [ ] 4.3 Replay the lot 5 PresentMon parity on the Lab loop; verify the result is recorded and matches the lot 5 criterion.
- [ ] 4.4 Run the gate and verify it is green.

## 5. CPU and memory injectors

- [ ] 5.1 Implement `cpu-burn` (TSC deadline, optional `fa_asm::fma::burn`) with CPUs from `fa_probe::plan::load_cpus`; verify with synthetic-topology tests that no SMT sibling of a measured CPU is loaded, and a run showing the stall.
- [ ] 5.2 Write the ASM-07 intrinsics reference and the behavioural latency-jump metric; write asm only if the reference cannot produce the behaviour; verify the metric, record the verdict (win under 5 % is a tie) in the catalog.
- [ ] 5.3 If an asm block exists: explanation checkpoint (French) for ASM-07; verify the user answered "compris". Skipped if intrinsics ship.
- [ ] 5.4 Implement `cache-thrash` (buffers well above 12 MiB), `tlb-thrash` (4 KiB pages), `mem-bandwidth`; verify each against the capture or a probe showing a measurable effect.
- [ ] 5.5 Implement `page-fault-storm` (WIN-12); verify page-fault counts rise in the capture and cleanup restores the working set.
- [ ] 5.6 Explanation checkpoint (French) for each new unsafe block of WIN-12; verify the user answered "compris".
- [ ] 5.7 Run the gate and verify it is green.

## 6. Disk, timer and priority injectors

- [ ] 6.1 Implement `disk-burst` (unbuffered write-through, 1 GB cap, 10 GB free check, temporary file deleted); verify with a test of the free-space refusal and that the file is gone after a run and after Ctrl+C.
- [ ] 6.2 Implement `timer-resolution` (own process only) and `priority-contention` (own threads); verify the effect appears in the loop pacing and that no other process is touched.
- [ ] 6.3 Run the gate and verify it is green.

## 7. Scenarios and demo

- [ ] 7.1 Write 10 single-cause and 3 two-cause scenarios in `lab/scenarios/*.toml`; verify each parses and each run produces a journal with the expected causes.
- [ ] 7.2 Ask the user to record the demo video (inject a cause, see the stutter); verify the user confirmed it.
- [ ] 7.3 Log measured loop jitter and any recalibrated value in `docs/decisions.md` section 5; verify the entries exist.
- [ ] 7.4 Run `openspec validate stutter-lab --strict` and the gate; verify both are green.

## Workflow follow-up

- Archive the change after the user validates the demo and the branch is merged (`openspec-archive-change`).
