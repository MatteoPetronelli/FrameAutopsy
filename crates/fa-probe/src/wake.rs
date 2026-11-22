//! Wake-up latency probe (no asm): how late a thread pinned on a CPU wakes from a high-resolution waitable timer.
//! Lateness = wake time minus (time just before the wait + requested delay); it therefore includes the cost of the
//! `SetWaitableTimer` call itself (a few microseconds). Our own process only: timer resolution is per process since
//! Windows 10 2004.

use fa_win::{clock::Clock, thread::pin_current_thread, timer::HiResTimer};
use std::io;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WakeStats {
    pub n: usize,
    pub p50_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
}

/// Nearest-rank percentile of a sorted slice (`p` in 0..=100).
fn percentile(sorted: &[u64], p: usize) -> u64 {
    let rank = (p * sorted.len()).div_ceil(100).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// p50/p99/max of lateness samples in nanoseconds; `None` for an empty slice.
pub fn summarize(late_ns: &mut [u64]) -> Option<WakeStats> {
    if late_ns.is_empty() {
        return None;
    }
    late_ns.sort_unstable();
    let us = |ns: u64| ns as f64 / 1000.0;
    Some(WakeStats {
        n: late_ns.len(),
        p50_us: us(percentile(late_ns, 50)),
        p99_us: us(percentile(late_ns, 99)),
        max_us: us(late_ns[late_ns.len() - 1]),
    })
}

/// `rounds` waits of `delay` on `cpu`.
pub fn run(cpu: u32, delay: Duration, rounds: usize) -> io::Result<Option<WakeStats>> {
    let _pin = pin_current_thread(cpu)?;
    let clock = Clock::new();
    let timer = HiResTimer::new()?;
    let delay_ns = delay.as_nanos();
    let mut late = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let t0 = clock.now();
        timer.wait(delay)?;
        let elapsed_ns = clock.ticks_to_ns(clock.now().saturating_sub(t0));
        late.push(elapsed_ns.saturating_sub(delay_ns) as u64);
    }
    Ok(summarize(&mut late))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let mut v: Vec<u64> = (1..=100).map(|x| x * 1000).collect();
        let s = summarize(&mut v).unwrap();
        assert_eq!(
            (s.p50_us, s.p99_us, s.max_us, s.n),
            (50.0, 99.0, 100.0, 100)
        );
    }

    #[test]
    fn one_outlier_moves_max_but_not_p99_of_a_hundred() {
        let mut v = vec![10_000u64; 99];
        v.push(5_000_000);
        let s = summarize(&mut v).unwrap();
        assert_eq!((s.p99_us, s.max_us), (10.0, 5000.0));
    }

    #[test]
    fn an_unsorted_input_is_handled_and_empty_gives_none() {
        let mut v = vec![30_000u64, 10_000, 20_000];
        assert_eq!(summarize(&mut v).unwrap().p50_us, 20.0);
        assert_eq!(summarize(&mut []), None);
    }

    #[test]
    fn a_real_run_reports_plausible_lateness() {
        let s = run(2, Duration::from_millis(1), 200).unwrap().unwrap();
        assert!(s.p50_us >= 0.0 && s.p50_us < 2000.0, "p50 {} us", s.p50_us);
        assert!(s.max_us >= s.p99_us && s.p99_us >= s.p50_us);
    }
}
