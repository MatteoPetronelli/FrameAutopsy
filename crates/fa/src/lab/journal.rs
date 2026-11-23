//! Ground-truth journal, JSONL. Task 2.3 writes the header line; start and end lines come with group 3.

use super::render_loop::{Baseline, Fps};
use serde::Serialize;

/// First line of a journal: what the run was made of, so a later scorer can tell a stall from the loop's own noise.
#[derive(Serialize)]
pub struct Header<'a> {
    pub kind: &'static str,
    pub scenario: &'a str,
    pub fps: String,
    pub tsc_hz: u64,
    pub adapter: &'a str,
    pub baseline_frames: usize,
    pub baseline_p50_ns: u64,
    pub baseline_p99_ns: u64,
    pub baseline_max_ns: u64,
    pub baseline_jitter_p99_ns: u64,
}

impl<'a> Header<'a> {
    pub fn new(scenario: &'a str, fps: Fps, tsc_hz: u64, adapter: &'a str, b: &Baseline) -> Self {
        Header {
            kind: "header",
            scenario,
            fps: fps.label(),
            tsc_hz,
            adapter,
            baseline_frames: b.frames,
            baseline_p50_ns: b.p50_ns,
            baseline_p99_ns: b.p99_ns,
            baseline_max_ns: b.max_ns,
            baseline_jitter_p99_ns: b.jitter_p99_ns,
        }
    }

    /// One JSON object, no newline.
    pub fn line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_line_is_one_json_object_with_the_baseline_and_an_escaped_adapter() {
        let b = Baseline {
            frames: 3,
            p50_ns: 6_944_444,
            p99_ns: 7_000_000,
            max_ns: 8_000_000,
            jitter_p99_ns: 120_000,
        };
        let line = Header::new(
            "idle",
            Fps::Fixed(144),
            2_688_000_000,
            "NVIDIA \"RTX\" 3060",
            &b,
        )
        .line();
        assert!(!line.contains('\n'));
        assert_eq!(
            line,
            r#"{"kind":"header","scenario":"idle","fps":"144","tsc_hz":2688000000,"adapter":"NVIDIA \"RTX\" 3060","baseline_frames":3,"baseline_p50_ns":6944444,"baseline_p99_ns":7000000,"baseline_max_ns":8000000,"baseline_jitter_p99_ns":120000}"#
        );
    }
}
