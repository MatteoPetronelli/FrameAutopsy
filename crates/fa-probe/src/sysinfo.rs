//! Safe system queries for `fa check`: memory modules through WMI and the GPU through NVML. Both use safe crates, so no
//! `unsafe` is written here. A failure never fires a verdict: the caller shows "inconnu" and lowers confidence.

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::sync::mpsc;
use std::time::Duration;

/// One physical memory module (a DIMM or soldered chip) as WMI reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryModule {
    pub capacity_bytes: u64,
    /// `ConfiguredClockSpeed`: what the module runs at. WMI documents it as MHz but reports the data rate in MT/s on
    /// DDR; the raw number is kept and used as MT/s (to confirm on the target machine).
    pub configured_speed: Option<u32>,
    /// `Speed`: the rated speed.
    pub rated_speed: Option<u32>,
    pub bank: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_PhysicalMemory", rename_all = "PascalCase")]
struct RawModule {
    capacity: Option<u64>,
    configured_clock_speed: Option<u32>,
    speed: Option<u32>,
    bank_label: Option<String>,
}

fn modules_from_raw(rows: Vec<RawModule>) -> Vec<MemoryModule> {
    rows.into_iter()
        .map(|r| MemoryModule {
            capacity_bytes: r.capacity.unwrap_or(0),
            configured_speed: r.configured_clock_speed.filter(|s| *s > 0),
            rated_speed: r.speed.filter(|s| *s > 0),
            bank: r.bank_label.unwrap_or_default().trim().to_owned(),
        })
        .collect()
}

/// `Win32_PhysicalMemory` on a dedicated thread (COM is initialised per thread by the `wmi` crate), abandoned after
/// `timeout`. A hung query leaves its thread behind until the process exits; `fa check` is short-lived, so that is fine.
pub fn memory_modules(timeout: Duration) -> Result<Vec<MemoryModule>, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = wmi::WMIConnection::new()
            .and_then(|con| con.query::<RawModule>())
            .map(modules_from_raw)
            .map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => r,
        Err(_) => Err(format!("WMI did not answer within {} s", timeout.as_secs())),
    }
}

/// Theoretical bandwidth of `channels` 64-bit channels at `mt_per_s` mega-transfers per second, in GB/s (1e9 bytes).
pub fn theoretical_gbs(mt_per_s: u32, channels: u32) -> f64 {
    mt_per_s as f64 * 8.0 * channels as f64 / 1000.0
}

/// What NVML says about the first GPU.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuInfo {
    pub name: String,
    /// "P0".."P15" or "inconnu".
    pub performance_state: String,
    pub power_limit_watts: Option<f64>,
}

fn pstate_text(p: nvml_wrapper::enum_wrappers::device::PerformanceState) -> String {
    use nvml_wrapper::enum_wrappers::device::PerformanceState as P;
    match p {
        P::Zero => "P0",
        P::One => "P1",
        P::Two => "P2",
        P::Three => "P3",
        P::Four => "P4",
        P::Five => "P5",
        P::Six => "P6",
        P::Seven => "P7",
        P::Eight => "P8",
        P::Nine => "P9",
        P::Ten => "P10",
        P::Eleven => "P11",
        P::Twelve => "P12",
        P::Thirteen => "P13",
        P::Fourteen => "P14",
        P::Fifteen => "P15",
        P::Unknown => "inconnu",
    }
    .to_owned()
}

/// Reads name, performance state and power limit of GPU 0 once, read-only. `None` when `nvml.dll` is absent, fails to
/// initialise or there is no GPU. On an Optimus laptop this call may wake a sleeping GPU: the report says so.
/// `lib` overrides the library path (tests); `None` uses the system one.
pub fn gpu(lib: Option<&OsStr>) -> Option<GpuInfo> {
    let nvml = match lib {
        Some(p) => nvml_wrapper::Nvml::builder().lib_path(p).init().ok()?,
        None => nvml_wrapper::Nvml::init().ok()?,
    };
    let dev = nvml.device_by_index(0).ok()?;
    Some(GpuInfo {
        name: dev.name().ok()?,
        performance_state: dev
            .performance_state()
            .map(pstate_text)
            .unwrap_or_else(|_| "inconnu".to_owned()),
        power_limit_watts: dev
            .power_management_limit()
            .ok()
            .map(|mw| mw as f64 / 1000.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(
        cap: Option<u64>,
        conf: Option<u32>,
        speed: Option<u32>,
        bank: Option<&str>,
    ) -> RawModule {
        RawModule {
            capacity: cap,
            configured_clock_speed: conf,
            speed,
            bank_label: bank.map(str::to_owned),
        }
    }

    #[test]
    fn raw_rows_become_modules_and_bad_fields_become_none_or_zero() {
        let m = modules_from_raw(vec![
            raw(Some(17179869184), Some(3200), Some(3200), Some("BANK 0 ")),
            raw(None, Some(0), None, None),
        ]);
        assert_eq!(m[0].capacity_bytes, 16 << 30);
        assert_eq!(
            (m[0].configured_speed, m[0].rated_speed),
            (Some(3200), Some(3200))
        );
        assert_eq!(m[0].bank, "BANK 0");
        assert_eq!(m[1].capacity_bytes, 0);
        assert_eq!((m[1].configured_speed, m[1].rated_speed), (None, None));
        assert_eq!(m[1].bank, "");
    }

    #[test]
    fn dual_channel_ddr4_3200_is_51_point_2_gbs() {
        assert!((theoretical_gbs(3200, 2) - 51.2).abs() < 1e-9);
        assert!((theoretical_gbs(3200, 1) - 25.6).abs() < 1e-9);
    }

    #[test]
    fn a_missing_nvml_library_means_no_gpu_section() {
        assert_eq!(gpu(Some(OsStr::new("definitely_not_nvml.dll"))), None);
    }

    #[test]
    fn the_real_wmi_query_answers_with_at_least_one_module() {
        let m = memory_modules(Duration::from_secs(3)).unwrap();
        println!("{m:?}");
        assert!(!m.is_empty());
        assert!(m.iter().all(|x| x.capacity_bytes > 0));
    }

    #[test]
    fn the_real_gpu_read_either_answers_or_is_absent() {
        println!("{:?}", gpu(None));
    }
}
