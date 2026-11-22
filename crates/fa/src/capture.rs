//! `fa capture --seconds N [--frames-csv FILE] [--allow-game]`: a developer command that runs the ETW session for a
//! while and prints the frames, the counters and the losses. It needs an ELEVATED terminal (ETW) and, by default,
//! refuses to start while a matrix game runs: the anti-cheat verdict for ETW observation is still `unverified`
//! (docs/compliance.md), so only the user may decide to run it next to a game.

use fa_capture::capture::Capture;
use fa_capture::frames::{Frame, FrameExtractor, FrameSource};
use fa_capture::modules::ModuleMap;
use fa_capture::processes::ProcessTable;
use fa_capture::record::{ColdRecord, DpcKind, HotRecord};
use fa_core::clock::{ticks_to_ns, Timebase};
use fa_core::matrix::Matrix;
use fa_probe::health::fr_num;
use fa_probe::plan::running_games;
use fa_sync::spsc::{ring, Consumer};
use fa_win::etw::Session;
use fa_win::{clock, console, procs, windows};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

pub struct Args {
    seconds: u64,
    frames_csv: Option<PathBuf>,
    allow_game: bool,
}

pub fn parse(args: &[String]) -> Option<Args> {
    let mut a = Args {
        seconds: 10,
        frames_csv: None,
        allow_game: false,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--seconds" => {
                a.seconds = it.next()?.parse().ok().filter(|s| (1..=3600).contains(s))?
            }
            "--frames-csv" => a.frames_csv = Some(PathBuf::from(it.next()?)),
            "--allow-game" => a.allow_game = true,
            _ => return None,
        }
    }
    Some(a)
}

#[derive(Default)]
struct Tally {
    cswitch: u64,
    dpc: u64,
    isr: u64,
    disk: u64,
    present_events: u64,
    frames: Vec<Frame>,
}

/// More frames than this in one run are counted but not kept (a 1 h run at 240 FPS is about 860 k).
const MAX_FRAMES: usize = 2_000_000;

fn games_running(matrix: &Matrix) -> io::Result<Vec<(fa_core::matrix::GameId, String)>> {
    let list = procs::process_list()?;
    let names: std::collections::HashMap<u32, String> = list.iter().cloned().collect();
    let images: Vec<String> = list.into_iter().map(|(_, n)| n).collect();
    let wins: Vec<(String, String)> = windows::visible_windows()
        .into_iter()
        .filter_map(|w| names.get(&w.pid).map(|n| (n.clone(), w.class)))
        .collect();
    Ok(running_games(matrix, &images, &wins))
}

struct Pipes {
    hot: Consumer<HotRecord>,
    cold: Consumer<ColdRecord>,
    extractor: FrameExtractor,
    processes: ProcessTable,
    modules: ModuleMap,
    tally: Tally,
}

impl Pipes {
    fn drain(&mut self) {
        while let Some(r) = self.hot.pop() {
            match r {
                HotRecord::CSwitch { .. } => self.tally.cswitch += 1,
                HotRecord::Dpc {
                    kind: DpcKind::Isr, ..
                } => self.tally.isr += 1,
                HotRecord::Dpc { .. } => self.tally.dpc += 1,
                HotRecord::DiskIo { .. } => self.tally.disk += 1,
                HotRecord::Present { .. } | HotRecord::DxgPresent { .. } => {
                    self.tally.present_events += 1
                }
            }
            if let Some(f) = self.extractor.push(&r) {
                if self.tally.frames.len() < MAX_FRAMES {
                    self.tally.frames.push(f);
                }
            }
        }
        while let Some(r) = self.cold.pop() {
            self.processes.apply(&r);
            self.modules.apply(&r);
        }
    }
}

pub fn run(args: Args) -> ExitCode {
    match run_inner(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fa capture : {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(args: Args) -> io::Result<ExitCode> {
    let _ = console::install_ctrl_c();
    if !args.allow_game {
        let matrix = Matrix::embedded().map_err(io::Error::other)?;
        let running = games_running(&matrix)?;
        if !running.is_empty() {
            let names: Vec<&str> = running.iter().map(|(_, n)| n.as_str()).collect();
            eprintln!(
                "fa capture ne se lance pas pendant un jeu : {} est en cours. La compatibilité de l'observation ETW avec \
                 l'anti-triche n'est pas vérifiée : fermez le jeu, ou relancez avec --allow-game si vous décidez de \
                 prendre ce risque.",
                names.join(", ")
            );
            return Ok(ExitCode::from(3));
        }
    }

    let (hot_p, hot_c) = ring(1 << 16);
    let (cold_p, cold_c) = ring(1 << 12);
    let mut capture = Capture::new(hot_p, cold_p);
    let stats = capture.shared_stats();
    let (tsc0, qpc0) = clock::read_pair();
    let session = Session::start(move |v| capture.on_event(v))?;
    let timebase = session.timebase();
    let qpc_hz = clock::qpc_freq();
    let tick_hz = match timebase {
        Timebase::Tsc => clock::Clock::new().hz(),
        Timebase::Qpc => qpc_hz,
    };
    println!(
        "Capture ETW en cours ({} s), Ctrl+C pour arrêter plus tôt…",
        args.seconds
    );

    let mut pipes = Pipes {
        hot: hot_c,
        cold: cold_c,
        extractor: FrameExtractor::new(tick_hz.get()),
        processes: ProcessTable::default(),
        modules: ModuleMap::new(true),
        tally: Tally::default(),
    };
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(args.seconds) && !console::ctrl_c_requested() {
        pipes.drain();
        std::thread::sleep(Duration::from_millis(20));
    }
    let elapsed = start.elapsed();
    let losses = session.losses();
    let panics = session.handler_panics();
    drop(session);
    pipes.drain();
    let s = stats.snapshot();

    println!(
        "\nCapture terminée : {} s, base de temps : {} (ticks bruts).",
        fr_num(elapsed.as_secs_f64(), 1),
        match timebase {
            Timebase::Tsc => "TSC",
            Timebase::Qpc => "QPC (mode dégradé : le TSC n'est pas invariant)",
        }
    );
    println!(
        "Événements : {} reçus ; chauds {}, froids {} ; ignorés {}, version inconnue {}, mal formés {}.",
        s.events, s.hot, s.cold, s.ignored, s.unknown_version, s.malformed
    );
    let (el, lb, rb) = losses.map_or((0, 0, 0), |l| {
        (l.events_lost, l.log_buffers_lost, l.realtime_buffers_lost)
    });
    println!(
        "Pertes : file chaude {}, file froide {} ; ETW : événements {el}, tampons {lb} + {rb} ; paniques du gestionnaire {panics}.",
        s.hot_dropped, s.cold_dropped
    );
    let t = &pipes.tally;
    println!(
        "Noyau : {} changements de contexte, {} DPC, {} ISR, {} E/S disque. {} événements de présentation. \
         {} modules noyau connus, {} processus nommés ({} noms refusés).",
        t.cswitch,
        t.dpc,
        t.isr,
        t.disk,
        t.present_events,
        pipes.modules.len(),
        count_names(&pipes.processes, t),
        pipes.processes.refused_names()
    );
    report_frames(&pipes, tick_hz);
    if let Some(path) = &args.frames_csv {
        write_csv(
            path,
            &pipes.tally.frames,
            (tsc0, qpc0),
            timebase,
            tick_hz,
            qpc_hz,
        )?;
        println!("Frames écrites dans {}.", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn count_names(p: &ProcessTable, t: &Tally) -> usize {
    let mut pids: Vec<u32> = t.frames.iter().map(|f| f.pid).collect();
    pids.sort_unstable();
    pids.dedup();
    pids.into_iter()
        .filter(|pid| p.name_of(*pid).is_some())
        .count()
}

fn ms(ticks: u64, hz: NonZeroU64) -> f64 {
    ticks_to_ns(ticks, hz) as f64 / 1e6
}

fn report_frames(pipes: &Pipes, hz: NonZeroU64) {
    let mut groups: BTreeMap<(u32, u64, bool), Vec<u64>> = BTreeMap::new();
    for f in &pipes.tally.frames {
        groups
            .entry((f.pid, f.swap_chain, f.source == FrameSource::DxgKrnl))
            .or_default()
            .push(f.frametime);
    }
    println!("\nFrames par processus et chaîne d'échange (au moins 10 frames) :");
    let mut shown = 0;
    for ((pid, chain, krnl), mut times) in groups {
        if times.len() < 10 {
            continue;
        }
        shown += 1;
        times.sort_unstable();
        let total: u64 = times.iter().sum();
        let p99 = times[(times.len() * 99 / 100).min(times.len() - 1)];
        let name = pipes.processes.name_of(pid).unwrap_or("nom inconnu");
        println!(
            "  {name} (pid {pid}, chaîne {chain:#x}{}) : {} frames, {} FPS moyen, frametime moyen {} ms, p99 {} ms ({} FPS), max {} ms",
            if krnl { ", via DxgKrnl" } else { "" },
            times.len(),
            fr_num(times.len() as f64 / (total as f64 / hz.get() as f64), 1),
            fr_num(ms(total / times.len() as u64, hz), 2),
            fr_num(ms(p99, hz), 2),
            fr_num(1000.0 / ms(p99, hz).max(1e-6), 1),
            fr_num(ms(*times.last().unwrap_or(&0), hz), 2),
        );
    }
    if shown == 0 {
        println!("  aucune (aucune application n'a présenté assez de frames pendant la capture).");
    }
}

/// `ts_qpc` maps the TSC timestamp onto the QPC clock with a (TSC, QPC) pair taken at start, so the frames can be
/// matched with another tool's QPC timestamps (PresentMon).
fn write_csv(
    path: &PathBuf,
    frames: &[Frame],
    pair: (u64, u64),
    timebase: Timebase,
    hz: NonZeroU64,
    qpc_hz: NonZeroU64,
) -> io::Result<()> {
    let mut out = io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(
        out,
        "# tick_hz={} qpc_hz={} timebase={timebase:?}",
        hz.get(),
        qpc_hz.get()
    )?;
    writeln!(
        out,
        "pid,swap_chain,source,ts_ticks,ts_qpc,frametime_ticks,frametime_ms"
    )?;
    for f in frames {
        let qpc = match timebase {
            Timebase::Qpc => f.ts as i128,
            Timebase::Tsc => {
                let d = i128::from(f.ts) - i128::from(pair.0);
                i128::from(pair.1) + d * i128::from(qpc_hz.get()) / i128::from(hz.get())
            }
        };
        writeln!(
            out,
            "{},{:#x},{},{},{qpc},{},{:.4}",
            f.pid,
            f.swap_chain,
            if f.source == FrameSource::Dxgi {
                "dxgi"
            } else {
                "dxgkrnl"
            },
            f.ts,
            f.frametime,
            ms(f.frametime, hz)
        )?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn defaults_and_options_are_parsed() {
        let a = parse(&args(&[])).unwrap();
        assert_eq!(
            (a.seconds, a.frames_csv.is_none(), a.allow_game),
            (10, true, false)
        );
        let a = parse(&args(&[
            "--seconds",
            "20",
            "--frames-csv",
            "f.csv",
            "--allow-game",
        ]))
        .unwrap();
        assert_eq!((a.seconds, a.allow_game), (20, true));
        assert_eq!(a.frames_csv, Some(PathBuf::from("f.csv")));
    }

    #[test]
    fn bad_arguments_are_refused() {
        for bad in [
            &["--seconds"][..],
            &["--seconds", "0"],
            &["--seconds", "x"],
            &["--seconds", "99999"],
            &["--frames-csv"],
            &["--nope"],
        ] {
            assert!(parse(&args(bad)).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_csv_maps_ticks_onto_the_qpc_clock() {
        let dir = std::env::temp_dir().join("fa-capture-csv-test.csv");
        let hz = NonZeroU64::new(2_000_000_000).unwrap();
        let qpc_hz = NonZeroU64::new(10_000_000).unwrap();
        let f = Frame {
            ts: 4_000_000_000,
            pid: 7,
            swap_chain: 0x10,
            frametime: 33_333_333,
            sync_interval: 1,
            source: FrameSource::Dxgi,
        };
        write_csv(
            &dir,
            &[f],
            (2_000_000_000, 1_000),
            Timebase::Tsc,
            hz,
            qpc_hz,
        )
        .unwrap();
        let text = std::fs::read_to_string(&dir).unwrap();
        let _ = std::fs::remove_file(&dir);
        let row = text.lines().nth(2).unwrap();
        // 2e9 TSC ticks at 2 GHz are 1 s, so 10 M QPC ticks after 1000.
        assert_eq!(row, "7,0x10,dxgi,4000000000,10001000,33333333,16.6667");
    }
}
