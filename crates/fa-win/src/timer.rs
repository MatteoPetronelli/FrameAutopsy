//! WIN-07 (timers part): a high-resolution waitable timer owned by the process, used by the wake-up latency probe.
//! It is a handle to a timer object we create ourselves: no handle on any other process or thread.

use std::io;
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CreateWaitableTimerExW, SetWaitableTimer, WaitForSingleObject,
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, TIMER_ALL_ACCESS,
};

/// One-shot high-resolution waitable timer (closed on drop).
pub struct HiResTimer {
    handle: HANDLE,
}

impl HiResTimer {
    pub fn new() -> io::Result<HiResTimer> {
        // SAFETY: null attributes and null name create an unnamed timer with default security; the flags and access
        // mask are the documented constants.
        let handle = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(HiResTimer { handle })
    }

    /// Blocks the calling thread for `delay` (resolution: the timer's, well under the 15.6 ms default tick).
    pub fn wait(&self, delay: Duration) -> io::Result<()> {
        // A negative due time is relative, in 100 ns units.
        let due = -i64::try_from(delay.as_nanos() / 100)
            .unwrap_or(i64::MAX)
            .max(1);
        // SAFETY: `self.handle` is a live timer; `due` outlives the call; no period, no completion routine.
        let ok = unsafe { SetWaitableTimer(self.handle, &due, 0, None, std::ptr::null(), 0) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `self.handle` is a live waitable object owned by `self`.
        match unsafe { WaitForSingleObject(self.handle, INFINITE) } {
            WAIT_OBJECT_0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl Drop for HiResTimer {
    fn drop(&mut self) {
        // SAFETY: `self.handle` is open and closed exactly once, here.
        unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_one_millisecond_wait_takes_between_one_and_five_milliseconds() {
        let t = HiResTimer::new().unwrap();
        // best of several tries ignores a preemption
        let best = (0..20)
            .map(|_| {
                let s = Instant::now();
                t.wait(Duration::from_millis(1)).unwrap();
                s.elapsed()
            })
            .min()
            .unwrap();
        assert!(best >= Duration::from_micros(900), "woke early: {best:?}");
        assert!(best < Duration::from_millis(5), "woke late: {best:?}");
    }

    #[test]
    fn a_zero_delay_still_returns() {
        HiResTimer::new().unwrap().wait(Duration::ZERO).unwrap();
    }

    #[test]
    fn the_timer_can_be_reused() {
        let t = HiResTimer::new().unwrap();
        for _ in 0..3 {
            t.wait(Duration::from_micros(200)).unwrap();
        }
    }

    #[test]
    fn a_pinned_thread_wakes_on_the_requested_cpu() {
        let _pin = crate::thread::pin_current_thread(3).unwrap();
        let t = HiResTimer::new().unwrap();
        for _ in 0..20 {
            t.wait(Duration::from_millis(1)).unwrap();
            assert_eq!(crate::thread::current_cpu(), 3);
        }
    }
}
