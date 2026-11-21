//! Safe CPUID wrapper. Used by validation tests (topology cross-check), not as a runtime data source.

use core::arch::x86_64::__cpuid_count;

/// Runs CPUID for `leaf`/`sub` and returns `[eax, ebx, ecx, edx]`.
pub fn cpuid_count(leaf: u32, sub: u32) -> [u32; 4] {
    // SAFETY: CPUID exists on every x86_64 CPU and has no side effect; an unsupported leaf returns zeros or data of the
    // highest basic leaf, never a fault.
    let r = unsafe { __cpuid_count(leaf, sub) };
    [r.eax, r.ebx, r.ecx, r.edx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_zero_reports_a_vendor_and_a_max_leaf() {
        let [max, b, c, d] = cpuid_count(0, 0);
        assert!(max >= 1);
        assert_ne!(b | c | d, 0);
    }
}
