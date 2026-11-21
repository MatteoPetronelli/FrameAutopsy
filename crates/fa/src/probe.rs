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
    /// Spin one of our own threads on one logical CPU of every other physical core: the whole package is loaded, the
    /// SMT sibling of the probed CPU stays idle.
    pub load_all: bool,
    /// Same, but on every other logical CPU, siblings included (the probe shares its core with a busy thread).
    pub load_smt: bool,
}

pub fn parse(rest: &[String]) -> Option<FreqArgs> {
    let mut a = FreqArgs {
        seconds: 60,
        cpu: 2,
        load_all: false,
        load_smt: false,
    };
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--seconds" => {
                a.seconds = it.next()?.parse().ok().filter(|s| (1..=3600).contains(s))?
            }
            "--cpu" => a.cpu = it.next()?.parse().ok().filter(|c| *c < 64)?,
            "--load-all" => a.load_all = true,
            "--load-smt" => a.load_smt = true,
            _ => return None,
        }
    }
    Some(a)
}

pub fn run_freq(a: &FreqArgs, cpus: &[fa_win::topology::LogicalCpu]) -> std::io::Result<()> {
    let hz = Clock::new().hz();
    let stop = Arc::new(AtomicBool::new(false));
    let mut loads = Vec::new();
    let targets: Vec<u32> = if a.load_smt {
        cpus.iter()
            .map(|c| c.index)
            .filter(|c| *c != a.cpu)
            .collect()
    } else if a.load_all {
        let core_of = |i: u32| cpus.iter().find(|x| x.index == i).map(|x| x.core);
        fa_probe::bandwidth::physical_core_cpus(cpus)
            .into_iter()
            .filter(|c| core_of(*c) != core_of(a.cpu))
            .collect()
    } else {
        Vec::new()
    };
    {
        for cpu in targets {
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

/// `--cpu N` and `--other N` flags; `None` when something else is passed.
fn cpu_flags(rest: &[String], default_cpu: u32, default_other: u32) -> Option<(u32, u32)> {
    let (mut cpu, mut other) = (default_cpu, default_other);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let v: u32 = it.next()?.parse().ok().filter(|c| *c < 64)?;
        match flag.as_str() {
            "--cpu" => cpu = v,
            "--other" => other = v,
            _ => return None,
        }
    }
    (cpu != other).then_some((cpu, other))
}

pub fn run_wake(rest: &[String]) -> Option<std::io::Result<()>> {
    let (cpu, _) = cpu_flags(rest, 2, u32::MAX)?;
    Some(fa_probe::wake::run(cpu, Duration::from_millis(1), 2000).map(|s| match s {
        Some(s) => println!(
            "wake-up lateness on CPU {cpu}, {} waits of 1 ms: p50 {:.1} us, p99 {:.1} us, max {:.1} us",
            s.n, s.p50_us, s.p99_us, s.max_us
        ),
        None => println!("no samples"),
    }))
}

pub fn run_domain(rest: &[String]) -> Option<std::io::Result<()>> {
    let (a, b) = cpu_flags(rest, 2, 4)?;
    let hz = Clock::new().hz();
    Some(
        fa_probe::domain::check(a, b, hz, Duration::from_secs(1), 5).map(|r| {
            println!(
                "core {a}: {:.3} GHz with core {b} idle, {:.3} GHz with core {b} loaded -> {:?}",
                r.idle_hz / 1e9,
                r.loaded_hz / 1e9,
                r.verdict
            )
        }),
    )
}

pub fn run_curve(rest: &[String]) -> Option<std::io::Result<()>> {
    let (cpu, _) = cpu_flags(rest, 2, u32::MAX)?;
    Some(curve(cpu))
}

fn curve(cpu: u32) -> std::io::Result<()> {
    let c = fa_probe::mem::cache_curve(cpu, true)?;
    println!("size_kib,ns_per_load,pages");
    for l in &c {
        println!("{},{:.3},{:?}", l.bytes >> 10, l.ns_per_load, l.page);
    }
    let pts: Vec<(usize, f64)> = c.iter().map(|l| (l.bytes, l.ns_per_load)).collect();
    let steps = fa_probe::mem::detect_steps(&pts);
    let topo = fa_win::topology::read().map_err(std::io::Error::other)?;
    for k in fa_probe::mem::compare_with_topology(&steps, &topo.caches) {
        println!(
            "L{}: topology {} KiB, detected {:?} KiB -> {}",
            k.level,
            k.topology_bytes >> 10,
            k.detected_bytes.map(|b| b >> 10),
            if k.agrees {
                "agrees"
            } else {
                "MISMATCH (reported, not fatal)"
            }
        );
    }
    Ok(())
}

pub fn run_bw(rest: &[String]) -> Option<std::io::Result<()>> {
    use fa_probe::bandwidth::{physical_core_cpus, run, Mode};
    let kernel = match rest {
        [] => fa_probe::bandwidth::best_kernel(),
        [f, k] if f == "--kernel" => match k.as_str() {
            "avx2" => fa_probe::bandwidth::Kernel::Avx2,
            "sse2" => fa_probe::bandwidth::Kernel::Sse2,
            "scalar" => fa_probe::bandwidth::Kernel::Scalar,
            _ => return None,
        },
        _ => return None,
    };
    Some((|| {
        let topo = fa_win::topology::read().map_err(std::io::Error::other)?;
        let cores = physical_core_cpus(&topo.cpus);
        println!("kernel {kernel:?}, 64 MiB per thread, GB/s (copy counts read + write)");
        println!("threads,read,write,copy");
        for n in 1..=cores.len() {
            let mut row = format!("{n}");
            for mode in [Mode::Read, Mode::Write, Mode::Copy] {
                row += &format!(",{:.1}", run(&cores[..n], mode, kernel, 64 << 20, 8)?);
            }
            println!("{row}");
        }
        Ok(())
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn freq_flags_are_parsed_and_bad_ones_refused() {
        let a = parse(&v(&["--seconds", "30", "--cpu", "5", "--load-all"])).unwrap();
        assert_eq!((a.seconds, a.cpu, a.load_all), (30, 5, true));
        let d = parse(&[]).unwrap();
        assert_eq!((d.seconds, d.cpu, d.load_all), (60, 2, false));
        assert!(parse(&v(&["--seconds", "0"])).is_none());
        assert!(parse(&v(&["--seconds", "99999"])).is_none());
        assert!(parse(&v(&["--cpu", "64"])).is_none());
        assert!(parse(&v(&["--cpu"])).is_none());
        assert!(parse(&v(&["--nope"])).is_none());
    }

    #[test]
    fn cpu_flags_refuse_the_same_core_twice_and_unknown_flags() {
        assert_eq!(cpu_flags(&[], 2, 4), Some((2, 4)));
        assert_eq!(
            cpu_flags(&v(&["--cpu", "7", "--other", "1"]), 2, 4),
            Some((7, 1))
        );
        assert_eq!(cpu_flags(&v(&["--cpu", "3", "--other", "3"]), 2, 4), None);
        assert_eq!(cpu_flags(&v(&["--cpu", "3"]), 2, 3), None); // default other == cpu
        assert_eq!(cpu_flags(&v(&["--x", "1"]), 2, 4), None);
    }
}
