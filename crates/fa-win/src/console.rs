//! WIN-10 (console): a Ctrl-C / Ctrl-Break flag, so a long command can stop cleanly. It only sets an atomic flag.

use std::io;
use std::panic::catch_unwind;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::core::BOOL;
use windows_sys::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT};

static REQUESTED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn handler(ctrl_type: u32) -> BOOL {
    // The body must not unwind into the OS: a panic is treated as "not handled".
    catch_unwind(|| {
        if ctrl_type == CTRL_C_EVENT || ctrl_type == CTRL_BREAK_EVENT {
            REQUESTED.store(true, Ordering::SeqCst);
            1
        } else {
            0
        }
    })
    .unwrap_or(0)
}

/// Installs the handler: from now on Ctrl-C sets the flag instead of killing the process.
pub fn install_ctrl_c() -> io::Result<()> {
    // SAFETY: `handler` matches `PHANDLER_ROUTINE`, lives for the whole process and only touches an atomic.
    if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// True once Ctrl-C or Ctrl-Break was pressed.
pub fn ctrl_c_requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handler_sets_the_flag_for_ctrl_c_and_ignores_other_events() {
        assert!(!ctrl_c_requested());
        // SAFETY: the handler is a plain function over an integer; calling it directly is how the OS calls it.
        assert_eq!(unsafe { handler(2) }, 0, "CTRL_CLOSE_EVENT is not handled");
        assert!(!ctrl_c_requested());
        // SAFETY: as above.
        assert_eq!(unsafe { handler(CTRL_C_EVENT) }, 1);
        assert!(ctrl_c_requested());
        REQUESTED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn installing_the_handler_succeeds() {
        install_ctrl_c().unwrap();
    }
}
