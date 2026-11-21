//! ASM-01: TSC reads. This is the intrinsics version (`_rdtsc`/`__rdtscp` + `_mm_lfence`), the reference that ships
//! unless the `asm!` candidate beats it (catalog `docs/asm-unsafe-catalog.md`, protocol in section 2).
//!
//! CPU features are read once; `FA_FORCE_SCALAR=1` forces every fallback, here the reference read path.

use crate::cpuid::cpuid_count;
use core::arch::x86_64::{__rdtscp, _mm_lfence, _rdtsc};
use std::sync::OnceLock;

struct Features {
    invariant_tsc: bool,
    rdtscp: bool,
}

fn cpuid_edx(leaf: u32) -> u32 {
    cpuid_count(leaf, 0)[3]
}

fn max_extended_leaf() -> u32 {
    cpuid_count(0x8000_0000, 0)[0]
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

/// CPUID leaf `0x15` as `(eax denominator, ebx numerator, ecx crystal Hz)`; `None` when the CPU lacks the leaf.
pub fn cpuid_leaf_15() -> Option<(u32, u32, u32)> {
    if cpuid_count(0, 0)[0] < 0x15 {
        return None;
    }
    let [eax, ebx, ecx, _] = cpuid_count(0x15, 0);
    Some((eax, ebx, ecx))
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

/// Intrinsics version: the reference of ASM-01. It ships unless the `asm!` version beats it (catalog section 2).
pub mod reference {
    use super::{__rdtscp, _mm_lfence, _rdtsc, has_rdtscp};

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

    /// `rdtscp; lfence`: `rdtscp` waits for earlier instructions, the fence stops later ones from starting early.
    /// Without `rdtscp` (checked once) it falls back to [`now`].
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
}

/// Hand-written `asm!` candidate of ASM-01, same instructions as the reference. Intel syntax (the `asm!` default).
///
/// Register contract: `rdtsc` and `rdtscp` write the low and high halves of the counter to EAX and EDX; `rdtscp` also
/// writes IA32_TSC_AUX to ECX. All three are declared as outputs so the compiler never keeps a live value in them.
/// `lfence` and `rdtsc` leave the flags untouched (`preserves_flags`) and the code uses no stack (`nostack`). `nomem`
/// is deliberately NOT given: the block must stay a compiler barrier, otherwise the compiler could move the timed code
/// across the read.
pub mod asm {
    use super::has_rdtscp;
    use core::arch::asm;

    /// `lfence; rdtsc; lfence`.
    pub fn now() -> u64 {
        let (lo, hi): (u32, u32);
        // SAFETY: `lfence` (SSE2) and `rdtsc` exist on every x86_64 CPU and `rdtsc` is unprivileged on Windows user
        // mode; EAX and EDX are declared as outputs and nothing else is touched.
        unsafe {
            asm!(
                "lfence",
                "rdtsc",
                "lfence",
                out("eax") lo,
                out("edx") hi,
                options(nostack, preserves_flags)
            );
        }
        u64::from(hi) << 32 | u64::from(lo)
    }

    /// `rdtscp; lfence`. Without `rdtscp` (checked once) it falls back to [`now`].
    pub fn now_serialized() -> u64 {
        if !has_rdtscp() {
            return now();
        }
        let (lo, hi): (u32, u32);
        // SAFETY: `rdtscp` is implemented (checked through CPUID 0x80000001); EAX, EDX and ECX (IA32_TSC_AUX, unused)
        // are declared as outputs and nothing else is touched.
        unsafe {
            asm!(
                "rdtscp",
                "lfence",
                out("eax") lo,
                out("edx") hi,
                out("ecx") _,
                options(nostack, preserves_flags)
            );
        }
        u64::from(hi) << 32 | u64::from(lo)
    }
}

/// True when the `asm!` version is selected: always, unless `FA_FORCE_SCALAR=1` forces the reference.
fn use_asm(force_scalar: bool) -> bool {
    !force_scalar
}

/// Name of the implementation behind [`now`] and [`now_serialized`] (`"asm"` or `"reference"`).
pub fn implementation() -> &'static str {
    if use_asm(force_scalar()) {
        "asm"
    } else {
        "reference"
    }
}

/// `lfence; rdtsc; lfence`, through the selected implementation.
pub fn now() -> u64 {
    if use_asm(force_scalar()) {
        asm::now()
    } else {
        reference::now()
    }
}

/// `rdtscp; lfence`, through the selected implementation.
pub fn now_serialized() -> u64 {
    if use_asm(force_scalar()) {
        asm::now_serialized()
    } else {
        reference::now_serialized()
    }
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

    /// Parity (catalog ASM-01): the `asm!` and intrinsics versions read the same counter, so close in time they
    /// differ only by the read overhead; the minimum over many tries ignores preemption. Both orders are checked.
    #[test]
    fn asm_and_reference_read_the_same_counter() {
        type Read = fn() -> u64;
        let pairs: [(Read, Read); 4] = [
            (asm::now, reference::now),
            (reference::now, asm::now),
            (asm::now_serialized, reference::now_serialized),
            (reference::now_serialized, asm::now_serialized),
        ];
        for (first, second) in pairs {
            let min_gap = (0..2_000)
                .map(|_| {
                    let a = first();
                    let b = second();
                    a.abs_diff(b)
                })
                .min()
                .unwrap();
            assert!(
                min_gap < 5_000,
                "implementations disagree: min gap {min_gap} ticks"
            );
        }
    }

    #[test]
    fn both_implementations_are_monotonic() {
        for read in [
            asm::now,
            reference::now,
            asm::now_serialized,
            reference::now_serialized,
        ] {
            let mut last = read();
            for _ in 0..10_000 {
                let t = read();
                assert!(t >= last, "TSC went backwards: {last} -> {t}");
                last = t;
            }
        }
    }

    #[test]
    fn force_scalar_selects_the_reference() {
        assert!(use_asm(false) && !use_asm(true));
        // Run this suite with and without `FA_FORCE_SCALAR=1`: the process-wide switch must match.
        let forced = std::env::var_os("FA_FORCE_SCALAR").is_some_and(|v| v == "1");
        assert_eq!(implementation(), if forced { "reference" } else { "asm" });
    }

    #[test]
    fn the_target_cpu_reports_an_invariant_tsc_and_rdtscp() {
        // Measured on the target machine (i5-11400H) on 2026-11-21; a VM or another CPU may differ.
        assert!(invariant_tsc() && has_rdtscp());
    }
}
