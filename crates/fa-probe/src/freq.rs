//! Effective frequency probe orchestration (ASM-02 kernel in `fa-asm`). Pure logic here, plus one pinned runner.

use fa_asm::freq::{self, Sample};
use std::io;
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

/// A sample whose elapsed ticks exceed this multiple of the median was preempted (spec: 1.5x the recent median).
pub const PREEMPT_FACTOR: f64 = 1.5;

/// Core cycles per sample: about 50 us at 4 GHz.
pub const SAMPLE_CYCLES: u64 = 200_000;

/// Keeps the samples whose elapsed ticks are at most [`PREEMPT_FACTOR`] times the median elapsed ticks.
pub fn drop_preempted(samples: &[Sample]) -> Vec<Sample> {
    if samples.is_empty() {
        return Vec::new();
    }
    let mut ticks: Vec<u64> = samples.iter().map(|s| s.ticks).collect();
    ticks.sort_unstable();
    let median = ticks[ticks.len() / 2] as f64;
    samples
        .iter()
        .copied()
        .filter(|s| s.ticks as f64 <= PREEMPT_FACTOR * median)
        .collect()
}

/// Effective core clock of one sample, in Hz: cycles per TSC tick times the TSC frequency.
pub fn effective_hz(s: Sample, tsc_hz: NonZeroU64) -> f64 {
    s.cycles as f64 / s.ticks.max(1) as f64 * tsc_hz.get() as f64
}

/// Result of one batch of samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreqBatch {
    pub median_hz: f64,
    pub min_hz: f64,
    pub max_hz: f64,
    pub kept: usize,
    pub rejected: usize,
}

/// Rejects preempted samples, then reports the median and the spread of the rest. `None` when nothing is left.
pub fn summarize(samples: &[Sample], tsc_hz: NonZeroU64) -> Option<FreqBatch> {
    let kept = drop_preempted(samples);
    let mut hz: Vec<f64> = kept.iter().map(|s| effective_hz(*s, tsc_hz)).collect();
    if hz.is_empty() {
        return None;
    }
    hz.sort_by(f64::total_cmp);
    Some(FreqBatch {
        median_hz: hz[hz.len() / 2],
        min_hz: hz[0],
        max_hz: hz[hz.len() - 1],
        kept: hz.len(),
        rejected: samples.len() - hz.len(),
    })
}

/// Pins the calling thread on `cpu`, spins the probe back to back for `batch` and returns one batch of samples.
/// Continuous sampling is the calibration setting (the core is busy, so its clock is steady); the in-game duty cycle
/// (at most 0.01 %) belongs to lot 8.
pub fn run_batch(cpu: u32, batch: Duration, tsc_hz: NonZeroU64) -> io::Result<Option<FreqBatch>> {
    let _pin = fa_win::thread::pin_current_thread(cpu)?;
    Ok(sample_for(batch, tsc_hz))
}

/// Samples back to back for `batch` on the current thread.
pub fn sample_for(batch: Duration, tsc_hz: NonZeroU64) -> Option<FreqBatch> {
    let end = Instant::now() + batch;
    let mut samples = Vec::with_capacity(1 << 14);
    while Instant::now() < end {
        samples.push(freq::sample(SAMPLE_CYCLES));
    }
    summarize(&samples, tsc_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HZ: NonZeroU64 = NonZeroU64::new(2_000_000_000).unwrap();

    fn s(ticks: u64) -> Sample {
        Sample {
            cycles: 200_000,
            ticks,
        }
    }

    #[test]
    fn a_preempted_sample_is_dropped_and_does_not_move_the_median() {
        // steady 100 000 ticks = 4 GHz at a 2 GHz TSC; one sample was preempted for 5 ms
        let mut v = vec![s(100_000); 20];
        v.push(s(10_000_000));
        let b = summarize(&v, HZ).unwrap();
        assert_eq!((b.kept, b.rejected), (20, 1));
        assert_eq!(b.median_hz, 4.0e9);
        assert_eq!((b.min_hz, b.max_hz), (4.0e9, 4.0e9));
    }

    #[test]
    fn the_limit_is_one_and_a_half_times_the_median() {
        let mut v = vec![s(1000); 9];
        v.push(s(1500)); // exactly 1.5x: kept
        v.push(s(1501)); // above: dropped
        assert_eq!(drop_preempted(&v).len(), 10);
    }

    #[test]
    fn no_samples_gives_no_result() {
        assert_eq!(summarize(&[], HZ), None);
    }

    #[test]
    fn an_all_slow_batch_is_kept_because_the_median_follows_it() {
        // a core that really dropped to a low clock is not "preemption": the median moves with it
        let v = vec![s(400_000); 10];
        let b = summarize(&v, HZ).unwrap();
        assert_eq!((b.kept, b.median_hz), (10, 1.0e9));
    }

    #[test]
    fn a_real_pinned_batch_reports_a_plausible_clock() {
        let hz = fa_win::clock::Clock::new().hz();
        let b = run_batch(2, Duration::from_millis(200), hz)
            .unwrap()
            .unwrap();
        let ghz = b.median_hz / 1e9;
        assert!((0.5..6.0).contains(&ghz), "{ghz} GHz");
        assert!(b.kept > 100);
    }
}
