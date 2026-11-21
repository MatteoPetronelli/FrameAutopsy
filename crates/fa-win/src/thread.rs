//! WIN-07 (pinning part): pin the CALLING thread to one logical CPU. It touches only our own thread, through the
//! current-thread pseudo-handle: no `OpenThread`, no handle on any other thread or process.

use std::io;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessorNumber, GetCurrentThread, SetThreadAffinityMask,
};

/// Restores the previous affinity mask of the calling thread when dropped. Keep it on the thread that created it.
#[derive(Debug)]
pub struct PinGuard {
    previous: usize,
}

impl PinGuard {
    #[cfg(test)]
    pub(crate) fn previous_mask(&self) -> usize {
        self.previous
    }
}

fn set_mask(mask: usize) -> usize {
    // SAFETY: `GetCurrentThread` has no preconditions; it returns a pseudo-handle valid for the calling thread.
    let me = unsafe { GetCurrentThread() };
    // SAFETY: `me` is the calling thread's pseudo-handle and `mask` is non-zero, within the thread's processor group.
    unsafe { SetThreadAffinityMask(me, mask) }
}

/// Pins the calling thread to `logical_cpu`.
///
/// shortcut: a single processor group (at most 64 logical CPUs); upgrade to `SetThreadGroupAffinity` if the target
/// machine ever has more.
pub fn pin_current_thread(logical_cpu: u32) -> io::Result<PinGuard> {
    if logical_cpu >= usize::BITS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "logical CPU outside the first processor group",
        ));
    }
    match set_mask(1usize << logical_cpu) {
        0 => Err(io::Error::last_os_error()),
        previous => Ok(PinGuard { previous }),
    }
}

impl Drop for PinGuard {
    fn drop(&mut self) {
        set_mask(self.previous);
    }
}

/// Logical CPU the calling thread is running on right now.
pub fn current_cpu() -> u32 {
    // SAFETY: `GetCurrentProcessorNumber` has no preconditions and cannot fail.
    unsafe { GetCurrentProcessorNumber() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pinned_thread_runs_on_the_requested_cpu() {
        let n = std::thread::available_parallelism().unwrap().get() as u32;
        for cpu in 0..n {
            let guard = pin_current_thread(cpu).unwrap();
            // The OS moves the thread as soon as its mask excludes the current CPU; yield once to be sure.
            std::thread::yield_now();
            assert_eq!(current_cpu(), cpu);
            drop(guard);
        }
    }

    #[test]
    fn the_previous_mask_is_restored_on_drop() {
        let first = pin_current_thread(0).unwrap();
        let original = first.previous_mask();
        drop(first);
        let again = pin_current_thread(0).unwrap();
        assert_eq!(
            again.previous_mask(),
            original,
            "dropping the guard must restore the original mask"
        );
    }

    #[test]
    fn a_cpu_outside_the_first_group_is_refused() {
        assert!(pin_current_thread(usize::BITS).is_err());
    }
}
