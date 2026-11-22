//! Writing the report and the AC baseline, and reading the baseline back as untrusted input.
//!
//! The baseline lives in a user-writable folder, so a reader (the collector, from lot 7 on) must not trust it: size cap,
//! strict schema (`deny_unknown_fields` on every type), schema version check.

use crate::health::{HealthReport, SCHEMA_VERSION};
use crate::render::to_json;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A real report is a few kilobytes; anything above this is refused unread.
pub const BASELINE_MAX_BYTES: u64 = 256 * 1024;

/// `YYYYMMDDTHHMMSSZ` for a Unix time in seconds (proleptic Gregorian calendar, days-from-civil algorithm).
pub fn utc_stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[derive(Debug, PartialEq, Eq)]
pub struct Saved {
    pub json_path: PathBuf,
    pub baseline_updated: bool,
}

/// Writes `data` next to `path` then renames it over `path`, so a reader never sees half a file.
fn write_atomic(path: &Path, data: &str) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// Writes `<dir>\<timestamp>.json`, and `<dir>\baseline.json` too when the run was on AC power. A battery run (or an
/// unknown power source) never touches the baseline.
pub fn save(dir: &Path, r: &HealthReport) -> io::Result<Saved> {
    fs::create_dir_all(dir)?;
    let json = to_json(r);
    let json_path = dir.join(format!("{}.json", r.timestamp));
    write_atomic(&json_path, &json)?;
    let baseline_updated = r.power.source == "ac";
    if baseline_updated {
        write_atomic(&dir.join("baseline.json"), &json)?;
    }
    Ok(Saved {
        json_path,
        baseline_updated,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum BaselineError {
    #[error("baseline is larger than {BASELINE_MAX_BYTES} bytes")]
    TooLarge,
    #[error("baseline does not parse: {0}")]
    Parse(String),
    #[error("baseline schema version {0} is not {SCHEMA_VERSION}")]
    Version(u32),
    #[error("baseline cannot be read: {0}")]
    Io(#[from] io::Error),
}

/// Reads and validates a baseline file.
pub fn read_baseline(path: &Path) -> Result<HealthReport, BaselineError> {
    if fs::metadata(path)?.len() > BASELINE_MAX_BYTES {
        return Err(BaselineError::TooLarge);
    }
    let text = fs::read_to_string(path)?;
    let r: HealthReport =
        serde_json::from_str(&text).map_err(|e| BaselineError::Parse(e.to_string()))?;
    if r.schema_version != SCHEMA_VERSION {
        return Err(BaselineError::Version(r.schema_version));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::fixtures;

    fn temp_dir(tag: &str) -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d = std::env::temp_dir().join(format!("fa-probe-{tag}-{}-{n}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn utc_stamps_match_known_dates() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(1_700_000_000), "20231114T221320Z");
        assert_eq!(utc_stamp(951_782_400), "20000229T000000Z", "leap day");
        assert_eq!(utc_stamp(1_785_000_000), "20260725T172000Z");
    }

    #[test]
    fn an_ac_run_writes_the_report_and_becomes_the_baseline() {
        let dir = temp_dir("ac");
        let r = fixtures::healthy();
        let saved = save(&dir, &r).unwrap();
        assert!(saved.baseline_updated);
        assert!(saved.json_path.ends_with(format!("{}.json", r.timestamp)));
        assert_eq!(read_baseline(&dir.join("baseline.json")).unwrap(), r);
        assert!(!dir.join("baseline.json.tmp").exists(), "no temp file left");
    }

    #[test]
    fn a_battery_run_never_overwrites_the_baseline() {
        let dir = temp_dir("batt");
        let ac = fixtures::healthy();
        save(&dir, &ac).unwrap();
        let mut batt = fixtures::throttling_laptop();
        batt.power.source = "battery".into();
        let saved = save(&dir, &batt).unwrap();
        assert!(!saved.baseline_updated);
        assert!(
            saved.json_path.exists(),
            "the report itself is still written"
        );
        assert_eq!(read_baseline(&dir.join("baseline.json")).unwrap(), ac);
    }

    #[test]
    fn an_unknown_power_source_does_not_write_a_baseline() {
        let dir = temp_dir("unk");
        let mut r = fixtures::healthy();
        r.power.source = "unknown".into();
        assert!(!save(&dir, &r).unwrap().baseline_updated);
        assert!(!dir.join("baseline.json").exists());
    }

    #[test]
    fn an_oversized_baseline_is_refused_unread() {
        let dir = temp_dir("big");
        let p = dir.join("baseline.json");
        fs::write(&p, vec![b' '; BASELINE_MAX_BYTES as usize + 1]).unwrap();
        assert!(matches!(read_baseline(&p), Err(BaselineError::TooLarge)));
    }

    #[test]
    fn malformed_wrong_version_and_unknown_field_baselines_are_refused() {
        let dir = temp_dir("bad");
        let p = dir.join("baseline.json");
        fs::write(&p, "{ not json").unwrap();
        assert!(matches!(read_baseline(&p), Err(BaselineError::Parse(_))));

        let good = to_json(&fixtures::healthy());
        fs::write(
            &p,
            good.replace("\"schema_version\": 1", "\"schema_version\": 2"),
        )
        .unwrap();
        assert!(matches!(read_baseline(&p), Err(BaselineError::Version(2))));

        fs::write(
            &p,
            good.replacen("\"cpu\"", "\"surprise\": 1,\n  \"cpu\"", 1),
        )
        .unwrap();
        assert!(matches!(read_baseline(&p), Err(BaselineError::Parse(_))));

        // an unknown field deep inside a nested type is refused too
        fs::write(
            &p,
            good.replacen("\"model\"", "\"extra\": true, \"model\"", 1),
        )
        .unwrap();
        assert!(matches!(read_baseline(&p), Err(BaselineError::Parse(_))));

        assert!(matches!(
            read_baseline(&dir.join("absent.json")),
            Err(BaselineError::Io(_))
        ));
    }
}
