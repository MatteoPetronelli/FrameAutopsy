//! SYNC-00: counting global allocator, used to prove the hot path is allocation-free.
//!
//! Install it with `#[global_allocator] static A: CountingAlloc = CountingAlloc;` in a test binary, then wrap the code
//! under test in [`count_allocs`] or [`assert_no_alloc`]. Counts are per thread, so parallel tests do not disturb each
//! other. If it is not installed as the global allocator, every count is 0.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // `const` init: no lazy registration, so reading it inside the allocator can never allocate.
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn bump() {
    // try_access: the thread-local may already be destroyed while the thread exits; ignore then.
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}

/// Delegates to [`System`] and counts every allocation (`alloc`, `alloc_zeroed`, `realloc`) of the calling thread.
pub struct CountingAlloc;

// SAFETY: every method forwards to `System` with the caller's arguments unchanged, so `System`'s guarantees (layout,
// alignment, no unwinding) carry over; the counter touches only a const-initialised thread-local and never allocates.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: the caller upholds the `GlobalAlloc::alloc` contract (non-zero size) and we forward it unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: same contract as `alloc`, forwarded unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was returned by this allocator with `layout`; every allocation came from `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump();
        // SAFETY: `ptr`, `layout` and `new_size` satisfy the `GlobalAlloc::realloc` contract and `ptr` came from `System`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Runs `f` and returns its result with the number of allocations the current thread made meanwhile.
pub fn count_allocs<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.with(Cell::get);
    let r = f();
    (r, ALLOCS.with(Cell::get) - before)
}

/// Panics if `f` allocates.
pub fn assert_no_alloc<R>(f: impl FnOnce() -> R) -> R {
    let (r, n) = count_allocs(f);
    assert_eq!(n, 0, "expected zero allocations, got {n}");
    r
}
