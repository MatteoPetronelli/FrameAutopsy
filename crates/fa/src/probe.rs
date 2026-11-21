//! `fa probe freq`: runs the ASM-02 frequency probe next to the PDH counter and prints one CSV line per second.
//! Off-game, own threads only. Continuous sampling (calibration setting), see `fa-probe::freq`.

use fa_probe::freq;
use fa_win::{clock::Clock, pdh::PdhReader, thread::pin_current_thread};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct FreqArgs {
    pub seconds: u64,
    pub cpu: u32,
    /// Spin one of our own threads on every other logical CPU, so the whole package is under steady load.
    pub load_all: bool,
}

pub fn parse(rest: &[String]) -> Option<FreqArgs> {
    let mut a = FreqArgs {
        seconds: 60,
        cpu: 2,
        load_all: false,
    };
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--seconds" => {
                a.seconds = it.next()?.parse().ok().filter(|s| (1..=3600).contains(s))?
            }
            "--cpu" => a.cpu = it.next()?.parse().ok().filter(|c| *c < 64)?,
            "--load-all" => a.load_all = true,
            _ => return None,
        }
    }
    Some(a)
}

pub fn run_freq(a: &FreqArgs, cpus: u32) -> std::io::Result<()> {
    let hz = Clock::new().hz();
    let stop = Arc::new(AtomicBool::new(false));
    let mut loads = Vec::new();
    if a.load_all {
        for cpu in (0..cpus).filter(|c| *c != a.cpu) {
            let stop = Arc::clone(&stop);
            loads.push(std::thread::spawn(move || {
                let _pin = pin_current_thread(cpu);
                let mut x = 1u64;
                while !stop.load(Ordering::Relaxed) {
                    for _ in 0..100_000 {
                        x = std::hint::black_box(x.wrapping_mul(x | 3));
                    }
                }
                x
            }));
        }
    }
    let cpu = a.cpu;
    let perf = format!(r"\Processor Information(0,{cpu})\% Processor Performance");
    let freq_c = format!(r"\Processor Information(0,{cpu})\Processor Frequency");
    let pdh = PdhReader::new(&[&perf, &freq_c])?;
    pdh.collect()?;
    println!("second,probe_ghz,probe_min_ghz,probe_max_ghz,kept,rejected,pdh_perf_pct,pdh_freq_mhz,pdh_ghz");
    let result = (|| {
        for second in 1..=a.seconds {
            let b = freq::run_batch(cpu, Duration::from_secs(1), hz)?;
            pdh.collect()?;
            let (p, f) = (pdh.value(0)?, pdh.value(1)?);
            match b {
                Some(b) => println!(
                    "{second},{:.4},{:.4},{:.4},{},{},{p:.1},{f:.0},{:.4}",
                    b.median_hz / 1e9,
                    b.min_hz / 1e9,
                    b.max_hz / 1e9,
                    b.kept,
                    b.rejected,
                    p / 100.0 * f / 1000.0
                ),
                None => println!("{second},,,,0,0,{p:.1},{f:.0},"),
            }
        }
        Ok(())
    })();
    stop.store(true, Ordering::Relaxed);
    for l in loads {
        let _ = l.join();
    }
    result
}
