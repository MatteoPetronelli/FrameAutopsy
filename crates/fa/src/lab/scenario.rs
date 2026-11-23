//! Lab scenarios: TOML files in `lab/scenarios/` (design D-4). Parsing rejects unknown fields; the guard caps are
//! checked by `guards::validate` before anything starts.

use serde::{Deserialize, Serialize};

/// The nine injectable causes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cause {
    CpuBurn,
    FrameStall,
    CacheThrash,
    TlbThrash,
    MemBandwidth,
    PageFaultStorm,
    DiskBurst,
    TimerResolution,
    PriorityContention,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Injection {
    pub cause: Cause,
    /// Seconds after the scenario starts.
    pub start: f64,
    /// Seconds.
    pub duration: f64,
    /// 0.0 to 1.0, where the cause has a notion of intensity.
    pub intensity: Option<f64>,
    pub cpus: Option<Vec<u32>>,
    /// Disk burst size in MiB.
    pub size_mib: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    /// `vsync`, `uncapped` or a rate in frames per second (same words as `fa lab loop --fps`).
    pub fps: String,
    /// Seconds.
    pub duration: f64,
    #[serde(default, rename = "injection")]
    pub injections: Vec<Injection>,
    pub expected_causes: Vec<Cause>,
}

impl Scenario {
    pub fn parse(text: &str) -> Result<Scenario, toml::de::Error> {
        toml::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::guards::{validate, GuardError};

    const VALID: &str = r#"
name = "cpu-burn-on-render"
fps = "144"
duration = 20.0
expected_causes = ["cpu-burn"]

[[injection]]
cause = "cpu-burn"
start = 5.0
duration = 10.0
intensity = 1.0
cpus = [2, 4]
"#;

    #[test]
    fn a_valid_scenario_parses_and_passes_the_guards() {
        let s = Scenario::parse(VALID).unwrap();
        assert_eq!(s.injections[0].cause, Cause::CpuBurn);
        assert_eq!(s.expected_causes, [Cause::CpuBurn]);
        validate(&s).unwrap();
    }

    #[test]
    fn an_unknown_field_or_cause_is_an_error() {
        let text = VALID.replace("intensity = 1.0", "intensity = 1.0\nturbo = true");
        assert!(Scenario::parse(&text)
            .unwrap_err()
            .to_string()
            .contains("turbo"));
        assert!(Scenario::parse(&VALID.replace("[\"cpu-burn\"]", "[\"gpu-burn\"]")).is_err());
    }

    #[test]
    fn an_injection_over_30_seconds_is_refused() {
        let text = VALID
            .replace("duration = 10.0", "duration = 31.0")
            .replace("duration = 20.0", "duration = 60.0");
        let s = Scenario::parse(&text).unwrap();
        assert!(matches!(validate(&s), Err(GuardError::TooLong { .. })));
    }
}
