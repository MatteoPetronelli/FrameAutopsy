//! Safe CPUID wrapper. Used by validation tests (topology cross-check), not as a runtime data source.

use core::arch::x86_64::__cpuid_count;

/// Runs CPUID for `leaf`/`sub` and returns `[eax, ebx, ecx, edx]`.
pub fn cpuid_count(leaf: u32, sub: u32) -> [u32; 4] {
    // SAFETY: CPUID exists on every x86_64 CPU and has no side effect; an unsupported leaf returns zeros or data of the
    // highest basic leaf, never a fault.
    let r = unsafe { __cpuid_count(leaf, sub) };
    [r.eax, r.ebx, r.ecx, r.edx]
}

/// The processor brand string (CPUID `0x80000002..=0x80000004`), trimmed; empty when the CPU does not report one.
pub fn brand_string() -> String {
    if cpuid_count(0x8000_0000, 0)[0] < 0x8000_0004 {
        return String::new();
    }
    let mut bytes = Vec::with_capacity(48);
    for leaf in 0x8000_0002..=0x8000_0004 {
        for r in cpuid_count(leaf, 0) {
            bytes.extend_from_slice(&r.to_le_bytes());
        }
    }
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

/// WAITPKG (`tpause`/`umwait`): CPUID `7.0:ECX[5]`.
pub fn has_waitpkg() -> bool {
    cpuid_count(0, 0)[0] >= 7 && cpuid_count(7, 0)[2] >> 5 & 1 == 1
}

/// MWAITX (`monitorx`/`mwaitx`, AMD): CPUID `0x80000001:ECX[29]`.
pub fn has_mwaitx() -> bool {
    cpuid_count(0x8000_0000, 0)[0] >= 0x8000_0001 && cpuid_count(0x8000_0001, 0)[2] >> 29 & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brand_string_names_the_target_cpu() {
        let b = brand_string();
        assert!(b.contains("i5-11400H"), "{b:?}"); // target machine, 2026-11-22
    }

    #[test]
    fn leaf_zero_reports_a_vendor_and_a_max_leaf() {
        let [max, b, c, d] = cpuid_count(0, 0);
        assert!(max >= 1);
        assert_ne!(b | c | d, 0);
    }

    /// Target machine (i5-11400H), checked 2026-11-21: neither feature, so ASM-05 stays dropped (catalog).
    #[test]
    fn the_target_cpu_has_neither_waitpkg_nor_mwaitx() {
        assert!(!has_waitpkg() && !has_mwaitx());
    }
}
