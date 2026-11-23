//! WIN-12: system calls of the Lab injectors. They act on the Lab's own process only: memory it allocates itself and
//! its own working set (through the current-process pseudo handle, never `OpenProcess`).

use std::io;
use std::ptr;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};

const PAGE: usize = 4096;

/// One page-fault cycle: commits `bytes` of fresh memory, touches one byte of every 4 KiB page (each first touch is a
/// demand-zero page fault) and releases the region. Returns the number of pages touched.
pub fn fault_cycle(bytes: usize) -> io::Result<u64> {
    let bytes = bytes.max(PAGE).next_multiple_of(PAGE);
    // SAFETY: a null address lets the system choose; the size is non-zero; the flags and protection are documented
    // constants. The result is checked before any use.
    let base =
        unsafe { VirtualAlloc(ptr::null(), bytes, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) };
    if base.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `base` is the start of `bytes` bytes of committed, zero-initialised, read-write memory that only this
    // function can reach until the `VirtualFree` below; `u8` has no alignment or validity requirement.
    let region = unsafe { std::slice::from_raw_parts_mut(base.cast::<u8>(), bytes) };
    let mut pages = 0u64;
    for page in region.chunks_mut(PAGE) {
        page[0] = 1;
        pages += 1;
    }
    // SAFETY: `base` comes from the `VirtualAlloc` above and is released once; with `MEM_RELEASE` the size must be 0.
    // `region` is not used after this point.
    if unsafe { VirtualFree(base, 0, MEM_RELEASE) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pages)
}

/// Empties the working set of the calling process: its pages go to the standby list and the next touch of each is a
/// (soft) page fault. `(SIZE_T)-1` for both limits is the documented way to ask for a trim.
pub fn trim_working_set() -> io::Result<()> {
    // SAFETY: `GetCurrentProcess` takes no argument and returns the pseudo handle of this process.
    let me = unsafe { GetCurrentProcess() };
    // SAFETY: `me` is the current-process pseudo handle, which needs no closing and grants the access this call needs;
    // the `usize::MAX` pair is the documented "trim" request.
    if unsafe { SetProcessWorkingSetSize(me, usize::MAX, usize::MAX) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Page faults (hard and soft) of this process since it started: the ground truth of what `page-fault-storm` causes.
pub fn own_page_faults() -> io::Result<u64> {
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: `GetCurrentProcess` takes no argument and returns the pseudo handle of this process.
    let me = unsafe { GetCurrentProcess() };
    // SAFETY: `me` is the current-process pseudo handle; `counters` is a valid out-pointer whose `cb` holds its size.
    if unsafe { K32GetProcessMemoryInfo(me, &mut counters, counters.cb) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(u64::from(counters.PageFaultCount))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cycle_touches_every_page_of_the_region_and_releases_it() {
        assert_eq!(fault_cycle(1 << 20).unwrap(), 256);
        assert_eq!(
            fault_cycle(1).unwrap(),
            1,
            "a request is rounded up to one page"
        );
        // The same cycle many times must not leak: 200 x 8 MiB would exhaust a small commit limit otherwise.
        for _ in 0..200 {
            fault_cycle(8 << 20).unwrap();
        }
    }

    #[test]
    fn the_own_fault_counter_rises_with_a_cycle() {
        let before = own_page_faults().unwrap();
        fault_cycle(8 << 20).unwrap(); // 2048 pages
        let rise = own_page_faults().unwrap() - before;
        assert!(
            rise >= 1500,
            "a cycle of 2048 first touches raised the counter by only {rise}"
        );
    }

    #[test]
    fn trimming_the_working_set_succeeds() {
        trim_working_set().unwrap();
    }
}
