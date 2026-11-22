//! Memory bandwidth probe: one pinned thread per physical core, each streaming through its own slice of one big buffer.
//! The kernels live in `fa_asm::bw`; this module only organises threads and aggregates.

use fa_asm::bw;
pub use fa_asm::bw::Kernel;
use fa_win::mem::PageBuf;
use fa_win::topology::LogicalCpu;
use fa_win::{clock::Clock, thread::pin_current_thread};
use std::io;
use std::sync::Barrier;

/// The fastest kernel this CPU can run.
pub fn best_kernel() -> Kernel {
    bw::best_intrinsics()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Read,
    Write,
    /// Counted like STREAM: every copied byte is one byte read plus one byte written.
    Copy,
}

/// One logical CPU per physical core (the lowest index of each core), in core order. The first processor group only,
/// like the pinning of `fa-win::thread`.
pub fn physical_core_cpus(cpus: &[LogicalCpu]) -> Vec<u32> {
    let mut by_core: Vec<(u32, u32)> = Vec::new(); // (core, lowest logical index)
    for c in cpus.iter().filter(|c| c.group == 0) {
        match by_core.iter_mut().find(|(core, _)| *core == c.core) {
            Some(e) => e.1 = e.1.min(c.index),
            None => by_core.push((c.core, c.index)),
        }
    }
    by_core.sort_unstable();
    by_core.into_iter().map(|(_, i)| i).collect()
}

/// Aggregate bandwidth in GB/s (1e9 bytes per second): all bytes moved by all threads over the slowest thread's time,
/// since the threads run at the same time and the run ends when the last one ends.
pub fn aggregate_gbs(parts: &[(u64, u64)], clock: &Clock) -> Option<f64> {
    let bytes: u64 = parts.iter().map(|p| p.0).sum();
    let ticks = parts.iter().map(|p| p.1).max()?;
    let ns = clock.ticks_to_ns(ticks);
    (ns > 0).then(|| bytes as f64 / ns as f64) // bytes per ns = GB/s
}

/// Streams for `passes` passes on `cpus.len()` threads, each owning `bytes_per_thread` bytes of one shared allocation.
pub fn run(
    cpus: &[u32],
    mode: Mode,
    kernel: Kernel,
    bytes_per_thread: usize,
    passes: usize,
) -> io::Result<f64> {
    let clock = Clock::new();
    let words = bytes_per_thread / size_of::<u64>();
    let mut buf = PageBuf::alloc(words * size_of::<u64>() * cpus.len(), false)?;
    let barrier = Barrier::new(cpus.len());
    let parts: Vec<io::Result<(u64, u64)>> = std::thread::scope(|s| {
        let handles: Vec<_> = buf
            .as_mut_u64()
            .chunks_mut(words)
            .zip(cpus)
            .map(|(chunk, &cpu)| {
                let (barrier, clock) = (&barrier, &clock);
                s.spawn(move || -> io::Result<(u64, u64)> {
                    let _pin = pin_current_thread(cpu)?;
                    let chunk = &mut chunk[..words];
                    let one_pass = |chunk: &mut [u64]| -> u64 {
                        match mode {
                            Mode::Read => bw::read_sum(chunk, kernel),
                            Mode::Write => {
                                bw::fill(chunk, 0x5A5A, kernel);
                                0
                            }
                            Mode::Copy => {
                                let (a, b) = chunk.split_at_mut(words / 2);
                                bw::copy(b, a, kernel);
                                0
                            }
                        }
                    };
                    let moved = match mode {
                        Mode::Copy => (words / 2 * size_of::<u64>() * 2) as u64,
                        _ => (words * size_of::<u64>()) as u64,
                    };
                    let mut sink = one_pass(chunk); // untimed: first touch maps the pages
                    barrier.wait();
                    let t0 = clock.now();
                    for _ in 0..passes {
                        sink = sink.wrapping_add(one_pass(chunk));
                    }
                    let t1 = clock.now();
                    std::hint::black_box(sink);
                    Ok((moved * passes as u64, t1.saturating_sub(t0)))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(io::Error::other("probe thread panicked")))
            })
            .collect()
    });
    let parts: Vec<(u64, u64)> = parts.into_iter().collect::<io::Result<_>>()?;
    aggregate_gbs(&parts, &clock).ok_or_else(|| io::Error::other("no time elapsed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu(index: u32, core: u32, group: u16) -> LogicalCpu {
        LogicalCpu {
            index,
            cpu_set_id: index,
            core,
            llc: 0,
            efficiency_class: 0,
            numa_node: 0,
            group,
        }
    }

    #[test]
    fn one_logical_cpu_per_core_the_lowest_one() {
        // 2 cores with SMT siblings listed out of order, plus a CPU of another processor group
        let v = [
            cpu(3, 1, 0),
            cpu(0, 0, 0),
            cpu(1, 0, 0),
            cpu(2, 1, 0),
            cpu(9, 7, 1),
        ];
        assert_eq!(physical_core_cpus(&v), vec![0, 2]);
        assert!(physical_core_cpus(&[]).is_empty());
    }

    #[test]
    fn aggregate_uses_the_slowest_thread() {
        let clock = Clock::new();
        let one_s = clock.hz().get(); // ticks in one second
                                      // two threads moved 1e9 bytes each; the slower took 2 s: 2e9 bytes / 2 s = 1 GB/s
        let g = aggregate_gbs(
            &[(1_000_000_000, one_s), (1_000_000_000, 2 * one_s)],
            &clock,
        )
        .unwrap();
        assert!((g - 1.0).abs() < 1e-6, "{g}");
        assert_eq!(aggregate_gbs(&[], &clock), None);
        assert_eq!(aggregate_gbs(&[(5, 0)], &clock), None);
    }

    #[test]
    fn a_real_small_run_reports_a_plausible_rate() {
        // best of several runs: other tests share CPU 2 and a preempted run reads far too low
        let g = (0..5)
            .map(|_| run(&[2], Mode::Read, Kernel::Avx2, 4 << 20, 3).unwrap())
            .fold(0.0, f64::max);
        assert!((0.5..500.0).contains(&g), "{g} GB/s");
    }
}
