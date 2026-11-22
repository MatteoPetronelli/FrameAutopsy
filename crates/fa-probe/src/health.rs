//! The data of one `fa check` run. One struct, rendered twice (French text for the terminal, JSON for the file and the
//! baseline). The JSON keeps numbers, not translated strings, so later lots can compare runs.
//!
//! Everything derives `deny_unknown_fields`: the baseline is a user-writable file and is read like untrusted input.

use crate::sysinfo::{GpuInfo, MemoryModule};
use crate::wake::WakeStats;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthReport {
    pub schema_version: u32,
    /// UTC, `YYYYMMDDTHHMMSSZ`.
    pub timestamp: String,
    pub cpu: CpuInfo,
    pub frequency: FrequencyInfo,
    pub memory: MemoryInfo,
    pub scheduling: Option<WakeStats>,
    pub power: PowerInfo,
    pub gpu: Option<GpuInfo>,
    pub verdicts: Vec<Verdict>,
    /// What was checked, in the order of the report (printed when nothing is found).
    pub checks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuInfo {
    pub model: String,
    pub physical_cores: u32,
    pub logical_cpus: u32,
    pub invariant_tsc: bool,
    pub tsc_hz: u64,
    pub avx2: bool,
    pub avx512: bool,
    pub waitpkg: bool,
    pub mwaitx: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrequencyInfo {
    /// ISA of the FMA load ("AVX-512", "AVX2" or "scalaire").
    pub load_isa: String,
    /// Peak clock of each core in turn with one core busy: `(logical CPU, GHz)`.
    pub single_core_peak_ghz: Vec<(u32, f64)>,
    /// Logical CPU whose clock is sampled once per second during the load.
    pub sampler_cpu: u32,
    /// Logical CPUs running the load (one per other physical core; the sampler's core and its SMT sibling excluded).
    pub load_cpus: Vec<u32>,
    /// Median clock of the sampler CPU for each second of the load, in GHz.
    pub curve_ghz: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CachePoint {
    pub kib: u64,
    pub ns_per_load: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheLine {
    pub level: u8,
    pub topology_kib: u64,
    pub detected_kib: Option<u64>,
    pub agrees: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryInfo {
    /// "4 Kio" or "2 Mio" (pages used by the latency chase).
    pub page_size: String,
    pub latency_curve: Vec<CachePoint>,
    pub cache: Vec<CacheLine>,
    pub bandwidth_threads: u32,
    pub read_gbs: Option<f64>,
    pub write_gbs: Option<f64>,
    pub copy_gbs: Option<f64>,
    pub modules: Option<Vec<MemoryModule>>,
    pub modules_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerInfo {
    /// "ac", "battery" or "unknown".
    pub source: String,
    pub scheme: String,
    pub overlay: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub id: String,
    pub fired: bool,
    pub confidence: Confidence,
    /// The numbers behind the verdict, with the rule thresholds.
    pub evidence: BTreeMap<String, f64>,
    /// The French sentence.
    pub text: String,
}
/// Test fixtures: two complete reports, a healthy desktop-like run and a throttling laptop.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::verdict::evaluate;

    fn cpu() -> CpuInfo {
        CpuInfo {
            model: "Intel(R) Core(TM) i5-11400H @ 2.70GHz".into(),
            physical_cores: 6,
            logical_cpus: 12,
            invariant_tsc: true,
            tsc_hz: 2_688_000_000,
            avx2: true,
            avx512: true,
            waitpkg: false,
            mwaitx: false,
        }
    }

    fn module() -> MemoryModule {
        MemoryModule {
            capacity_bytes: 16 << 30,
            configured_speed: Some(3200),
            rated_speed: Some(3200),
            bank: "BANK 0".into(),
        }
    }

    fn memory(modules: Vec<MemoryModule>, read: f64) -> MemoryInfo {
        let p = |kib, ns| CachePoint {
            kib,
            ns_per_load: ns,
        };
        MemoryInfo {
            page_size: "4 Kio".into(),
            latency_curve: vec![p(32, 1.25), p(512, 4.4), p(65_536, 108.0)],
            cache: vec![
                CacheLine {
                    level: 1,
                    topology_kib: 48,
                    detected_kib: Some(48),
                    agrees: true,
                },
                CacheLine {
                    level: 3,
                    topology_kib: 12_288,
                    detected_kib: Some(1_024),
                    agrees: false,
                },
            ],
            bandwidth_threads: 6,
            read_gbs: Some(read),
            write_gbs: Some(30.0),
            copy_gbs: Some(28.0),
            modules: Some(modules),
            modules_error: None,
        }
    }

    fn base(
        timestamp: &str,
        curve: Vec<f64>,
        modules: Vec<MemoryModule>,
        read: f64,
        wake: WakeStats,
    ) -> HealthReport {
        let verdicts = evaluate(&curve, false, Some(&modules), Some(read), Some(&wake));
        HealthReport {
            schema_version: SCHEMA_VERSION,
            timestamp: timestamp.into(),
            cpu: cpu(),
            frequency: FrequencyInfo {
                load_isa: "AVX-512".into(),
                single_core_peak_ghz: vec![(0, 4.19), (2, 4.31), (4, 4.29)],
                sampler_cpu: 4,
                load_cpus: vec![0, 2, 6, 8, 10],
                curve_ghz: curve,
            },
            memory: memory(modules, read),
            scheduling: Some(wake),
            power: PowerInfo {
                source: "ac".into(),
                scheme: "Consigliati da HP".into(),
                overlay: Some("{00000000-0000-0000-0000-000000000000}".into()),
            },
            gpu: Some(GpuInfo {
                name: "NVIDIA GeForce RTX 3060 Laptop GPU".into(),
                performance_state: "P8".into(),
                power_limit_watts: None,
            }),
            verdicts,
            checks: vec![
                "fréquence : pic mono-cœur et courbe sous charge FMA de 60 s".into(),
                "mémoire : latence, débit, tailles de cache, modules WMI".into(),
                "ordonnancement : latence de réveil du timer haute résolution".into(),
                "alimentation : source, plan, mode".into(),
            ],
        }
    }

    pub fn healthy() -> HealthReport {
        let wake = WakeStats {
            n: 2000,
            p50_us: 520.0,
            p99_us: 2_500.0,
            max_us: 11_000.0,
        };
        base(
            "20261122T101500Z",
            vec![4.09; 60],
            vec![module(), module()],
            32.0,
            wake,
        )
    }

    pub fn throttling_laptop() -> HealthReport {
        let mut curve = vec![4.2; 25];
        curve.extend(vec![3.1; 35]);
        let wake = WakeStats {
            n: 2000,
            p50_us: 600.0,
            p99_us: 7_200.0,
            max_us: 30_000.0,
        };
        base("20261122T103000Z", curve, vec![module()], 22.0, wake)
    }
}
