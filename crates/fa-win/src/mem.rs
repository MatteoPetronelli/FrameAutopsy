//! WIN-09: page-granular allocation for the memory probes, with optional 2 MiB large pages.
//!
//! Large pages need `SeLockMemoryPrivilege`, which only the user grants by local policy, and which a process must also
//! enable in its own token. We never grant or enable it (that would need `OpenProcessToken`, which the project rule
//! forbids outside `open_target`): the large-page attempt simply fails and we fall back to 4 KiB pages, reporting why.

use std::io;
use windows_sys::Win32::System::Memory::{
    GetLargePageMinimum, VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_LARGE_PAGES, MEM_RELEASE,
    MEM_RESERVE, PAGE_READWRITE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSize {
    /// 4 KiB pages (the default).
    Small,
    /// Large pages (2 MiB on x86_64).
    Large,
}

/// Zero-filled, page-aligned memory, freed on drop.
pub struct PageBuf {
    ptr: *mut std::ffi::c_void,
    words: usize,
    page: PageSize,
    large_failure: Option<i32>,
}

/// Size of a large page, or `None` when the system has none.
pub fn large_page_minimum() -> Option<usize> {
    // SAFETY: `GetLargePageMinimum` takes no arguments and only reads a system constant.
    match unsafe { GetLargePageMinimum() } {
        0 => None,
        n => Some(n),
    }
}

fn virtual_alloc(bytes: usize, flags: u32) -> *mut std::ffi::c_void {
    // SAFETY: a null address lets the system choose; `bytes` is non-zero; the flags are documented constants.
    unsafe { VirtualAlloc(std::ptr::null(), bytes, flags, PAGE_READWRITE) }
}

impl PageBuf {
    /// Allocates at least `bytes` bytes. With `try_large`, asks for large pages first and falls back to 4 KiB pages;
    /// [`PageBuf::page_size`] says which one was obtained and [`PageBuf::large_failure`] why large pages were refused.
    pub fn alloc(bytes: usize, try_large: bool) -> io::Result<PageBuf> {
        let bytes = bytes.max(1);
        let mut large_failure = None;
        if try_large {
            if let Some(min) = large_page_minimum() {
                let rounded = bytes.div_ceil(min) * min;
                let p = virtual_alloc(rounded, MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES);
                if !p.is_null() {
                    return Ok(PageBuf {
                        ptr: p,
                        words: rounded / size_of::<usize>(),
                        page: PageSize::Large,
                        large_failure: None,
                    });
                }
                large_failure = io::Error::last_os_error().raw_os_error();
            }
        }
        let rounded = bytes.div_ceil(4096) * 4096;
        let p = virtual_alloc(rounded, MEM_RESERVE | MEM_COMMIT);
        if p.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(PageBuf {
            ptr: p,
            words: rounded / size_of::<usize>(),
            page: PageSize::Small,
            large_failure,
        })
    }

    pub fn page_size(&self) -> PageSize {
        self.page
    }

    /// Windows error code of the refused large-page attempt (`1314` = privilege not held), if there was one.
    pub fn large_failure(&self) -> Option<i32> {
        self.large_failure
    }

    pub fn as_mut_slice(&mut self) -> &mut [usize] {
        // SAFETY: `ptr` is a live, committed, page-aligned read-write region of `words` words, zero-filled by the
        // system, exclusively owned by `self`; the returned borrow keeps `self` mutably borrowed.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.cast::<usize>(), self.words) }
    }
}

impl PageBuf {
    /// The same memory as 64-bit words (`usize` is 64 bits on x86_64; the bandwidth kernels take `u64`).
    pub fn as_mut_u64(&mut self) -> &mut [u64] {
        // SAFETY: same region and exclusivity as `as_mut_slice`; `u64` has the alignment of the page-aligned base and
        // the byte length is a multiple of 8.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.cast::<u64>(), self.words) }
    }

    /// The same memory as bytes (to write it to a file: the page-aligned base suits unbuffered I/O).
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: the region is live and committed for `words * 8` bytes, and `u8` has no alignment or validity rule;
        // the borrow keeps `self` borrowed shared, so nothing writes meanwhile.
        unsafe { std::slice::from_raw_parts(self.ptr.cast::<u8>(), self.words * 8) }
    }
}

impl Drop for PageBuf {
    fn drop(&mut self) {
        // SAFETY: `ptr` came from `VirtualAlloc`, is released exactly once, here; size 0 is required with MEM_RELEASE.
        unsafe { VirtualFree(self.ptr, 0, MEM_RELEASE) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_pages_are_zeroed_aligned_and_writable() {
        let mut b = PageBuf::alloc(10_000, false).unwrap();
        assert_eq!(b.page_size(), PageSize::Small);
        assert_eq!(b.large_failure(), None);
        let s = b.as_mut_slice();
        assert!(s.len() * 8 >= 10_000 && s.iter().all(|w| *w == 0));
        assert_eq!(s.as_ptr() as usize % 4096, 0);
        s[s.len() - 1] = 7;
        assert_eq!(s[s.len() - 1], 7);
    }

    /// Whatever the privilege state, a large-page request must yield usable memory and say which page size it got.
    #[test]
    fn a_large_page_request_always_returns_usable_memory_and_reports_the_page_size() {
        let mut b = PageBuf::alloc(3 << 20, true).unwrap();
        match b.page_size() {
            PageSize::Large => assert!(
                b.as_mut_slice().len() * 8 >= 4 << 20,
                "rounded to 2 MiB pages"
            ),
            PageSize::Small => {
                assert!(b.large_failure().is_some() || large_page_minimum().is_none())
            }
        }
        b.as_mut_slice()[0] = 1;
    }

    #[test]
    fn the_large_page_minimum_is_a_power_of_two_when_present() {
        if let Some(m) = large_page_minimum() {
            assert!(m.is_power_of_two() && m >= 1 << 16);
        }
    }
}
