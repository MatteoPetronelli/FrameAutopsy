//! `fa check`: the machine health check. Non-elevated, read-only (it changes no setting), at most 90 s.
//!
//! Order of the phases: refusal checks, single-core peaks, memory (latency, cache sizes, bandwidth), wake-up latency,
//! then the 60 s FMA load with the clock sampled once per second. The cold phases come first so that our own load
//! threads cannot disturb them; the load comes last and takes whatever the deadline allows.

use fa_asm::fma::{self, Isa};
use fa_core::matrix::Matrix;
use fa_probe::bandwidth::{self, physical_core_cpus, Mode};
use fa_probe::health::{
    CacheLine, CachePoint, CpuInfo, FrequencyInfo, HealthReport, MemoryInfo, PowerInfo,
    SCHEMA_VERSION,
};
use fa_probe::plan::{load_cpus, refusal_message, running_games, Deadline, TOTAL_BUDGET};
use fa_probe::{freq, mem, render, store, sysinfo, verdict, wake};
use fa_win::clock::Clock;
use fa_win::power::{self, PowerSource};
use fa_win::thread::pin_current_thread;
use fa_win::topology::LogicalCpu;
use fa_win::{console, procs, topology, windows};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const LOAD_SECONDS: Duration = Duration::from_secs(60);
const PEAK_BATCH: Duration = Duration::from_millis(600);
/// The cache sweep stops here for `fa check` (the full 256 MiB sweep is `fa probe curve`).
const CURVE_MAX_BYTES: usize = 64 << 20;
const CURVE_RUNS: usize = 2;
const BW_BYTES_PER_THREAD: usize = 16 << 20;
const BW_PASSES: usize = 4;
const WAKE_ROUNDS: usize = 2000;

enum Abort {
    Interrupted,
    Failed(String),
}

impl<E: std::fmt::Display> From<E> for Abort {
    fn from(e: E) -> Abort {
        Abort::Failed(e.to_string())
    }
}

fn check_interrupt() -> Result<(), Abort> {
    if console::ctrl_c_requested() {
        Err(Abort::Interrupted)
    } else {
        Ok(())
    }
}

/// Wall time of each phase, printed at the end (the budgets are frozen from these numbers).
struct Timings(Vec<(&'static str, Duration)>);

impl Timings {
    fn phase<T>(&mut self, name: &'static str, f: impl FnOnce() -> T) -> T {
        let t = Instant::now();
        let r = f();
        self.0.push((name, t.elapsed()));
        r
    }
}

fn isa_name(isa: Isa) -> &'static str {
    match isa {
        Isa::Avx512 => "AVX-512",
        Isa::Avx2 => "AVX2",
        Isa::Scalar => "scalaire",
    }
}

fn load_block(isa: Isa) -> u64 {
    // about 1 ms per block, so the stop flag is honoured within a few milliseconds
    match isa {
        Isa::Avx512 => 400_000,
        Isa::Avx2 => 200_000,
        Isa::Scalar => 2_000,
    }
}

fn health_dir() -> Result<PathBuf, Abort> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(base).join("FrameAutopsy").join("health"))
}

/// `(owner image name, window class)` of every visible window, and the image names, for the refusal check.
fn games_running(matrix: &Matrix) -> Result<Vec<(fa_core::matrix::GameId, String)>, Abort> {
    let list = procs::process_list()?;
    let names: HashMap<u32, String> = list.iter().cloned().collect();
    let images: Vec<String> = list.into_iter().map(|(_, n)| n).collect();
    let wins: Vec<(String, String)> = windows::visible_windows()
        .into_iter()
        .filter_map(|w| names.get(&w.pid).map(|n| (n.clone(), w.class)))
        .collect();
    Ok(running_games(matrix, &images, &wins))
}

pub fn run() -> ExitCode {
    let _ = console::install_ctrl_c();
    match run_inner() {
        Ok(code) => code,
        Err(Abort::Interrupted) => {
            eprintln!("Interrompu : la charge est arrêtée, rien n'est écrit.");
            ExitCode::from(130)
        }
        Err(Abort::Failed(e)) => {
            eprintln!("fa check : {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner() -> Result<ExitCode, Abort> {
    let t0 = Instant::now();
    let deadline = Deadline::new(t0, TOTAL_BUDGET);
    let mut timings = Timings(Vec::new());

    // 1. Refusal while a matrix game runs, then the power warning.
    let matrix = Matrix::embedded()?;
    let running = games_running(&matrix)?;
    if !running.is_empty() {
        eprintln!("{}", refusal_message(&running));
        return Ok(ExitCode::from(3));
    }
    let power = power::read().ok();
    let source = power.as_ref().map_or(PowerSource::Unknown, |p| p.source);
    if source == PowerSource::Battery {
        println!("Attention : la machine est sur batterie. Le test tourne quand même, mais la baseline ne sera pas mise à jour ; branchez le secteur pour une mesure de référence.");
    }
    println!("fa check démarre (environ 75 s). La charge de 60 s sollicite tous les cœurs : la machine va chauffer. Ctrl-C arrête.");

    // WMI and GPU reads run on their own threads while the other phases go on.
    let wmi = std::thread::spawn(|| sysinfo::memory_modules(Duration::from_secs(3)));
    let gpu = std::thread::spawn(|| sysinfo::gpu(None));

    let topo = topology::read()?;
    let cpus: &[LogicalCpu] = &topo.cpus;
    let cores = physical_core_cpus(cpus);
    if cores.is_empty() {
        return Err(Abort::Failed("topologie vide".into()));
    }
    let sampler = cores[cores.len().min(3) - 1];
    let clock = Clock::new();
    let hz = clock.hz();

    // 2. Single-core peaks.
    println!("[1/4] pic de fréquence de chaque cœur...");
    let mut peaks = Vec::new();
    timings.phase("pic mono-coeur", || -> Result<(), Abort> {
        for &cpu in &cores {
            check_interrupt()?;
            if let Some(b) = freq::run_batch(cpu, PEAK_BATCH, hz)? {
                peaks.push((cpu, (b.median_hz / 1e7).round() / 100.0));
            }
        }
        Ok(())
    })?;

    // 3. Memory: latency and cache sizes, then bandwidth.
    println!("[2/4] mémoire : latence, tailles de cache, débit...");
    let (curve, bw) = timings.phase("memoire", || -> Result<_, Abort> {
        check_interrupt()?;
        let curve = mem::cache_curve_with(sampler, false, CURVE_MAX_BYTES, CURVE_RUNS)?;
        check_interrupt()?;
        let kernel = bandwidth::best_kernel();
        let mut bw = [None; 3];
        for (i, mode) in [Mode::Read, Mode::Write, Mode::Copy]
            .into_iter()
            .enumerate()
        {
            bw[i] = bandwidth::run(&cores, mode, kernel, BW_BYTES_PER_THREAD, BW_PASSES).ok();
        }
        Ok((curve, bw))
    })?;
    let pts: Vec<(usize, f64)> = curve.iter().map(|l| (l.bytes, l.ns_per_load)).collect();
    let steps = mem::detect_steps(&pts);
    let cache: Vec<CacheLine> = mem::compare_with_topology(&steps, &topo.caches)
        .into_iter()
        .map(|k| CacheLine {
            level: k.level,
            topology_kib: u64::from(k.topology_bytes) / 1024,
            detected_kib: k.detected_bytes.map(|b| b as u64 / 1024),
            agrees: k.agrees,
        })
        .collect();

    // 4. Scheduling.
    println!("[3/4] latence de réveil...");
    let wake_stats = timings.phase("reveil", || -> Result<_, Abort> {
        check_interrupt()?;
        Ok(wake::run(sampler, Duration::from_millis(1), WAKE_ROUNDS)?)
    })?;

    // 5. The load, last: it takes what the deadline leaves.
    let isa = fma::best_isa();
    let load_dur = deadline.budget(LOAD_SECONDS, Duration::from_secs(2));
    let load_set = load_cpus(cpus, sampler);
    println!(
        "[4/4] charge FMA {} de {} s sur {} CPU, mesure sur le CPU {sampler}...",
        isa_name(isa),
        load_dur.as_secs(),
        load_set.len()
    );
    let curve_ghz = timings.phase("charge", || {
        load_and_sample(isa, &load_set, sampler, load_dur, hz)
    })?;

    // 6. Assemble, evaluate, print, save.
    let modules = wmi.join().ok();
    let (modules, modules_error) = match modules {
        Some(Ok(m)) => (Some(m), None),
        Some(Err(e)) => (None, Some(e)),
        None => (None, Some("thread WMI interrompu".to_owned())),
    };
    let gpu = gpu.join().ok().flatten();
    let read_gbs = bw[0];
    let on_battery = source == PowerSource::Battery;
    let verdicts = verdict::evaluate(
        &curve_ghz,
        on_battery,
        modules.as_deref(),
        read_gbs,
        wake_stats.as_ref(),
    );
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let report = HealthReport {
        schema_version: SCHEMA_VERSION,
        timestamp: store::utc_stamp(ts),
        cpu: CpuInfo {
            model: fa_asm::cpuid::brand_string(),
            physical_cores: cores.len() as u32,
            logical_cpus: cpus.len() as u32,
            invariant_tsc: fa_asm::tsc::invariant_tsc(),
            tsc_hz: hz.get(),
            avx2: std::arch::is_x86_feature_detected!("avx2"),
            avx512: std::arch::is_x86_feature_detected!("avx512f"),
            waitpkg: fa_asm::cpuid::has_waitpkg(),
            mwaitx: fa_asm::cpuid::has_mwaitx(),
        },
        frequency: FrequencyInfo {
            load_isa: isa_name(isa).to_owned(),
            single_core_peak_ghz: peaks,
            sampler_cpu: sampler,
            load_cpus: load_set,
            curve_ghz,
        },
        memory: MemoryInfo {
            page_size: "4 Kio".to_owned(),
            latency_curve: curve
                .iter()
                .map(|l| CachePoint {
                    kib: l.bytes as u64 / 1024,
                    ns_per_load: l.ns_per_load,
                })
                .collect(),
            cache,
            bandwidth_threads: cores.len() as u32,
            read_gbs: bw[0],
            write_gbs: bw[1],
            copy_gbs: bw[2],
            modules,
            modules_error,
        },
        scheduling: wake_stats,
        power: PowerInfo {
            source: match source {
                PowerSource::Ac => "ac",
                PowerSource::Battery => "battery",
                PowerSource::Unknown => "unknown",
            }
            .to_owned(),
            scheme: power
                .as_ref()
                .map(|p| {
                    p.scheme_name
                        .clone()
                        .unwrap_or_else(|| p.scheme_guid.clone())
                })
                .unwrap_or_else(|| "inconnu".to_owned()),
            overlay: power.as_ref().and_then(|p| p.overlay_guid.clone()),
        },
        gpu,
        verdicts,
        checks: vec![
            "fréquence : pic mono-cœur de chaque cœur et courbe sous charge FMA".to_owned(),
            "mémoire : latence, tailles de cache, débit, modules (WMI)".to_owned(),
            "ordonnancement : latence de réveil d'un timer haute résolution".to_owned(),
            "alimentation : source, plan, mode".to_owned(),
        ],
    };
    println!();
    print!("{}", render::render_fr(&report));
    let saved = store::save(&health_dir()?, &report)?;
    println!("\nRapport enregistré : {}", saved.json_path.display());
    println!(
        "{}",
        if saved.baseline_updated {
            "Baseline mise à jour (run sur secteur)."
        } else {
            "Baseline inchangée (le run n'est pas sur secteur)."
        }
    );
    println!("\nDurées par phase :");
    for (name, d) in &timings.0 {
        println!("  {name:<14} {:>5.1} s", d.as_secs_f64());
    }
    println!("  {:<14} {:>5.1} s", "total", t0.elapsed().as_secs_f64());
    Ok(ExitCode::SUCCESS)
}

/// Runs the FMA load on `load_set`, and samples the clock of `sampler` once per second for `duration`.
fn load_and_sample(
    isa: Isa,
    load_set: &[u32],
    sampler: u32,
    duration: Duration,
    hz: std::num::NonZeroU64,
) -> Result<Vec<f64>, Abort> {
    let stop = AtomicBool::new(false);
    let block = load_block(isa);
    let mut curve = Vec::new();
    let result = std::thread::scope(|s| {
        for &cpu in load_set {
            let stop = &stop;
            s.spawn(move || {
                let _pin = pin_current_thread(cpu);
                fma::burn(isa, stop, block);
            });
        }
        let r = (|| -> Result<(), Abort> {
            let _pin = pin_current_thread(sampler)?;
            let end = Instant::now() + duration;
            let mut last = 0.0;
            while Instant::now() < end {
                check_interrupt()?;
                if let Some(b) = freq::sample_for(Duration::from_secs(1), hz) {
                    last = b.median_hz / 1e9;
                }
                curve.push(last);
            }
            Ok(())
        })();
        stop.store(true, Ordering::Relaxed);
        r
    });
    result?;
    Ok(curve)
}
