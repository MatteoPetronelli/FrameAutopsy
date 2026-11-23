//! `fa lab verify JOURNAL FRAMES.csv`: does the lot 5 capture see the stall the Lab injected, at the exact frame?
//!
//! The journal and the capture share the raw TSC timebase. For each `frame-stall` the frame is the first frame of the
//! Lab process presented at or after the end of the stall; its frame time must equal the injected stall within the
//! jitter budget (1.6 ms at 144 FPS, `docs/decisions.md` section 5). Only meaningful for a stall longer than one frame.

use serde_json::Value;

/// Frozen budget, in milliseconds (decision of 2026-11-23).
pub const BUDGET_MS: f64 = 1.6;

pub struct Journal {
    pub pid: u64,
    pub tsc_hz: f64,
    /// `(start_tsc, end_tsc)` of every finished frame-stall.
    pub stalls: Vec<(u64, u64)>,
}

pub fn parse_journal(text: &str) -> Option<Journal> {
    let mut pid = None;
    let mut tsc_hz = None;
    let mut stalls = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).ok()?;
        match v["kind"].as_str()? {
            "header" => {
                pid = v["pid"].as_u64();
                tsc_hz = v["tsc_hz"].as_f64();
            }
            "end" if v["cause"] == "frame-stall" => {
                stalls.push((v["start_tsc"].as_u64()?, v["end_tsc"].as_u64()?));
            }
            _ => {}
        }
    }
    Some(Journal {
        pid: pid?,
        tsc_hz: tsc_hz?,
        stalls,
    })
}

/// `(ts_ticks, frametime_ms)` of the frames of `pid` in a `fa capture --frames-csv` file.
pub fn parse_frames(text: &str, pid: u64) -> Vec<(u64, f64)> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("pid,"))
        .filter_map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            if c.len() < 7 || c[0].parse::<u64>().ok()? != pid {
                return None;
            }
            Some((c[3].parse().ok()?, c[6].parse().ok()?))
        })
        .collect()
}

#[derive(Debug, PartialEq)]
pub struct Check {
    pub injected_ms: f64,
    pub frame_ms: Option<f64>,
    pub ok: bool,
}

pub fn check(j: &Journal, frames: &[(u64, f64)], budget_ms: f64) -> Vec<Check> {
    j.stalls
        .iter()
        .map(|&(start, end)| {
            let injected_ms = (end - start) as f64 * 1000.0 / j.tsc_hz;
            let frame_ms = frames
                .iter()
                .filter(|(ts, _)| *ts >= end)
                .min_by_key(|(ts, _)| *ts)
                .map(|&(_, ft)| ft);
            let ok = frame_ms.is_some_and(|ft| (ft - injected_ms).abs() <= budget_ms);
            Check {
                injected_ms,
                frame_ms,
                ok,
            }
        })
        .collect()
}

pub fn run(journal: &str, frames: &str) -> std::io::Result<bool> {
    let j = parse_journal(&std::fs::read_to_string(journal)?)
        .ok_or_else(|| std::io::Error::other("journal illisible ou sans en-tête"))?;
    let frames = parse_frames(&std::fs::read_to_string(frames)?, j.pid);
    println!(
        "{} stall(s) dans le journal, {} frame(s) du processus {} dans la capture",
        j.stalls.len(),
        frames.len(),
        j.pid
    );
    let results = check(&j, &frames, BUDGET_MS);
    for (i, c) in results.iter().enumerate() {
        match c.frame_ms {
            Some(ft) => println!(
                "stall {i} : injecté {:.3} ms, frame capturée {ft:.3} ms, écart {:+.3} ms ({})",
                c.injected_ms,
                ft - c.injected_ms,
                if c.ok { "ok" } else { "HORS BUDGET" }
            ),
            None => println!("stall {i} : aucune frame capturée après la fin du stall"),
        }
    }
    Ok(!results.is_empty() && results.iter().all(|c| c.ok))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOURNAL: &str = concat!(
        r#"{"kind":"header","scenario":"s","pid":4242,"fps":"144","tsc_hz":1000000000}"#,
        "\n",
        r#"{"kind":"start","cause":"frame-stall","start_tsc":1000000,"params":{}}"#,
        "\n",
        r#"{"kind":"end","cause":"frame-stall","start_tsc":1000000,"end_tsc":51000000,"params":{}}"#,
        "\n",
    );

    fn csv(frametime_ms: &str) -> String {
        format!(
            "# tick_hz=1000000000 qpc_hz=10000000 timebase=Tsc\n\
             pid,swap_chain,source,ts_ticks,ts_qpc,frametime_ticks,frametime_ms\n\
             4242,0x1,dxgi,40000000,0,6944444,6.9444\n\
             9999,0x2,dxgi,52000000,0,1,0.0001\n\
             4242,0x1,dxgi,52000000,0,50500000,{frametime_ms}\n\
             4242,0x1,dxgi,59000000,0,7000000,7.0000\n"
        )
    }

    #[test]
    fn the_frame_after_the_stall_must_match_the_injected_duration() {
        let j = parse_journal(JOURNAL).unwrap();
        assert_eq!((j.pid, j.stalls.len()), (4242, 1));
        let frames = parse_frames(&csv("50.5000"), 4242);
        assert_eq!(frames.len(), 3, "the other process is ignored");
        let r = check(&j, &frames, BUDGET_MS);
        assert!(r[0].ok, "injected 50 ms, captured 50.5 ms: {r:?}");
        // 1.7 ms off is outside the 1.6 ms budget; 1.5 ms is inside.
        assert!(!check(&j, &parse_frames(&csv("51.7000"), 4242), BUDGET_MS)[0].ok);
        assert!(check(&j, &parse_frames(&csv("51.5000"), 4242), BUDGET_MS)[0].ok);
        // A frame shorter than the stall is wrong too, not only a longer one.
        assert!(!check(&j, &parse_frames(&csv("48.0000"), 4242), BUDGET_MS)[0].ok);
    }

    #[test]
    fn a_stall_with_no_frame_after_it_fails() {
        let j = parse_journal(JOURNAL).unwrap();
        let r = check(&j, &[(1, 7.0)], BUDGET_MS);
        assert_eq!(r[0].frame_ms, None);
        assert!(!r[0].ok);
    }
}
