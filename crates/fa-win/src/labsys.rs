//! WIN-12: system calls of the Lab injectors. They act on the Lab's own process only: memory it allocates itself and
//! its own working set (through the current-process pseudo handle, never `OpenProcess`), its own threads' priority,
//! its own timer period, and a free-space query on a directory.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::ptr;
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod, TIMERR_NOERROR};
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH,
};
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, SetProcessWorkingSetSize, SetThreadPriority,
    THREAD_PRIORITY_TIME_CRITICAL,
};

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

/// Creates a new file for writing that bypasses the system cache and waits for the disk (`NO_BUFFERING` needs buffers
/// and write sizes that are multiples of the sector size, and an aligned address: a page-aligned 1 MiB buffer is).
/// Safe: the flags go through the standard library's open options.
pub fn create_unbuffered(path: &Path) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH)
        .open(path)
}

/// Free bytes available to the caller on the volume holding `dir` (the disk-burst guard reads it before any write).
pub fn free_disk_bytes(dir: &Path) -> io::Result<u64> {
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the call; `free` is a valid out-pointer and the
    // two other out-pointers are documented as optional (null).
    let ok =
        unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, ptr::null_mut(), ptr::null_mut()) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(free)
}

/// Raises the calling thread to the highest priority of its class: `priority-contention` workers use it so that the
/// scheduler prefers them to the (normal priority) render thread. Own thread only, through the pseudo handle.
pub fn raise_current_thread_priority() -> io::Result<()> {
    // SAFETY: `GetCurrentThread` takes no argument and returns the pseudo handle of the calling thread.
    let me = unsafe { GetCurrentThread() };
    // SAFETY: `me` is the current-thread pseudo handle, which needs no closing and carries the access this call needs;
    // the level is a documented constant.
    if unsafe { SetThreadPriority(me, THREAD_PRIORITY_TIME_CRITICAL) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A `timeBeginPeriod` request for this process, undone by `timeEndPeriod` on drop. Since Windows 10 2004 the request
/// only changes this process's own timers.
pub struct TimerPeriod(u32);

impl TimerPeriod {
    pub fn begin(ms: u32) -> io::Result<TimerPeriod> {
        // SAFETY: plain call with a period in milliseconds; it is paired with `timeEndPeriod` of the same value on drop.
        if unsafe { timeBeginPeriod(ms) } != TIMERR_NOERROR {
            return Err(io::Error::other("timeBeginPeriod refused"));
        }
        Ok(TimerPeriod(ms))
    }
}

impl Drop for TimerPeriod {
    fn drop(&mut self) {
        // SAFETY: matches the successful `timeBeginPeriod(self.0)` of `begin`, once.
        unsafe { timeEndPeriod(self.0) };
    }
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
    fn free_space_of_the_temp_dir_is_positive_and_a_missing_dir_errors() {
        assert!(free_disk_bytes(&std::env::temp_dir()).unwrap() > 0);
        assert!(free_disk_bytes(Path::new(r"Z:\no\such\dir")).is_err());
    }

    #[test]
    fn period_and_priority_calls_succeed_on_this_process_and_thread() {
        drop(TimerPeriod::begin(1).unwrap());
        // On a throw-away thread: the priority stays with the thread.
        std::thread::spawn(|| raise_current_thread_priority().unwrap())
            .join()
            .unwrap();
    }

    #[test]
    fn trimming_the_working_set_succeeds() {
        trim_working_set().unwrap();
    }
}
