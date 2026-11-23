//! The Stutter Lab (lot 6): own render loop and known-cause injectors. The scorer, `lab/score.rs`, is lot 10.

mod guards;
mod inject;
// Cause variants and scenario fields are consumed injector by injector (groups 5 and 6).
#[allow(dead_code)]
mod journal;
mod render_loop;
#[allow(dead_code)]
mod scenario;
mod verify;

use render_loop::{Fps, Runner};
use scenario::Cause;
use std::io;
use std::path::PathBuf;

/// Seconds of idle loop measured before a scenario, for the journal header.
const BASELINE_SECONDS: f64 = 3.0;

/// Parsed `fa lab ...` arguments.
pub enum LabArgs {
    /// The render loop alone.
    Loop { fps: Fps, seconds: u64 },
    /// Parse a scenario file and check it against the guards, without running anything.
    Check { file: PathBuf },
    /// Run a scenario and write its ground-truth journal.
    Run {
        file: PathBuf,
        journal: Option<PathBuf>,
    },
    /// Compare a journal with a `fa capture --frames-csv` file.
    Verify { journal: PathBuf, frames: PathBuf },
}

pub fn parse(args: &[String]) -> Option<LabArgs> {
    let [first, rest @ ..] = args else {
        return None;
    };
    match (first.as_str(), rest) {
        ("check", [file]) => Some(LabArgs::Check { file: file.into() }),
        ("verify", [journal, frames]) => Some(LabArgs::Verify {
            journal: journal.into(),
            frames: frames.into(),
        }),
        ("run", [file]) => Some(LabArgs::Run {
            file: file.into(),
            journal: None,
        }),
        ("run", [file, flag, out]) if flag == "--journal" => Some(LabArgs::Run {
            file: file.into(),
            journal: Some(out.into()),
        }),
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

fn load(file: &PathBuf) -> io::Result<scenario::Scenario> {
    let text = std::fs::read_to_string(file)?;
    let s = scenario::Scenario::parse(&text).map_err(io::Error::other)?;
    guards::validate(&s).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(s)
}

/// `%LOCALAPPDATA%\FrameAutopsy\lab\<scenario>-<unix seconds>.jsonl`: a new name per run, nothing is overwritten.
fn default_journal_path(name: &str) -> io::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))?;
    let dir = PathBuf::from(base).join("FrameAutopsy").join("lab");
    std::fs::create_dir_all(&dir)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Ok(dir.join(format!("{name}-{secs}.jsonl")))
}

fn run_scenario(file: PathBuf, journal: Option<PathBuf>) -> io::Result<()> {
    let s = load(&file)?;
    if let Some(inj) = s.injections.iter().find(|i| i.cause != Cause::FrameStall) {
        return Err(io::Error::other(format!(
            "l'injecteur {:?} n'est pas encore implémenté",
            inj.cause
        )));
    }
    guards::ensure_no_game()?;
    fa_win::console::install_ctrl_c()?;
    let fps = Fps::parse(&s.fps).ok_or_else(|| io::Error::other("fps inconnu"))?;
    let mut runner = Runner::open(fps)?;
    println!("Mesure de la gigue de base ({BASELINE_SECONDS} s)...");
    let baseline = runner.run_for(BASELINE_SECONDS, &mut |_, _, _| Ok(()))?;
    let path = match journal {
        Some(p) => p,
        None => default_journal_path(&s.name)?,
    };
    let header = journal::Header::new(
        &s.name,
        fps,
        runner.clock.hz().get(),
        std::process::id(),
        runner.adapter(),
        &baseline,
    );
    let mut log = journal::Journal::create(std::fs::File::create(&path)?, &header)?;
    let mut stalls: Vec<inject::FrameStall> =
        s.injections.iter().map(inject::FrameStall::new).collect();
    println!(
        "Scénario {} ({} s), journal : {}",
        s.name,
        s.duration,
        path.display()
    );
    runner.run_for(s.duration, &mut |clock, elapsed, stage| {
        stalls
            .iter_mut()
            .try_for_each(|st| st.hook(&mut log, clock, elapsed, stage))
    })?;
    println!("Terminé. Processus du Lab : {}.", std::process::id());
    Ok(())
}

pub fn run(a: LabArgs) -> io::Result<()> {
    match a {
        LabArgs::Check { file } => {
            let s = load(&file)?;
            println!(
                "{} : valide ({} injection(s), {} s)",
                s.name,
                s.injections.len(),
                s.duration
            );
            Ok(())
        }
        LabArgs::Run { file, journal } => run_scenario(file, journal),
        LabArgs::Verify { journal, frames } => {
            let ok = verify::run(&journal.to_string_lossy(), &frames.to_string_lossy())?;
            if ok {
                Ok(())
            } else {
                Err(io::Error::other(
                    "la capture ne reproduit pas le stall injecté",
                ))
            }
        }
        LabArgs::Loop { fps, seconds } => {
            guards::ensure_no_game()?;
            fa_win::console::install_ctrl_c()?;
            let r = render_loop::run(fps, seconds)?;
            let h = journal::Header::new(
                "loop",
                fps,
                r.tsc_hz,
                std::process::id(),
                &r.adapter,
                &r.baseline,
            );
            println!("{}", h.line());
            Ok(())
        }
    }
}
