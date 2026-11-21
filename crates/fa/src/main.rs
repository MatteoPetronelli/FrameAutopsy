//! `fa`: FrameAutopsy command line. Non-elevated; only read-only commands exist so far.
#![forbid(unsafe_code)]

use fa_core::clock::Timebase;
use fa_win::{clock, topology};
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: fa clock [--seconds N]   (TSC vs QPC interval, default 2 s)";

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
        _ => usage(),
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
