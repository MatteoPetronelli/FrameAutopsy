//! WIN-10 (process names): the image names of the running processes, from a Toolhelp snapshot.
//!
//! A snapshot is a copy of the process list that Windows hands out as a handle to the SNAPSHOT, not to any process: it
//! gives the same information as Task Manager. No `OpenProcess`, no handle on any process, ever (enforced by the
//! source test of `fa-core`, which scans this file too). Names are full image names (up to 259 characters).

use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

/// Closes the snapshot handle when dropped.
struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: the handle came from `CreateToolhelp32Snapshot` and is closed exactly once, here.
        unsafe { CloseHandle(self.0) };
    }
}

/// Converts a NUL-terminated UTF-16 file name buffer to a `String` (lossy on unpaired surrogates).
fn name_from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Image names of every running process, in snapshot order (one entry per process, duplicates kept).
pub fn process_names() -> io::Result<Vec<String>> {
    // SAFETY: `TH32CS_SNAPPROCESS` with process id 0 snapshots all processes; no pointer is passed.
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snap = Snapshot(handle);
    // SAFETY: all-zero is a valid `PROCESSENTRY32W` (plain integers and arrays); `dwSize` is set right after.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut names = Vec::new();
    // SAFETY: `snap.0` is a live snapshot and `entry` is a valid, correctly sized out-structure.
    let mut more = unsafe { Process32FirstW(snap.0, &mut entry) } != 0;
    while more {
        names.push(name_from_wide(&entry.szExeFile));
        // SAFETY: same as above.
        more = unsafe { Process32NextW(snap.0, &mut entry) } != 0;
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_contains_the_running_test_process() {
        let me = std::env::current_exe().unwrap();
        let me = me.file_name().unwrap().to_string_lossy().into_owned();
        let names = process_names().unwrap();
        assert!(
            names.iter().any(|n| n.eq_ignore_ascii_case(&me)),
            "{me} not in {} names",
            names.len()
        );
        assert!(
            names.len() > 20,
            "a Windows session runs far more than 20 processes"
        );
    }

    #[test]
    fn a_wide_buffer_is_cut_at_the_first_nul() {
        let mut b = [0u16; 8];
        for (i, c) in "game".encode_utf16().enumerate() {
            b[i] = c;
        }
        assert_eq!(name_from_wide(&b), "game");
        assert_eq!(name_from_wide(&[]), "");
        let full: Vec<u16> = "abc".encode_utf16().collect();
        assert_eq!(name_from_wide(&full), "abc", "no NUL: the whole buffer");
    }
}
