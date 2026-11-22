//! ASM-06: FMA power load. Eight independent accumulator chains of `acc = acc * M + C` keep both FMA ports busy
//! (4 cycles of latency times 2 ports needs at least 8 chains), which is the closest a user-mode program gets to a
//! power virus. `fa check` runs it on every physical core but one to make a laptop show its throttling; the Lab reuses it.
//!
//! Every variant does exactly the same arithmetic on the same 8 x 16 `f32` accumulators, and a fused multiply-add rounds
//! once, so the result is bit-identical across AVX-512, AVX2 and the scalar `f32::mul_add` fallback: that is the parity
//! test. `FA_FORCE_SCALAR=1` selects the scalar variant.
//!
//! Variants: [`Isa::Avx512`] (8 `zmm` chains), [`Isa::Avx2`] (two passes of 8 `ymm` chains over the two 8-lane halves,
//! so it never spills), [`Isa::Scalar`]. All are intrinsics or plain Rust.
//!
//! Verdict (2026-11-22): a hand-written `asm!` candidate (`vfmadd213ps` x 8 in a counted loop) was benchmarked against
//! these intrinsics, 3 runs pinned on CPU 4: AVX-512 +1.5 % and AVX2 -0.9 % median, inside the run-to-run spread, so by
//! the catalog protocol it is a tie and the intrinsics ship. The asm code and its bench stay in git history; the numbers
//! are in the catalog. Both ISAs run at 28 to 31 FLOP per core cycle: the loop is port-bound, which is the point of it.

use crate::tsc::{force_scalar, now};
use core::arch::x86_64::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Accumulator chains and `f32` lanes per chain.
pub const CHAINS: usize = 8;
pub const LANES: usize = 16;

/// Multiplier just above 1 and a tiny addend: the values drift upward slowly, never overflow nor go denormal within
/// the loads we run, and stay distinct per lane, so a wrong lane shows in the parity test.
const M: f32 = 1.000_000_1;
const C: f32 = 1.0e-7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Isa {
    Avx512,
    Avx2,
    Scalar,
}

/// The accumulators.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FmaState {
    pub acc: [[f32; LANES]; CHAINS],
}

impl FmaState {
    pub fn new() -> FmaState {
        let mut acc = [[0.0f32; LANES]; CHAINS];
        for (c, row) in acc.iter_mut().enumerate() {
            for (l, v) in row.iter_mut().enumerate() {
                *v = 1.0 + (c * LANES + l) as f32 * 1.0e-3;
            }
        }
        FmaState { acc }
    }

    /// Bit patterns, for exact comparisons.
    pub fn bits(&self) -> Vec<u32> {
        self.acc.iter().flatten().map(|v| v.to_bits()).collect()
    }
}

impl Default for FmaState {
    fn default() -> Self {
        Self::new()
    }
}

/// Floating-point operations done by `iters` iterations (one FMA counts as two).
pub fn flops(iters: u64) -> u64 {
    iters * (CHAINS * LANES * 2) as u64
}

fn has_avx2_fma() -> bool {
    std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma")
}

fn has_avx512() -> bool {
    std::arch::is_x86_feature_detected!("avx512f")
}

/// The ISA that will really run when `isa` is asked for on this CPU.
pub fn effective(isa: Isa) -> Isa {
    if force_scalar() {
        return Isa::Scalar;
    }
    match isa {
        Isa::Avx512 if has_avx512() => Isa::Avx512,
        Isa::Avx512 | Isa::Avx2 if has_avx2_fma() => Isa::Avx2,
        _ => Isa::Scalar,
    }
}

/// The widest ISA of this CPU.
pub fn best_isa() -> Isa {
    effective(Isa::Avx512)
}

/// Runs `iters` iterations on `state`.
pub fn run(state: &mut FmaState, isa: Isa, iters: u64) {
    if iters == 0 {
        return;
    }
    match effective(isa) {
        // SAFETY: AVX-512F availability was checked by `effective`.
        Isa::Avx512 => unsafe { avx512_intrinsics(state, iters) },
        // SAFETY: AVX2 and FMA availability was checked by `effective`.
        Isa::Avx2 => unsafe { avx2_intrinsics(state, iters) },
        Isa::Scalar => scalar(state, iters),
    }
}

fn scalar(state: &mut FmaState, iters: u64) {
    for _ in 0..iters {
        for v in state.acc.iter_mut().flatten() {
            *v = v.mul_add(M, C);
        }
    }
}

#[target_feature(enable = "avx2,fma")]
unsafe fn avx2_intrinsics(state: &mut FmaState, iters: u64) {
    let (m, c) = (_mm256_set1_ps(M), _mm256_set1_ps(C));
    for half in 0..2 {
        let base = half * 8;
        let mut a = [_mm256_setzero_ps(); CHAINS];
        for (row, r) in state.acc.iter().zip(a.iter_mut()) {
            // SAFETY: `row[base..base + 8]` is 8 valid `f32`; the load is unaligned.
            *r = unsafe { _mm256_loadu_ps(row[base..base + 8].as_ptr()) };
        }
        for _ in 0..iters {
            for r in a.iter_mut() {
                *r = _mm256_fmadd_ps(*r, m, c);
            }
        }
        for (row, r) in state.acc.iter_mut().zip(a) {
            // SAFETY: `row[base..base + 8]` is 8 valid `f32`; the store is unaligned.
            unsafe { _mm256_storeu_ps(row[base..base + 8].as_mut_ptr(), r) };
        }
    }
}

#[target_feature(enable = "avx512f")]
unsafe fn avx512_intrinsics(state: &mut FmaState, iters: u64) {
    let (m, c) = (_mm512_set1_ps(M), _mm512_set1_ps(C));
    let mut a = [_mm512_setzero_ps(); CHAINS];
    for (row, r) in state.acc.iter().zip(a.iter_mut()) {
        // SAFETY: `row` is 16 valid `f32`; the load is unaligned.
        *r = unsafe { _mm512_loadu_ps(row.as_ptr()) };
    }
    for _ in 0..iters {
        for r in a.iter_mut() {
            *r = _mm512_fmadd_ps(*r, m, c);
        }
    }
    for (row, r) in state.acc.iter_mut().zip(a) {
        // SAFETY: `row` is 16 valid `f32`; the store is unaligned.
        unsafe { _mm512_storeu_ps(row.as_mut_ptr(), r) };
    }
}

/// Work done by [`burn`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Burn {
    pub flops: u64,
    pub ticks: u64,
}

/// Runs blocks of `block_iters` iterations until `stop` is set (checked between blocks, in safe Rust), and returns the
/// FLOP count and the TSC ticks spent. Aim for blocks of about 1 ms so that a stop request is honoured within a few ms.
pub fn burn(isa: Isa, stop: &AtomicBool, block_iters: u64) -> Burn {
    let mut state = FmaState::new();
    let mut total = Burn::default();
    while !stop.load(Ordering::Relaxed) {
        state = FmaState::new(); // values grow by about 1e-7 per iteration: restart so they never reach infinity (a constant operand would also draw less power)
        let t0 = now();
        run(&mut state, isa, block_iters);
        total.ticks += now().saturating_sub(t0);
        total.flops += flops(block_iters);
    }
    std::hint::black_box(&state);
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Isa; 3] = [Isa::Avx512, Isa::Avx2, Isa::Scalar];

    #[test]
    fn every_variant_ends_on_the_same_bits() {
        for iters in [1u64, 2, 7, 1000] {
            let mut want = FmaState::new();
            scalar(&mut want, iters);
            for isa in ALL {
                let mut s = FmaState::new();
                run(&mut s, isa, iters);
                assert_eq!(s.bits(), want.bits(), "{isa:?} iters={iters}");
            }
        }
    }

    #[test]
    fn the_work_really_changes_the_state_and_zero_iterations_do_not() {
        let mut s = FmaState::new();
        run(&mut s, Isa::Avx2, 0);
        assert_eq!(s, FmaState::new());
        run(&mut s, Isa::Avx2, 1000);
        assert_ne!(s.bits(), FmaState::new().bits());
        // finite and still growing slowly: no overflow, no denormals
        assert!(s
            .acc
            .iter()
            .flatten()
            .all(|v| v.is_finite() && *v > 1.0 && *v < 3.0));
    }

    #[test]
    fn the_flop_count_is_two_per_fma() {
        assert_eq!(flops(1), 8 * 16 * 2);
        assert_eq!(flops(1000), 256_000);
    }

    #[test]
    fn the_target_cpu_dispatches_to_avx512() {
        // Target machine (i5-11400H, AVX-512F confirmed by CPUID 2026-11-21); other CPUs downgrade by design.
        assert_eq!(effective(Isa::Avx512), Isa::Avx512);
        assert_eq!(effective(Isa::Avx2), Isa::Avx2);
        assert_eq!(effective(Isa::Scalar), Isa::Scalar);
    }

    #[test]
    fn burn_stops_within_a_few_blocks_of_the_flag() {
        let stop = AtomicBool::new(false);
        let b = std::thread::scope(|s| {
            let h = s.spawn(|| burn(Isa::Avx2, &stop, 20_000));
            std::thread::sleep(std::time::Duration::from_millis(30));
            stop.store(true, Ordering::Relaxed);
            h.join().unwrap()
        });
        assert!(b.flops > 0 && b.ticks > 0);
        // an already-set flag does no work at all
        assert_eq!(burn(Isa::Avx2, &stop, 20_000), Burn::default());
    }
}
