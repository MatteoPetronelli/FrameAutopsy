//! ASM-02: effective frequency probe, a chain of dependent `add` (asm-mandatory).
//!
//! Each `add rax, 1` needs the result of the previous one and has a latency of 1 cycle, so a chain of `n` adds takes
//! `n` core cycles. Elapsed TSC time for the chain then gives `n / seconds` = the core clock actually running.
//! The compiler would fold `n` repeated `x + 1` into one add, which is why this is written in asm.
//!
//! Reference ([`reference_chain`]): a chain of dependent squarings (`x = x * x`) in Rust, one `imul` per step, 3 cycles
//! of latency, not a closed form, so the compiler cannot fold it. Two earlier forms failed and are not used: `x * 3`
//! was folded by LLVM (12 cycles per TSC tick, impossible) and `x * (x | 3)` costs a non-integer 4.25 cycles per step
//! on this CPU (the `mov` between steps is not always eliminated), a 5.9 % gap. `FA_FORCE_SCALAR=1` selects it.

use crate::tsc::{force_scalar, now};
use core::arch::asm;
use std::hint::black_box;

/// Adds executed per loop iteration of [`add_chain`].
pub const ADDS_PER_ITER: u64 = 16;

/// Runs `iters * 16` dependent `add` instructions and returns their count.
pub fn add_chain(iters: u64) -> u64 {
    let mut acc: u64 = 0;
    // SAFETY: only registers are touched (`nomem`, `nostack`); `acc` and `iters` are in-out registers; the loop ends
    // when the counter reaches zero (a zero start would wrap, so callers pass `iters >= 1`, guarded below). Flags are
    // clobbered by `add`/`dec`, which is the default (no `preserves_flags`).
    unsafe {
        asm!(
            "2:",
            "add {acc}, 1", "add {acc}, 1", "add {acc}, 1", "add {acc}, 1",
            "add {acc}, 1", "add {acc}, 1", "add {acc}, 1", "add {acc}, 1",
            "add {acc}, 1", "add {acc}, 1", "add {acc}, 1", "add {acc}, 1",
            "add {acc}, 1", "add {acc}, 1", "add {acc}, 1", "add {acc}, 1",
            "dec {n}",
            "jnz 2b",
            acc = inout(reg) acc,
            n = inout(reg) iters.max(1) => _,
            options(nomem, nostack),
        );
    }
    acc
}

/// Reference: `steps` dependent squarings (one `imul`, 3 cycles each) on the cores checked by the parity test.
#[inline(never)]
pub fn reference_chain(steps: u64) -> u64 {
    let mut x = black_box(3u64);
    for _ in 0..steps {
        x = x.wrapping_mul(x);
    }
    black_box(x)
}

/// Core cycles per step of [`reference_chain`] (64-bit `imul`); verified by the parity test.
pub const REFERENCE_CYCLES_PER_STEP: u64 = 3;

/// One timed sample: `cycles` core cycles executed between two TSC reads that are `ticks` apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub cycles: u64,
    pub ticks: u64,
}

/// Times a chain of about `cycles` core cycles (rounded to whole iterations). Uses the asm chain, or the reference
/// when `FA_FORCE_SCALAR=1`.
pub fn sample(cycles: u64) -> Sample {
    if force_scalar() {
        sample_reference(cycles)
    } else {
        sample_asm(cycles)
    }
}

/// The asm chain, whatever `FA_FORCE_SCALAR` says (used by the parity test and the bench).
pub fn sample_asm(cycles: u64) -> Sample {
    let iters = (cycles / ADDS_PER_ITER).max(1);
    let t0 = now();
    let n = add_chain(iters);
    let t1 = now();
    Sample {
        cycles: n,
        ticks: t1.saturating_sub(t0),
    }
}

/// The reference chain, whatever `FA_FORCE_SCALAR` says.
pub fn sample_reference(cycles: u64) -> Sample {
    let steps = (cycles / REFERENCE_CYCLES_PER_STEP).max(1);
    let t0 = now();
    black_box(reference_chain(steps));
    let t1 = now();
    Sample {
        cycles: steps * REFERENCE_CYCLES_PER_STEP,
        ticks: t1.saturating_sub(t0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chain_counts_every_add() {
        assert_eq!(add_chain(1), 16);
        assert_eq!(add_chain(1000), 16_000);
        assert_eq!(add_chain(0), 16, "zero iterations is clamped to one");
    }

    /// Best cycles-per-TSC-tick of each kernel over many tries, taken in alternation so both see the same clock
    /// conditions; the best of many ignores preemption and ramp-up. Core clock / TSC clock.
    fn best_ratios() -> (f64, f64) {
        let ratio = |s: Sample| s.cycles as f64 / s.ticks as f64;
        let (mut a, mut r) = (0.0f64, 0.0f64);
        for _ in 0..1_000 {
            a = a.max(ratio(sample_asm(200_000)));
            r = r.max(ratio(sample_reference(200_000)));
        }
        (a, r)
    }

    /// Parity (catalog ASM-02): both chains measure the same clock; the 3 % limit is the one of the spec.
    #[test]
    fn asm_and_reference_chains_report_the_same_clock_within_three_percent() {
        let _ = best_ratios(); // warm up so the core is at its running frequency
        let (a, r) = best_ratios();
        let gap = (a - r).abs() / a;
        assert!(
            gap < 0.03,
            "asm {a:.4} vs reference {r:.4} cycles/tick, gap {:.2} %",
            gap * 100.0
        );
    }

    #[test]
    fn a_core_clock_between_one_and_six_ghz_is_reported_against_the_tsc() {
        // Target machine: TSC 2.688 GHz; any x86 desktop core runs well within 1..6 GHz.
        let ratio = best_ratios().0;
        let ghz = ratio * 2.688;
        assert!((1.0..6.0).contains(&ghz), "{ghz} GHz");
    }
}
