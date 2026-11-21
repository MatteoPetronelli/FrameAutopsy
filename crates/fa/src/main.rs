//! `fa`: FrameAutopsy command line. Non-elevated; only read-only commands exist so far.
#![forbid(unsafe_code)]

mod probe;

use fa_core::clock::Timebase;
use fa_win::{clock, topology};
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: fa clock [--seconds N]   (TSC vs QPC interval, default 2 s)
       fa probe freq [--seconds N] [--cpu N] [--load-all]   (frequency probe next to PDH, CSV)
       fa probe wake [--cpu N]   (timer wake-up lateness, 1 ms x 2000)
       fa probe domain [--cpu A] [--other B]   (does core A follow the load of core B?)
       fa probe curve [--cpu N]   (memory latency, 16 KiB to 256 MiB, CSV + comparison with the topology)
       fa probe bw [--kernel avx2|sse2|scalar]   (memory bandwidth, 1 to N cores)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("clock") => match parse_seconds(&args[1..]) {
            Some(seconds) => {
                print_clock(seconds);
                ExitCode::SUCCESS
            }
            None => usage(),
        },
        Some("probe") if args.get(1).map(String::as_str) == Some("freq") => {
            match probe::parse(&args[2..]) {
                Some(a) => {
                    let cpus = topology::read().map(|t| t.cpus.len() as u32).unwrap_or(1);
                    match probe::run_freq(&a, cpus) {
                        Ok(()) => ExitCode::SUCCESS,
                        Err(e) => {
                            eprintln!("fa probe freq: {e}");
                            ExitCode::FAILURE
                        }
                    }
                }
                None => usage(),
            }
        }
        Some("probe") if args.get(1).map(String::as_str) == Some("wake") => {
            run_or_fail(|| probe::run_wake(&args[2..]))
        }
        Some("probe") if args.get(1).map(String::as_str) == Some("domain") => {
            run_or_fail(|| probe::run_domain(&args[2..]))
        }
        Some("probe") if args.get(1).map(String::as_str) == Some("curve") => {
            run_or_fail(|| probe::run_curve(&args[2..]))
        }
        Some("probe") if args.get(1).map(String::as_str) == Some("bw") => {
            run_or_fail(|| probe::run_bw(&args[2..]))
        }
        _ => usage(),
    }
}

fn run_or_fail(f: impl FnOnce() -> Option<std::io::Result<()>>) -> ExitCode {
    match f() {
        None => usage(),
        Some(Ok(())) => ExitCode::SUCCESS,
        Some(Err(e)) => {
            eprintln!("fa probe: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn parse_seconds(rest: &[String]) -> Option<u64> {
    match rest {
        [] => Some(2),
        [flag, n] if flag == "--seconds" => n.parse().ok().filter(|s| (1..=3600).contains(s)),
        _ => None,
    }
}

fn print_clock(seconds: u64) {
    let c = clock::Clock::new();
    let tb = match c.timebase() {
        Timebase::Tsc => "TSC (invariant)",
        Timebase::Qpc => "QPC (degraded: no invariant TSC)",
    };
    println!("timebase:      {tb}");
    println!(
        "frequency:     {} Hz ({:?})",
        c.hz(),
        c.baseline().map(|b| b.source)
    );
    match clock::Clock::measure_baseline() {
        Some(b) => println!(
            "baseline:      CPUID {:?} Hz, calibrated {:?} Hz, gap {:?} ppm",
            b.cpuid_hz, b.calibrated_hz, b.gap_ppm
        ),
        None => println!("baseline:      unavailable (no CPUID frequency, QPC did not advance)"),
    }
    let (clock_ns, qpc_ns) = clock::measure_interval(&c, Duration::from_secs(seconds));
    println!(
        "interval:      {seconds} s -> clock {clock_ns} ns, QPC {qpc_ns} ns, gap {} ns",
        clock_ns.abs_diff(qpc_ns)
    );
    match clock::cross_core_worst_backward(20) {
        Ok(ticks) => println!(
            "core skew:     worst backward step {ticks} ticks ({} ns)",
            c.ticks_to_ns(ticks)
        ),
        Err(e) => println!("core skew:     unavailable ({e})"),
    }
    match topology::read() {
        Ok(t) => println!(
            "topology:      {} logical CPUs, {} cores, {} LLC domain(s), hybrid: {}",
            t.cpus.len(),
            t.physical_cores(),
            t.llc_domains(),
            t.is_hybrid()
        ),
        Err(e) => println!("topology:      unavailable ({e})"),
    }
}
