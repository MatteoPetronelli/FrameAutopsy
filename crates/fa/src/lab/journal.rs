//! Ground-truth journal, JSONL, in raw TSC ticks (the capture's timebase).
//!
//! Line 1 is the header. Every injection writes a `start` line before it begins and an `end` line after it finished,
//! each flushed at once: a killed run still shows what was active (a `start` without its `end`).

use super::render_loop::{Baseline, Fps};
use super::scenario::Cause;
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

/// First line of a journal: what the run was made of, so a later scorer can tell a stall from the loop's own noise.
#[derive(Serialize)]
pub struct Header<'a> {
    pub kind: &'static str,
    pub scenario: &'a str,
    /// The Lab process, to pick its frames out of a capture.
    pub pid: u32,
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
    pub fn new(
        scenario: &'a str,
        fps: Fps,
        tsc_hz: u64,
        pid: u32,
        adapter: &'a str,
        b: &Baseline,
    ) -> Self {
        Header {
            kind: "header",
            scenario,
            pid,
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

#[derive(Serialize)]
struct Start<'a> {
    kind: &'static str,
    cause: Cause,
    start_tsc: u64,
    params: &'a Value,
}

#[derive(Serialize)]
struct End<'a> {
    kind: &'static str,
    cause: Cause,
    start_tsc: u64,
    end_tsc: u64,
    params: &'a Value,
}

/// The journal writer. `W` is a file in a real run.
pub struct Journal<W: Write> {
    out: W,
}

impl<W: Write> Journal<W> {
    /// Writes the header line.
    pub fn create(out: W, header: &Header) -> io::Result<Self> {
        let mut j = Journal { out };
        j.line(&header.line())?;
        Ok(j)
    }

    fn line(&mut self, text: &str) -> io::Result<()> {
        writeln!(self.out, "{text}")?;
        self.out.flush()
    }

    fn json(&mut self, v: &impl Serialize) -> io::Result<()> {
        let text = serde_json::to_string(v).map_err(io::Error::other)?;
        self.line(&text)
    }

    /// To be called just before the injection starts.
    pub fn start(&mut self, cause: Cause, start_tsc: u64, params: &Value) -> io::Result<()> {
        self.json(&Start {
            kind: "start",
            cause,
            start_tsc,
            params,
        })
    }

    /// To be called once the injection has stopped; repeats the start tick so the line stands alone.
    pub fn end(
        &mut self,
        cause: Cause,
        start_tsc: u64,
        end_tsc: u64,
        params: &Value,
    ) -> io::Result<()> {
        self.json(&End {
            kind: "end",
            cause,
            start_tsc,
            end_tsc,
            params,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn baseline() -> Baseline {
        Baseline {
            frames: 3,
            p50_ns: 6_944_444,
            p99_ns: 7_000_000,
            max_ns: 8_000_000,
            jitter_p99_ns: 120_000,
        }
    }

    #[test]
    fn the_header_line_is_one_json_object_with_the_baseline_and_an_escaped_adapter() {
        let line = Header::new(
            "idle",
            Fps::Fixed(144),
            2_688_000_000,
            4242,
            "NVIDIA \"RTX\" 3060",
            &baseline(),
        )
        .line();
        assert!(!line.contains('\n'));
        assert_eq!(
            line,
            r#"{"kind":"header","scenario":"idle","pid":4242,"fps":"144","tsc_hz":2688000000,"adapter":"NVIDIA \"RTX\" 3060","baseline_frames":3,"baseline_p50_ns":6944444,"baseline_p99_ns":7000000,"baseline_max_ns":8000000,"baseline_jitter_p99_ns":120000}"#
        );
    }

    #[test]
    fn a_whole_run_matches_the_golden_file() {
        let header = Header::new(
            "golden",
            Fps::Fixed(144),
            2_688_000_000,
            4242,
            "GPU",
            &baseline(),
        );
        let mut j = Journal::create(Vec::new(), &header).unwrap();
        let params = json!({"cpus": [2, 4], "intensity": 1.0});
        j.start(Cause::CpuBurn, 1000, &params).unwrap();
        j.end(Cause::CpuBurn, 1000, 2_688_001_000, &params).unwrap();
        let written = String::from_utf8(j.out).unwrap();
        let golden = include_str!("testdata/journal.jsonl").replace("\r\n", "\n");
        assert_eq!(written, golden);
    }

    #[test]
    fn a_killed_run_leaves_a_start_line_without_an_end_line() {
        let path =
            std::env::temp_dir().join(format!("fa-lab-journal-{}.jsonl", std::process::id()));
        let header = Header::new("killed", Fps::Uncapped, 1, 4242, "GPU", &baseline());
        let mut j = Journal::create(std::fs::File::create(&path).unwrap(), &header).unwrap();
        j.start(Cause::DiskBurst, 42, &json!({"size_mib": 512}))
            .unwrap();
        // The journal is neither ended nor dropped: this is what a killed process leaves on disk.
        let text = std::fs::read_to_string(&path).unwrap();
        let kinds: Vec<&str> = text
            .lines()
            .map(|l| {
                if l.contains(r#""kind":"start""#) {
                    "start"
                } else if l.contains(r#""kind":"end""#) {
                    "end"
                } else {
                    "header"
                }
            })
            .collect();
        assert_eq!(kinds, ["header", "start"]);
        drop(j);
        let _ = std::fs::remove_file(&path);
    }
}
