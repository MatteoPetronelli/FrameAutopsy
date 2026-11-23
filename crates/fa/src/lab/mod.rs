//! The Stutter Lab (lot 6): own render loop and known-cause injectors. The scorer, `lab/score.rs`, is lot 10.

mod guards;
// The journal writer, the scenario fields and the temp-file guard are driven by `fa lab run` (group 4 on).
#[allow(dead_code)]
mod journal;
mod render_loop;
#[allow(dead_code)]
mod scenario;

use render_loop::Fps;
use std::path::PathBuf;

/// Parsed `fa lab ...` arguments.
pub enum LabArgs {
    /// The render loop alone.
    Loop { fps: Fps, seconds: u64 },
    /// Parse a scenario file and check it against the guards, without running anything.
    Check { file: PathBuf },
}

pub fn parse(args: &[String]) -> Option<LabArgs> {
    let [first, rest @ ..] = args else {
        return None;
    };
    match (first.as_str(), rest) {
        ("check", [file]) => Some(LabArgs::Check { file: file.into() }),
        ("loop", rest) => {
            let (mut fps, mut seconds) = (Fps::Fixed(60), 10);
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                match (flag.as_str(), it.next()) {
                    ("--fps", Some(v)) => fps = Fps::parse(v)?,
                    ("--seconds", Some(v)) => {
                        seconds = v.parse().ok().filter(|s| (1..=3600).contains(s))?
                    }
                    _ => return None,
                }
            }
            Some(LabArgs::Loop { fps, seconds })
        }
        _ => None,
    }
}

pub fn run(a: LabArgs) -> std::io::Result<()> {
    match a {
        LabArgs::Check { file } => {
            let text = std::fs::read_to_string(&file)?;
            let s = scenario::Scenario::parse(&text).map_err(std::io::Error::other)?;
            guards::validate(&s).map_err(|e| std::io::Error::other(e.to_string()))?;
            println!(
                "{} : valide ({} injection(s), {} s)",
                s.name,
                s.injections.len(),
                s.duration
            );
            Ok(())
        }
        LabArgs::Loop { fps, seconds } => {
            guards::ensure_no_game()?;
            fa_win::console::install_ctrl_c()?;
            let r = render_loop::run(fps, seconds)?;
            let h = journal::Header::new("loop", fps, r.tsc_hz, &r.adapter, &r.baseline);
            println!("{}", h.line());
            Ok(())
        }
    }
}
