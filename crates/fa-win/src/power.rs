//! WIN-10 (power): read-only power state for `fa check`. Nothing here sets a power plan, an overlay or any setting.

use std::io;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS};
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, PowerGetActiveScheme, PowerReadFriendlyName, SYSTEM_POWER_STATUS,
};

// `PowerGetEffectiveOverlayScheme` is exported by `powrprof.dll` but is not in `windows-sys` 0.61: declared here.
// Signature taken from the Windows SDK documentation (one out-pointer to a GUID, returns a Win32 error code); verify
// the values against the power mode slider (task 3.2 manual run) before trusting them.
#[link(name = "powrprof", kind = "raw-dylib")]
extern "system" {
    fn PowerGetEffectiveOverlayScheme(effective_overlay_policy_guid: *mut GUID) -> u32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSource {
    Ac,
    Battery,
    Unknown,
}

/// `ACLineStatus`: 0 offline (battery), 1 online (AC), 255 unknown; anything else is unknown.
pub fn source_from_line_status(status: u8) -> PowerSource {
    match status {
        0 => PowerSource::Battery,
        1 => PowerSource::Ac,
        _ => PowerSource::Unknown,
    }
}

/// `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` in lower case.
pub fn guid_string(g: &GUID) -> String {
    let d = g.data4;
    format!(
        "{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        g.data1, g.data2, g.data3, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
    )
}

/// UTF-16 little-endian bytes, NUL-terminated, to `String`; a trailing odd byte is ignored.
fn string_from_wide_bytes(bytes: &[u8]) -> String {
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerState {
    pub source: PowerSource,
    pub scheme_guid: String,
    pub scheme_name: Option<String>,
    /// The effective power mode overlay (the power mode slider). The all-zero GUID means no overlay. Not named here:
    /// the GUID-to-name mapping is not verified.
    pub overlay_guid: Option<String>,
}

fn active_scheme() -> io::Result<GUID> {
    let mut p: *mut GUID = std::ptr::null_mut();
    // SAFETY: a null root key means the current user; `p` is a valid out-pointer that receives a system allocation.
    let st = unsafe { PowerGetActiveScheme(std::ptr::null_mut(), &mut p) };
    if st != ERROR_SUCCESS || p.is_null() {
        return Err(io::Error::from_raw_os_error(st as i32));
    }
    // SAFETY: on success `p` points to a valid GUID allocated for us.
    let g = unsafe { *p };
    // SAFETY: the buffer was allocated by the system with `LocalAlloc` and is freed exactly once; `g` is a copy.
    unsafe { LocalFree(p.cast()) };
    Ok(g)
}

fn friendly_name(scheme: &GUID) -> Option<String> {
    let mut size = 0u32;
    // SAFETY: a null buffer asks for the required size in bytes, written to `size`; `scheme` is a valid GUID.
    let st = unsafe {
        PowerReadFriendlyName(
            std::ptr::null_mut(),
            scheme,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if st != ERROR_SUCCESS || size == 0 || size > 4096 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: `buf` holds `size` bytes, as announced by the first call; `size` is a valid in-out pointer.
    let st = unsafe {
        PowerReadFriendlyName(
            std::ptr::null_mut(),
            scheme,
            std::ptr::null(),
            std::ptr::null(),
            buf.as_mut_ptr(),
            &mut size,
        )
    };
    (st == ERROR_SUCCESS).then(|| string_from_wide_bytes(&buf[..(size as usize).min(buf.len())]))
}

fn overlay() -> Option<GUID> {
    let mut g = GUID {
        data1: 0,
        data2: 0,
        data3: 0,
        data4: [0; 8],
    };
    // SAFETY: `g` is a valid out-pointer for one GUID.
    let st = unsafe { PowerGetEffectiveOverlayScheme(&mut g) };
    (st == ERROR_SUCCESS).then_some(g)
}

/// Reads the power source, the active scheme (GUID and friendly name) and the effective overlay.
pub fn read() -> io::Result<PowerState> {
    // SAFETY: all-zero is a valid `SYSTEM_POWER_STATUS` (plain integers).
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: `s` is a valid out-structure.
    if unsafe { GetSystemPowerStatus(&mut s) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let scheme = active_scheme()?;
    Ok(PowerState {
        source: source_from_line_status(s.ACLineStatus),
        scheme_guid: guid_string(&scheme),
        scheme_name: friendly_name(&scheme),
        overlay_guid: overlay().map(|g| guid_string(&g)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_status_maps_to_a_source() {
        assert_eq!(source_from_line_status(0), PowerSource::Battery);
        assert_eq!(source_from_line_status(1), PowerSource::Ac);
        assert_eq!(source_from_line_status(255), PowerSource::Unknown);
        assert_eq!(source_from_line_status(2), PowerSource::Unknown);
    }

    #[test]
    fn a_guid_prints_in_the_registry_format() {
        let g = GUID {
            data1: 0x381b4222,
            data2: 0xf694,
            data3: 0x41f0,
            data4: [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e],
        };
        assert_eq!(guid_string(&g), "{381b4222-f694-41f0-9685-ff5bb260df2e}");
    }

    #[test]
    fn utf16_bytes_stop_at_the_nul_and_an_odd_trailing_byte_is_ignored() {
        let mut b: Vec<u8> = "Hi".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        b.extend([0, 0, 120, 0]);
        assert_eq!(string_from_wide_bytes(&b), "Hi");
        assert_eq!(string_from_wide_bytes(&[97, 0, 98]), "a");
    }

    #[test]
    fn the_real_state_reads_and_has_a_scheme() {
        let p = read().unwrap();
        assert!(p.scheme_guid.starts_with('{') && p.scheme_guid.len() == 38);
        println!("{p:?}");
    }
}
