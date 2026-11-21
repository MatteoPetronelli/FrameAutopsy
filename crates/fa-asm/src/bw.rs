//! ASM-04: memory bandwidth kernels (read with wide loads, write with non-temporal stores, copy).
//!
//! Verdict (2026-11-21): a hand-written AVX2 `asm!` candidate (`vpaddq`/`vmovdqa` loads, `vmovntdq` stores, `sfence`) was
//! benchmarked against these intrinsics on 512 KiB and 64 MiB buffers, 3 runs pinned on CPU 4. No operation won by 5 %
//! consistently (median gains -2.0 to +5.9 %, inside the run-to-run spread), so by the catalog protocol it is a tie and the
//! intrinsics ship. The asm code and its bench stay in git history; the numbers are in the catalog.
//!
//! Three kernels per operation: [`Kernel::Avx2`] and [`Kernel::Sse2`] (`core::arch` intrinsics) and [`Kernel::Scalar`]
//! (plain Rust). A kernel that the CPU cannot run, or a buffer that is not 32-byte aligned or not a multiple of 128 bytes,
//! is silently downgraded: the safe API never executes an instruction the CPU lacks and never reads out of bounds.
//!
//! Non-temporal stores (`vmovntdq` through `_mm256_stream_si256`) bypass the cache and are weakly ordered: the data only
//! counts as written after an `sfence`, which every write kernel ends with. Parity tests read the data back after the kernel.

use crate::tsc::force_scalar;
use core::arch::x86_64::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kernel {
    Avx2,
    Sse2,
    Scalar,
}

/// Words (u64) handled per loop iteration: 128 bytes.
const BLOCK: usize = 16;

fn has_avx2() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
}

/// The kernel that will really run for `k` on this CPU and these buffers (`ptrs` are the buffer base addresses).
fn effective(k: Kernel, ptrs: &[usize], words: usize) -> Kernel {
    let wide =
        words > 0 && words.is_multiple_of(BLOCK) && ptrs.iter().all(|p| p.is_multiple_of(32));
    let k = if force_scalar() { Kernel::Scalar } else { k };
    match k {
        Kernel::Avx2 if !has_avx2() => effective(Kernel::Sse2, ptrs, words),
        _ if !wide => Kernel::Scalar,
        k => k,
    }
}

/// The kernel to use when asm is not wanted.
pub fn best_intrinsics() -> Kernel {
    if has_avx2() {
        Kernel::Avx2
    } else {
        Kernel::Sse2
    }
}

// ---------------------------------------------------------------- read

/// Wrapping sum of all words.
pub fn read_sum(buf: &[u64], k: Kernel) -> u64 {
    let (p, n) = (buf.as_ptr(), buf.len());
    match effective(k, &[p as usize], n) {
        // SAFETY: same checks as above.
        Kernel::Avx2 => unsafe { read_avx2(p, n / BLOCK) },
        // SAFETY: SSE2 is part of x86_64; alignment and length as above.
        Kernel::Sse2 => unsafe { read_sse2(p, n / BLOCK) },
        Kernel::Scalar => buf.iter().fold(0u64, |a, &x| a.wrapping_add(x)),
    }
}

#[target_feature(enable = "avx2")]
unsafe fn read_avx2(p: *const u64, blocks: usize) -> u64 {
    let mut acc = [_mm256_setzero_si256(); 4];
    for b in 0..blocks {
        let base = p.wrapping_add(b * BLOCK);
        for (i, a) in acc.iter_mut().enumerate() {
            // SAFETY: `base + i*4` words stays inside the block `b` of the buffer; 32-byte aligned (caller contract).
            let v = unsafe { _mm256_load_si256(base.wrapping_add(i * 4).cast::<__m256i>()) };
            *a = _mm256_add_epi64(*a, v);
        }
    }
    hsum_avx2(acc)
}

#[target_feature(enable = "avx2")]
fn hsum_avx2(acc: [__m256i; 4]) -> u64 {
    let s = _mm256_add_epi64(
        _mm256_add_epi64(acc[0], acc[1]),
        _mm256_add_epi64(acc[2], acc[3]),
    );
    let mut out = [0u64; 4];
    // SAFETY: `out` is 32 bytes, the unaligned store writes exactly 32 bytes into it.
    unsafe { _mm256_storeu_si256(out.as_mut_ptr().cast::<__m256i>(), s) };
    out.iter().fold(0u64, |a, &x| a.wrapping_add(x))
}

#[target_feature(enable = "sse2")]
unsafe fn read_sse2(p: *const u64, blocks: usize) -> u64 {
    let mut acc = [_mm_setzero_si128(); 8];
    for b in 0..blocks {
        let base = p.wrapping_add(b * BLOCK);
        for (i, a) in acc.iter_mut().enumerate() {
            // SAFETY: `base + i*2` words stays inside block `b`; 16-byte aligned because the base is 32-byte aligned.
            let v = unsafe { _mm_load_si128(base.wrapping_add(i * 2).cast::<__m128i>()) };
            *a = _mm_add_epi64(*a, v);
        }
    }
    let mut s = _mm_setzero_si128();
    for a in acc {
        s = _mm_add_epi64(s, a);
    }
    let mut out = [0u64; 2];
    // SAFETY: `out` is 16 bytes, the unaligned store writes exactly 16 bytes into it.
    unsafe { _mm_storeu_si128(out.as_mut_ptr().cast::<__m128i>(), s) };
    out[0].wrapping_add(out[1])
}

// ---------------------------------------------------------------- write

/// Fills `buf` with `v` using non-temporal stores when the kernel allows it.
pub fn fill(buf: &mut [u64], v: u64, k: Kernel) {
    let (p, n) = (buf.as_mut_ptr(), buf.len());
    match effective(k, &[p as usize], n) {
        // SAFETY: same checks as above.
        Kernel::Avx2 => unsafe { fill_avx2(p, n / BLOCK, v) },
        // SAFETY: SSE2 is part of x86_64; alignment and length as above.
        Kernel::Sse2 => unsafe { fill_sse2(p, n / BLOCK, v) },
        Kernel::Scalar => buf.fill(v),
    }
}

#[target_feature(enable = "avx2")]
unsafe fn fill_avx2(p: *mut u64, blocks: usize, v: u64) {
    let x = _mm256_set1_epi64x(v as i64);
    for b in 0..blocks {
        let base = p.wrapping_add(b * BLOCK);
        for i in 0..4 {
            // SAFETY: inside block `b` of the exclusively borrowed buffer; 32-byte aligned (caller contract).
            unsafe { _mm256_stream_si256(base.wrapping_add(i * 4).cast::<__m256i>(), x) };
        }
    }
    // SAFETY: `sfence` has no operands; it orders the non-temporal stores above before any later store.
    unsafe { _mm_sfence() };
}

#[target_feature(enable = "sse2")]
unsafe fn fill_sse2(p: *mut u64, blocks: usize, v: u64) {
    let x = _mm_set1_epi64x(v as i64);
    for b in 0..blocks {
        let base = p.wrapping_add(b * BLOCK);
        for i in 0..8 {
            // SAFETY: inside block `b` of the exclusively borrowed buffer; 16-byte aligned (base is 32-aligned).
            unsafe { _mm_stream_si128(base.wrapping_add(i * 2).cast::<__m128i>(), x) };
        }
    }
    // SAFETY: `sfence` has no operands; it orders the non-temporal stores above before any later store.
    unsafe { _mm_sfence() };
}

// ---------------------------------------------------------------- copy

/// Copies `src` into `dst` (same length) with wide loads and non-temporal stores when the kernel allows it.
pub fn copy(dst: &mut [u64], src: &[u64], k: Kernel) {
    assert_eq!(dst.len(), src.len(), "copy needs equal lengths");
    let (d, s, n) = (dst.as_mut_ptr(), src.as_ptr(), dst.len());
    match effective(k, &[d as usize, s as usize], n) {
        // SAFETY: same checks as above.
        Kernel::Avx2 => unsafe { copy_avx2(d, s, n / BLOCK) },
        // SAFETY: SSE2 is part of x86_64; alignment, length and non-overlap as above.
        Kernel::Sse2 => unsafe { copy_sse2(d, s, n / BLOCK) },
        Kernel::Scalar => dst.copy_from_slice(src),
    }
}

#[target_feature(enable = "avx2")]
unsafe fn copy_avx2(d: *mut u64, s: *const u64, blocks: usize) {
    for b in 0..blocks {
        let (db, sb) = (d.wrapping_add(b * BLOCK), s.wrapping_add(b * BLOCK));
        for i in 0..4 {
            // SAFETY: inside block `b` of the source; 32-byte aligned (caller contract).
            let v = unsafe { _mm256_load_si256(sb.wrapping_add(i * 4).cast::<__m256i>()) };
            // SAFETY: inside block `b` of the exclusively borrowed destination; 32-byte aligned.
            unsafe { _mm256_stream_si256(db.wrapping_add(i * 4).cast::<__m256i>(), v) };
        }
    }
    // SAFETY: `sfence` has no operands; it orders the non-temporal stores above before any later store.
    unsafe { _mm_sfence() };
}

#[target_feature(enable = "sse2")]
unsafe fn copy_sse2(d: *mut u64, s: *const u64, blocks: usize) {
    for b in 0..blocks {
        let (db, sb) = (d.wrapping_add(b * BLOCK), s.wrapping_add(b * BLOCK));
        for i in 0..8 {
            // SAFETY: inside block `b` of the source; 16-byte aligned (bases are 32-aligned).
            let v = unsafe { _mm_load_si128(sb.wrapping_add(i * 2).cast::<__m128i>()) };
            // SAFETY: inside block `b` of the exclusively borrowed destination; 16-byte aligned.
            unsafe { _mm_stream_si128(db.wrapping_add(i * 2).cast::<__m128i>(), v) };
        }
    }
    // SAFETY: `sfence` has no operands; it orders the non-temporal stores above before any later store.
    unsafe { _mm_sfence() };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32-byte aligned buffer of `words` words (a Vec of 32-byte chunks viewed as words).
    #[repr(C, align(32))]
    #[derive(Clone, Copy)]
    struct Chunk([u64; 4]);

    fn aligned(words: usize) -> Vec<Chunk> {
        vec![Chunk([0; 4]); words.div_ceil(4)]
    }

    fn as_words(v: &mut [Chunk]) -> &mut [u64] {
        let len = v.len() * 4;
        // SAFETY: `Chunk` is `repr(C)` over `[u64; 4]`, so a slice of chunks is a slice of `4 * len` words.
        unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr().cast::<u64>(), len) }
    }

    const KERNELS: [Kernel; 3] = [Kernel::Avx2, Kernel::Sse2, Kernel::Scalar];

    fn pattern(i: usize) -> u64 {
        (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xDEAD_BEEF
    }

    #[test]
    fn every_kernel_reads_the_same_checksum() {
        for words in [16usize, 32, 4096, 1 << 16] {
            let mut v = aligned(words);
            let buf = as_words(&mut v);
            for (i, w) in buf.iter_mut().enumerate() {
                *w = pattern(i);
            }
            let want = buf.iter().fold(0u64, |a, &x| a.wrapping_add(x));
            for k in KERNELS {
                assert_eq!(read_sum(buf, k), want, "{k:?} {words}");
            }
        }
    }

    #[test]
    fn every_kernel_writes_the_same_bytes_visible_right_after_the_call() {
        for words in [16usize, 64, 4096] {
            for k in KERNELS {
                let mut v = aligned(words);
                let buf = as_words(&mut v);
                fill(buf, 0xA5A5_5A5A_1234_5678, k);
                assert!(
                    buf.iter().all(|&w| w == 0xA5A5_5A5A_1234_5678),
                    "{k:?} {words}"
                );
            }
        }
    }

    #[test]
    fn every_kernel_copies_the_same_bytes() {
        for words in [16usize, 48, 4096] {
            for k in KERNELS {
                let mut sv = aligned(words);
                let mut dv = aligned(words);
                let src = as_words(&mut sv);
                for (i, w) in src.iter_mut().enumerate() {
                    *w = pattern(i);
                }
                let dst = as_words(&mut dv);
                copy(dst, src, k);
                assert_eq!(dst, src, "{k:?} {words}");
            }
        }
    }

    /// A misaligned start or a length that is not a multiple of 16 words must still be correct (scalar path).
    #[test]
    fn unaligned_or_odd_buffers_fall_back_without_reading_out_of_bounds() {
        let mut v = aligned(80);
        let all = as_words(&mut v);
        let odd = &mut all[1..40]; // 8-byte aligned only, 39 words
        for (i, w) in odd.iter_mut().enumerate() {
            *w = pattern(i);
        }
        let want = odd.iter().fold(0u64, |a, &x| a.wrapping_add(x));
        for k in KERNELS {
            assert_eq!(read_sum(odd, k), want, "{k:?}");
            fill(odd, 7, k);
            assert!(odd.iter().all(|&w| w == 7));
            for (i, w) in odd.iter_mut().enumerate() {
                *w = pattern(i);
            }
        }
    }

    #[test]
    fn an_empty_buffer_is_fine() {
        for k in KERNELS {
            assert_eq!(read_sum(&[], k), 0);
            fill(&mut [], 1, k);
            copy(&mut [], &[], k);
        }
    }

    #[test]
    fn the_kernels_do_not_touch_a_guard_word_past_the_end() {
        for k in KERNELS {
            let mut v = aligned(48);
            let all = as_words(&mut v);
            all[32..].fill(u64::MAX);
            fill(&mut all[..32], 1, k);
            assert!(all[32..].iter().all(|&w| w == u64::MAX), "{k:?}");
        }
    }

    #[test]
    fn the_target_cpu_has_avx2_so_asm_and_avx2_really_run() {
        // Target machine (i5-11400H), 2026-11-21; on a CPU without AVX2 it is downgraded by design.
        assert!(has_avx2());
        let mut v = aligned(16);
        let p = as_words(&mut v).as_ptr() as usize;
        assert_eq!(effective(Kernel::Avx2, &[p], 16), Kernel::Avx2);
        assert_eq!(effective(Kernel::Avx2, &[p + 8], 16), Kernel::Scalar);
    }
}
