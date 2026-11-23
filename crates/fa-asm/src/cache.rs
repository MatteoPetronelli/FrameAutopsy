//! ASM-07 (reference): a cache-hostile sweep for the Lab `cache-thrash` injector.
//!
//! One pass visits every 64-byte line of the buffer once, in a scattered order (a fixed prime step in lines, so the
//! hardware prefetcher cannot follow), and dirties it. With `flush` set, each line is also evicted from every cache
//! level by `clflush` right after, so the next pass finds it in RAM and the victim's data is pushed out of the shared
//! last-level cache and the memory controller is kept busy.
//!
//! `_mm_clflushopt` does not exist in `core::arch` on this toolchain (rustc 1.92): the reference uses `clflush`
//! (SSE2). A `clflushopt` `asm!` candidate would only change the flush throughput, and the design (D-6) writes asm
//! only if this reference cannot produce the behaviour. The behaviour (victim latency jump) is measured in the catalog.
//! `clflush` is ordered with respect to writes to the same line, so the closing `mfence` is for the caller's timing.

use core::arch::x86_64::{_mm_clflush, _mm_mfence};

const LINE_WORDS: usize = 8;
/// Distance between two visited lines, in lines: prime, so every line is reached when the line count is not a
/// multiple of it (a power of two never is).
const STEP_LINES: usize = 67;

/// One pass over `buf` (a whole number of lines, at least one). Returns the number of lines visited.
pub fn sweep(buf: &mut [u64], flush: bool) -> u64 {
    let lines = buf.len() / LINE_WORDS;
    let mut line = 0usize;
    for _ in 0..lines {
        let i = line * LINE_WORDS;
        buf[i] = buf[i].wrapping_add(1);
        if flush {
            let p: *const u8 = std::ptr::from_ref(&buf[i]).cast();
            // SAFETY: `p` points at a live, initialised element of `buf`; `clflush` only evicts its cache line and
            // changes no data.
            unsafe { _mm_clflush(p) };
        }
        line = (line + STEP_LINES) % lines;
    }
    // SAFETY: `mfence` takes no operand and only orders memory operations.
    unsafe { _mm_mfence() };
    lines as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_dirties_every_line_exactly_once_with_and_without_flush() {
        for flush in [false, true] {
            let mut buf = vec![0u64; 1024 * LINE_WORDS]; // 1024 lines, a power of two
            assert_eq!(sweep(&mut buf, flush), 1024);
            for (l, chunk) in buf.chunks(LINE_WORDS).enumerate() {
                assert_eq!(chunk[0], 1, "line {l} (flush {flush})");
                assert!(
                    chunk[1..].iter().all(|&w| w == 0),
                    "only the first word of a line changes"
                );
            }
        }
    }

    #[test]
    fn flush_does_not_change_the_data() {
        let (mut a, mut b) = (vec![0u64; 512 * LINE_WORDS], vec![0u64; 512 * LINE_WORDS]);
        sweep(&mut a, false);
        sweep(&mut b, true);
        assert_eq!(a, b);
    }
}
