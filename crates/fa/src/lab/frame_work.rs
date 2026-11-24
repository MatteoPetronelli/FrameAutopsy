//! The Lab loop's per-frame work, so that injected load can actually hurt it like it hurts a game.
//!
//! Without it the frame is one GPU clear and nothing a CPU, cache or memory injector does reaches the frame times.
//! Each frame walks a 2 MiB pointer-chase ring once (above the 1.25 MiB L2 so it lives in the shared last-level cache,
//! below the 3 to 4 MiB effective capacity measured here; a 3 MiB ring was mostly RAM already, 5 ms a frame: from the
//! cache this costs about half a millisecond, from RAM several) and then runs a
//! fixed amount of dependent integer work calibrated to about 1 ms (sensitive to frequency and to a busy SMT sibling).

use fa_asm::chase::ChaseRing;
use fa_probe::mem::{sattolo, LINE_BYTES};
use fa_win::clock::Clock;
use fa_win::mem::PageBuf;
use std::hint::black_box;
use std::io;

const RING_BYTES: usize = 2 << 20;
/// Target length of the compute part, in nanoseconds.
const COMPUTE_NS: u64 = 1_000_000;

pub struct Work {
    mem: PageBuf,
    next: Vec<u32>,
    /// Loads per frame: every line of the ring once.
    pub slots: u64,
    /// Iterations of [`compute`] per frame.
    pub iters: u64,
}

/// `iters` rounds of a dependent multiply/xor chain: no cache or memory involved, one chain so no ILP to hide a busy
/// sibling thread.
pub fn compute(iters: u64) -> u64 {
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..iters {
        x = black_box((x ^ (x >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    }
    x
}

impl Work {
    pub fn new(clock: &Clock) -> io::Result<Work> {
        let mem = PageBuf::alloc(RING_BYTES, false)?;
        let slots = RING_BYTES / LINE_BYTES;
        let next = sattolo(slots, 0xF4A3_5EED);
        // Calibration: the second of two timings, the first one warms the code and the clock.
        let mut per_iter_ns = 1.0;
        for _ in 0..2 {
            let t0 = clock.now();
            black_box(compute(2_000_000));
            per_iter_ns = clock.ticks_to_ns(clock.now() - t0) as f64 / 2_000_000.0;
        }
        Ok(Work {
            mem,
            next,
            slots: slots as u64,
            iters: (COMPUTE_NS as f64 / per_iter_ns.max(0.01)) as u64,
        })
    }

    /// The ring over this work's buffer (one cycle through every line).
    pub fn ring(&mut self) -> io::Result<ChaseRing<'_>> {
        ChaseRing::link(self.mem.as_mut_slice(), &self.next, LINE_BYTES / 8)
            .map_err(|e| io::Error::other(format!("{e:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_is_deterministic_and_depends_on_the_iteration_count() {
        assert_eq!(compute(1000), compute(1000));
        assert_ne!(compute(1000), compute(1001));
    }

    #[test]
    fn the_calibration_lands_near_one_millisecond() {
        let clock = Clock::new();
        let mut w = Work::new(&clock).unwrap();
        assert_eq!(w.slots, (RING_BYTES / LINE_BYTES) as u64);
        // Best of several: a busy machine only makes a run slower.
        let best = (0..5)
            .map(|_| {
                let t0 = clock.now();
                black_box(compute(w.iters));
                clock.ticks_to_ns(clock.now() - t0)
            })
            .min()
            .unwrap();
        assert!(
            (600_000..2_000_000).contains(&best),
            "about 1 ms expected, got {best} ns"
        );
        let ring = w.ring().unwrap();
        assert_eq!(ring.slots() as u64, w.slots);
    }
}
