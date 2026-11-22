//! WIN-10 (windows): the visible top-level windows with their owner process id, title and class name.
//!
//! Used to tell a game window from an ordinary Java program (the image name `javaw.exe` is not enough). It needs no
//! handle on any process: `EnumWindows` hands out window handles (HWND), `GetWindowThreadProcessId` gives the owner pid,
//! and the title and class are read from the window itself, the same information Task Manager and the taskbar show.
//! Nothing is sent to a window and nothing is written.

use std::panic::{catch_unwind, AssertUnwindSafe};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    pub pid: u32,
    pub title: String,
    pub class: String,
}

fn text_from_wide(buf: &[u16], len: i32) -> String {
    let n = usize::try_from(len).unwrap_or(0).min(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

fn read_window(hwnd: HWND) -> Option<WindowInfo> {
    // SAFETY: `hwnd` was handed out by `EnumWindows` a moment ago; a window that vanished makes the call return 0.
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out-pointer; `hwnd` as above.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    // SAFETY: `hwnd` as above; the call only reads the length of the title.
    let wanted = unsafe { GetWindowTextLengthW(hwnd) };
    let mut title = vec![0u16; usize::try_from(wanted).unwrap_or(0).min(1024) + 1];
    // SAFETY: the buffer holds `title.len()` UTF-16 units, the capacity passed to the call.
    let got = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    let mut class = vec![0u16; 256];
    // SAFETY: the buffer holds 256 UTF-16 units, the capacity passed to the call.
    let class_len = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32) };
    Some(WindowInfo {
        pid,
        title: text_from_wide(&title, got),
        class: text_from_wide(&class, class_len),
    })
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // A panic must not unwind through the OS frames of `EnumWindows`: swallow it and keep enumerating.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Some(w) = read_window(hwnd) {
            // SAFETY: `lparam` is the address of the `Vec` that `visible_windows` passed to `EnumWindows` and keeps
            // alive and unaliased for the whole call; the callback runs on the same thread.
            let out = unsafe { &mut *(lparam as *mut Vec<WindowInfo>) };
            out.push(w);
        }
    }));
    1
}

/// Every visible top-level window, in z-order, including those with an empty title.
pub fn visible_windows() -> Vec<WindowInfo> {
    let mut out: Vec<WindowInfo> = Vec::new();
    // SAFETY: `collect` matches the `WNDENUMPROC` signature and `out` outlives the call (it returns when the
    // enumeration is over); the pointer is only used inside the call.
    unsafe { EnumWindows(Some(collect), &mut out as *mut Vec<WindowInfo> as LPARAM) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_buffer_is_cut_at_the_reported_length() {
        let b: Vec<u16> = "Cobble\0\0".encode_utf16().collect();
        assert_eq!(text_from_wide(&b, 6), "Cobble");
        assert_eq!(text_from_wide(&b, 0), "");
        assert_eq!(text_from_wide(&b, -3), "");
        assert_eq!(
            text_from_wide(&b, 999),
            "Cobble\0\0",
            "never past the buffer"
        );
    }

    #[test]
    fn the_session_has_some_visible_windows_with_a_class_and_an_owner() {
        let w = visible_windows();
        // CI and headless sessions may have none; an interactive one always has the taskbar at least
        if !w.is_empty() {
            assert!(w.iter().all(|x| x.pid != 0 && !x.class.is_empty()));
        }
    }
}
