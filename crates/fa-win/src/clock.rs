//! `Clock`: the one timebase of the project (raw TSC ticks; QPC in degraded mode). Pure math lives in
//! `fa_core::clock`; TSC and CPUID reads in `fa_asm::tsc`; this module adds the QPC read (WIN-11) and ties them.

use fa_asm::tsc;
use fa_core::clock::{
    choose_baseline, freq_from_cpuid_15, freq_from_pair, median, ticks_to_ns, ClockBaseline,
    Timebase,
};
use std::num::NonZeroU64;
use std::time::Duration;
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

/// Calibration runs and their minimum length (spec `tsc-clock`: 5 runs of at least 200 ms, median kept).
pub const CALIBRATION_RUNS: usize = 5;
pub const CALIBRATION_RUN: Duration = Duration::from_millis(200);

/// QPC ticks now. QPC cannot fail on any supported Windows version.
pub fn qpc_now() -> u64 {
    let mut v = 0i64;
    // SAFETY: `v` is a valid, writable i64 for the call.
    let ok = unsafe { QueryPerformanceCounter(&mut v) };
    debug_assert!(ok != 0, "QueryPerformanceCounter failed");
    u64::try_from(v).unwrap_or(0)
}

/// QPC ticks per second (constant for the lifetime of the system).
pub fn qpc_freq() -> NonZeroU64 {
    let mut v = 0i64;
    // SAFETY: `v` is a valid, writable i64 for the call.
    let ok = unsafe { QueryPerformanceFrequency(&mut v) };
    debug_assert!(ok != 0, "QueryPerformanceFrequency failed");
    NonZeroU64::new(u64::try_from(v).unwrap_or(0)).unwrap_or(NonZeroU64::MIN)
}

/// A TSC read and a QPC read taken back to back; the QPC read is bracketed by two TSC reads and the midpoint is used,
/// so the pair describes (nearly) the same instant.
pub fn read_pair() -> (u64, u64) {
    let t0 = tsc::now_serialized();
    let q = qpc_now();
    let t1 = tsc::now_serialized();
    (t0 + t1.saturating_sub(t0) / 2, q)
}

/// TSC frequency measured against QPC: `runs` intervals of at least `run`, median kept. `None` if QPC did not advance.
pub fn calibrate_tsc_hz(runs: usize, run: Duration) -> Option<NonZeroU64> {
    let qpc_hz = qpc_freq();
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let (t0, q0) = read_pair();
        std::thread::sleep(run);
        let (t1, q1) = read_pair();
        samples.push(freq_from_pair(t1.saturating_sub(t0), q1.saturating_sub(q0), qpc_hz)?.get());
    }
    NonZeroU64::new(median(&mut samples)?)
}

/// The same interval measured with the active timebase and with QPC, in nanoseconds: `(clock_ns, qpc_ns)`.
pub fn measure_interval(clock: &Clock, duration: Duration) -> (u128, u128) {
    let (t0, q0) = read_pair();
    std::thread::sleep(duration);
    let (t1, q1) = read_pair();
    let ticks = match clock.timebase() {
        Timebase::Tsc => t1.saturating_sub(t0),
        Timebase::Qpc => q1.saturating_sub(q0),
    };
    (
        clock.ticks_to_ns(ticks),
        ticks_to_ns(q1.saturating_sub(q0), qpc_freq()),
    )
}

/// Worst backward TSC step, in ticks, seen while the calling thread hops through every logical CPU `rounds` times.
/// Reads follow each other in real time, so a core whose TSC is behind by `d` shows up as a backward step of about `d`;
/// forward steps are the hop cost, not a skew.
pub fn cross_core_worst_backward(rounds: u32) -> std::io::Result<u64> {
    let n = std::thread::available_parallelism()?.get() as u32;
    let mut worst = 0u64;
    let mut last = tsc::now_serialized();
    for _ in 0..rounds {
        for cpu in 0..n {
            let _pin = crate::thread::pin_current_thread(cpu)?;
            let t = tsc::now_serialized();
            worst = worst.max(last.saturating_sub(t));
            last = last.max(t);
        }
    }
    Ok(worst)
}

#[derive(Debug, Clone)]
pub struct Clock {
    timebase: Timebase,
    hz: NonZeroU64,
    baseline: Option<ClockBaseline>,
}

impl Clock {
    /// Timebase and frequency, fast path: CPUID `0x15` when it gives a frequency, otherwise a QPC calibration
    /// (about one second). Without an invariant TSC the clock runs on QPC (degraded mode, reported by the health check).
    pub fn new() -> Clock {
        if !tsc::invariant_tsc() {
            return Clock {
                timebase: Timebase::Qpc,
                hz: qpc_freq(),
                baseline: None,
            };
        }
        let cpuid = tsc::cpuid_leaf_15().and_then(|(a, b, c)| freq_from_cpuid_15(a, b, c));
        let calibrated = if cpuid.is_none() {
            calibrate_tsc_hz(CALIBRATION_RUNS, CALIBRATION_RUN)
        } else {
            None
        };
        match choose_baseline(cpuid, calibrated) {
            Some(b) => Clock {
                timebase: Timebase::Tsc,
                hz: NonZeroU64::new(b.tsc_hz).unwrap_or(NonZeroU64::MIN),
                baseline: Some(b),
            },
            // No CPUID frequency and QPC did not advance: nothing trustworthy, stay on QPC.
            None => Clock {
                timebase: Timebase::Qpc,
                hz: qpc_freq(),
                baseline: None,
            },
        }
    }

    /// Full baseline for the health check: CPUID and a calibration, both recorded with their gap. Takes about a second.
    pub fn measure_baseline() -> Option<ClockBaseline> {
        let cpuid = tsc::cpuid_leaf_15().and_then(|(a, b, c)| freq_from_cpuid_15(a, b, c));
        choose_baseline(cpuid, calibrate_tsc_hz(CALIBRATION_RUNS, CALIBRATION_RUN))
    }

    pub fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Ticks per second of the active timebase.
    pub fn hz(&self) -> NonZeroU64 {
        self.hz
    }

    pub fn baseline(&self) -> Option<&ClockBaseline> {
        self.baseline.as_ref()
    }

    /// Current tick count in the active timebase.
    pub fn now(&self) -> u64 {
        match self.timebase {
            Timebase::Tsc => tsc::now(),
            Timebase::Qpc => qpc_now(),
        }
    }

    pub fn ticks_to_ns(&self, ticks: u64) -> u128 {
        ticks_to_ns(ticks, self.hz)
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qpc_advances_and_has_a_plausible_frequency() {
        let (a, b) = (qpc_now(), qpc_now());
        assert!(b >= a);
        assert!(
            qpc_freq().get() >= 1_000_000,
            "QPC below 1 MHz is not a high-resolution counter"
        );
    }

    #[test]
    fn the_pair_lies_between_its_two_tsc_reads() {
        let before = tsc::now_serialized();
        let (t, _) = read_pair();
        let after = tsc::now_serialized();
        assert!(before <= t && t <= after);
    }

    #[test]
    fn the_target_uses_the_tsc_with_the_cpuid_frequency() {
        // Target machine (i5-11400H): invariant TSC, CPUID 0x15 = 2.688 GHz.
        let c = Clock::new();
        assert_eq!(c.timebase(), Timebase::Tsc);
        assert_eq!(c.hz().get(), 2_688_000_000);
    }

    /// Takes about a second: a short calibration (2 x 100 ms) must land within 1 % of the CPUID value.
    #[test]
    fn a_short_calibration_agrees_with_cpuid_within_one_percent() {
        let measured = calibrate_tsc_hz(2, Duration::from_millis(100))
            .unwrap()
            .get();
        let cpuid = 2_688_000_000u64;
        assert!(
            measured.abs_diff(cpuid) * 100 < cpuid,
            "calibrated {measured} Hz vs CPUID {cpuid} Hz"
        );
    }

    /// Cross-core consistency (spec `tsc-clock`, skew < 1 us, calibrate).
    #[test]
    fn the_tsc_has_no_skew_between_cores_above_one_microsecond() {
        let one_us = Clock::new().hz().get() / 1_000_000;
        let worst = cross_core_worst_backward(50).unwrap();
        eprintln!("cross-core: worst backward step {worst} ticks (limit {one_us} ticks = 1 us)");
        assert!(
            worst < one_us,
            "TSC steps back by {worst} ticks when changing core"
        );
    }

    /// 10 s TSC vs QPC (spec: gap < 100 us, calibrate; recalibrated from 10 us in lot 2, measured about 45 us). Slow, so run it on demand:
    /// `cargo test -p fa-win ten_second -- --ignored --nocapture`.
    #[test]
    #[ignore = "takes 10 s; run on demand and record the value"]
    fn ten_second_tsc_and_qpc_intervals_agree_within_a_hundred_microseconds() {
        let clock = Clock::new();
        let (tsc_ns, qpc_ns) = measure_interval(&clock, Duration::from_secs(10));
        let gap_us = tsc_ns.abs_diff(qpc_ns) as f64 / 1_000.0;
        let ppm = tsc_ns.abs_diff(qpc_ns) as f64 / qpc_ns as f64 * 1e6;
        eprintln!("10 s: tsc {tsc_ns} ns, qpc {qpc_ns} ns, gap {gap_us:.1} us ({ppm:.2} ppm)");
        assert!(
            gap_us < 100.0,
            "gap of {gap_us:.1} us with the {:?} frequency",
            clock.baseline().map(|b| b.source)
        );
    }

    #[test]
    fn conversion_uses_the_active_frequency() {
        let c = Clock::new();
        assert_eq!(c.ticks_to_ns(c.hz().get()), 1_000_000_000);
    }
}
