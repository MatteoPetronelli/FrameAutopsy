//! ASM-01: TSC reads. This is the intrinsics version (`_rdtsc`/`__rdtscp` + `_mm_lfence`), the reference that ships
//! unless the `asm!` candidate beats it (catalog `docs/asm-unsafe-catalog.md`, protocol in section 2).
//!
//! CPU features are read once; `FA_FORCE_SCALAR=1` forces every fallback, here the reference read path.

use core::arch::x86_64::{__cpuid, __rdtscp, _mm_lfence, _rdtsc};
use std::sync::OnceLock;

struct Features {
    invariant_tsc: bool,
    rdtscp: bool,
}

fn cpuid_edx(leaf: u32) -> u32 {
    // SAFETY: CPUID exists on every x86_64 CPU and has no side effect; an unsupported leaf returns zeros or data of the
    // highest basic leaf, never a fault.
    unsafe { __cpuid(leaf) }.edx
}

fn max_extended_leaf() -> u32 {
    // SAFETY: as for `cpuid_edx`.
    unsafe { __cpuid(0x8000_0000) }.eax
}

/// `0x80000007:EDX[8]`.
fn invariant_flag(edx: u32) -> bool {
    edx >> 8 & 1 == 1
}

/// `0x80000001:EDX[27]`.
fn rdtscp_flag(edx: u32) -> bool {
    edx >> 27 & 1 == 1
}

fn features() -> &'static Features {
    static F: OnceLock<Features> = OnceLock::new();
    F.get_or_init(|| {
        let max = max_extended_leaf();
        Features {
            invariant_tsc: max >= 0x8000_0007 && invariant_flag(cpuid_edx(0x8000_0007)),
            rdtscp: max >= 0x8000_0001 && rdtscp_flag(cpuid_edx(0x8000_0001)),
        }
    })
}

/// True when CPUID reports an invariant TSC (constant rate across P-states and sleep, same on all cores).
pub fn invariant_tsc() -> bool {
    features().invariant_tsc
}

/// True when the CPU implements `rdtscp`.
pub fn has_rdtscp() -> bool {
    features().rdtscp
}

/// `FA_FORCE_SCALAR=1`: use the reference implementation of every kernel. Read once.
pub fn force_scalar() -> bool {
    static F: OnceLock<bool> = OnceLock::new();
    *F.get_or_init(|| std::env::var_os("FA_FORCE_SCALAR").is_some_and(|v| v == "1"))
}

/// `lfence; rdtsc; lfence`: the read cannot start before earlier instructions finish nor be reordered with later ones.
pub fn now() -> u64 {
    // SAFETY: `lfence` is part of SSE2, mandatory on x86_64; it touches no memory.
    unsafe { _mm_lfence() };
    // SAFETY: `rdtsc` is unprivileged on Windows user mode and present on every x86_64 CPU.
    let t = unsafe { _rdtsc() };
    // SAFETY: as for the first fence.
    unsafe { _mm_lfence() };
    t
}

/// `rdtscp; lfence`: `rdtscp` waits for earlier instructions, the fence stops later ones from starting early. Without
/// `rdtscp` (checked once) it falls back to [`now`].
pub fn now_serialized() -> u64 {
    if !has_rdtscp() {
        return now();
    }
    let mut aux = 0u32;
    // SAFETY: `rdtscp` is implemented (checked through CPUID 0x80000001) and writes IA32_TSC_AUX to the valid `aux`.
    let t = unsafe { __rdtscp(&mut aux) };
    // SAFETY: `lfence` is part of SSE2 and touches no memory.
    unsafe { _mm_lfence() };
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_decoded_from_the_right_bits() {
        assert!(invariant_flag(1 << 8) && !invariant_flag(!(1 << 8)));
        assert!(rdtscp_flag(1 << 27) && !rdtscp_flag(!(1 << 27)));
    }

    #[test]
    fn reads_are_monotonic_on_one_thread() {
        let mut last = now();
        for _ in 0..10_000 {
            let t = if last.is_multiple_of(2) {
                now()
            } else {
                now_serialized()
            };
            assert!(t >= last, "TSC went backwards: {last} -> {t}");
            last = t;
        }
    }

    /// Parity: both reads measure the same counter, so close in time they differ only by the read overhead.
    /// The minimum over many tries ignores preemption.
    #[test]
    fn now_and_now_serialized_agree_within_the_read_overhead() {
        let min_gap = (0..2_000)
            .map(|_| {
                let a = now();
                let b = now_serialized();
                a.abs_diff(b)
            })
            .min()
            .unwrap();
        assert!(
            min_gap < 5_000,
            "gap of {min_gap} ticks is far above a read overhead"
        );
    }

    #[test]
    fn the_target_cpu_reports_an_invariant_tsc_and_rdtscp() {
        // Measured on the target machine (i5-11400H) on 2026-11-21; a VM or another CPU may differ.
        assert!(invariant_tsc() && has_rdtscp());
    }
}
