//! ASM-03: pointer chase (memory latency). Each load needs the result of the previous one, so the time per load is
//! the latency of the level of memory the ring lives in.
//!
//! The ring is built by [`ChaseRing::link`] from a successor table (one random cycle, made by the caller): it checks
//! the table, writes the addresses of the next slots into the caller's memory and then borrows that memory read-only
//! for as long as the ring exists. That is what makes the unsafe loads sound: every word the chase can reach is an
//! address of another slot of the same slice, written by `link`, and nothing can change it afterwards.
//!
//! Verdict (2026-11-21): an unrolled `mov rax, [rax]` candidate was benchmarked against this `read_volatile` chain
//! on rings in L1, L2 and RAM (3 runs, pinned on CPU 4): same speed within the run-to-run spread (under 2 % apart in
//! every case, sign changing between runs), so by the catalog protocol it is a tie and the Rust chain ships. The asm
//! code and the bench stay in git history; the numbers are in the catalog.

use crate::tsc::now;
use std::ptr;

/// Why a ring could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// No slots, or a stride of zero.
    Empty,
    /// The slice is too small for `slots * stride_words` words.
    TooSmall,
    /// The successor table is not one single cycle over all slots.
    NotOneCycle,
}

/// A validated ring of pointers living in a caller-provided slice.
pub struct ChaseRing<'a> {
    /// Keeps the slots borrowed read-only for as long as the ring exists (the soundness argument above).
    _mem: &'a [usize],
    start: usize,
    slots: usize,
    stride_words: usize,
}

impl<'a> ChaseRing<'a> {
    /// Slot `i` lives at word `i * stride_words`; `next[i]` is the slot visited after `i`.
    /// A stride of 8 words (64 bytes) puts one slot per cache line.
    pub fn link(
        mem: &'a mut [usize],
        next: &[u32],
        stride_words: usize,
    ) -> Result<ChaseRing<'a>, LinkError> {
        let slots = next.len();
        if slots == 0 || stride_words == 0 {
            return Err(LinkError::Empty);
        }
        if (slots - 1) * stride_words + 1 > mem.len() {
            return Err(LinkError::TooSmall);
        }
        // Walk the table: it must come back to slot 0 after exactly `slots` steps, never earlier.
        let mut i = 0usize;
        for step in 1..=slots {
            i = *next.get(i).ok_or(LinkError::NotOneCycle)? as usize;
            if i >= slots || (i == 0) != (step == slots) {
                return Err(LinkError::NotOneCycle);
            }
        }
        let base = mem.as_ptr() as usize;
        for (slot, &succ) in next.iter().enumerate() {
            mem[slot * stride_words] = base + succ as usize * stride_words * size_of::<usize>();
        }
        let mem: &'a [usize] = mem;
        Ok(ChaseRing {
            _mem: mem,
            start: base,
            slots,
            stride_words,
        })
    }

    /// Memory spanned by the slots, in bytes.
    pub fn bytes(&self) -> usize {
        self.slots * self.stride_words * size_of::<usize>()
    }

    pub fn slots(&self) -> usize {
        self.slots
    }

    /// Follows the ring for `loads` loads (rounded up to a multiple of 8) and returns the final address plus the
    /// number of loads done. Reference implementation.
    pub fn chase(&self, loads: u64) -> (usize, u64) {
        let n = loads.div_ceil(8).max(1) * 8;
        let mut p = self.start;
        for _ in 0..n {
            // SAFETY: `p` is `start` or a word read from the ring, i.e. the address of a slot of `self.mem`, which is
            // alive, aligned, initialised and immutable for `'a` (see the module doc).
            p = unsafe { ptr::read_volatile(p as *const usize) };
        }
        (p, n)
    }

    /// Timed chase: `(loads done, TSC ticks)`.
    pub fn timed(&self, loads: u64) -> (u64, u64) {
        let t0 = now();
        let (_, done) = self.chase(loads);
        let t1 = now();
        (done, t1.saturating_sub(t0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed single cycle that jumps around: i -> (i * 7 + 1) mod n with n coprime to 7 and `n` prime-ish.
    fn cycle(n: usize) -> Vec<u32> {
        // visiting order 0, k, 2k, ... mod n with gcd(k, n) = 1 is one cycle
        let k = (1..n).rev().find(|k| gcd(*k, n) == 1).unwrap_or(1);
        (0..n).map(|i| ((i + k) % n) as u32).collect()
    }

    fn gcd(a: usize, b: usize) -> usize {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }

    #[test]
    fn the_chase_follows_the_links_for_many_ring_sizes() {
        for n in [1usize, 2, 3, 8, 100, 1000] {
            let mut mem = vec![0usize; n * 8];
            let ring = ChaseRing::link(&mut mem, &cycle(n), 8).unwrap();
            // after exactly n * 8 loads the walk has done 8 laps: it is back on the start
            assert_eq!(
                ring.chase(n as u64 * 8),
                (ring.start, n as u64 * 8),
                "n={n}"
            );
        }
    }

    #[test]
    fn a_full_lap_returns_to_the_start() {
        let n = 64;
        let mut mem = vec![0usize; n * 8];
        let ring = ChaseRing::link(&mut mem, &cycle(n), 8).unwrap();
        let start = ring.start;
        assert_eq!(ring.chase(n as u64 * 8).0, start); // n * 8 loads = 8 laps
        assert_ne!(ring.chase(8 * 7).0, start); // 56 loads is not a multiple of 64
    }

    #[test]
    fn a_table_with_a_short_cycle_is_refused() {
        let mut mem = vec![0usize; 64];
        // 0 -> 1 -> 0 and 2 -> 3 -> 2: two cycles
        assert_eq!(
            ChaseRing::link(&mut mem, &[1, 0, 3, 2], 1).err(),
            Some(LinkError::NotOneCycle)
        );
        // a fixed point
        assert_eq!(
            ChaseRing::link(&mut mem, &[0, 1], 1).err(),
            Some(LinkError::NotOneCycle)
        );
    }

    #[test]
    fn out_of_range_tables_and_small_slices_are_refused() {
        let mut mem = vec![0usize; 16];
        assert_eq!(
            ChaseRing::link(&mut mem, &[1, 7, 0], 1).err(),
            Some(LinkError::NotOneCycle)
        );
        assert_eq!(
            ChaseRing::link(&mut mem, &[1, 0], 16).err(),
            Some(LinkError::TooSmall)
        );
        assert_eq!(
            ChaseRing::link(&mut mem, &[], 1).err(),
            Some(LinkError::Empty)
        );
        assert_eq!(
            ChaseRing::link(&mut mem, &[0], 0).err(),
            Some(LinkError::Empty)
        );
    }

    #[test]
    fn a_single_slot_ring_points_to_itself() {
        let mut mem = vec![0usize; 1];
        let ring = ChaseRing::link(&mut mem, &[0], 1).unwrap();
        assert_eq!(ring.chase(8).0, ring.start);
    }

    #[test]
    fn timed_reports_the_loads_done() {
        let mut mem = vec![0usize; 64];
        let ring = ChaseRing::link(&mut mem, &cycle(8), 8).unwrap();
        assert_eq!(ring.timed(100).0, 104);
    }
}
