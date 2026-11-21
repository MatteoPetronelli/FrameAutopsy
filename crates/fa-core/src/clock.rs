//! Pure clock logic (no hardware access): timebase, TSC frequency choice, integer tick conversion and the QPC<->TSC
//! map. Readings come from outside (`fa-asm::tsc`, `fa-win::clock`) so everything here is testable with fake clocks.

use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

const NS_PER_S: u128 = 1_000_000_000;
const PPM: u128 = 1_000_000;

/// Largest accepted gap between the CPUID and the calibrated TSC frequency, in parts per million. (calibrate)
pub const GAP_PPM_LIMIT: u64 = 1_000;
/// Interval between two QPC<->TSC map re-estimations, in seconds.
pub const RESYNC_SECONDS: u64 = 60;

/// The counter every timestamp is expressed in. Raw TSC ticks everywhere unless the CPU has no invariant TSC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timebase {
    Tsc,
    /// Degraded mode: ticks are QPC counts. The health check reports it.
    Qpc,
}

impl Timebase {
    pub fn from_invariant(invariant_tsc: bool) -> Self {
        if invariant_tsc {
            Timebase::Tsc
        } else {
            Timebase::Qpc
        }
    }
}

/// Ticks to nanoseconds in integer math: no float, no drift, no overflow for any `u64` tick count.
pub fn ticks_to_ns(ticks: u64, hz: NonZeroU64) -> u128 {
    u128::from(ticks) * NS_PER_S / u128::from(hz.get())
}

/// TSC frequency from CPUID leaf `0x15` (`eax` = denominator, `ebx` = numerator, `ecx` = crystal Hz).
/// `None` when any of the three is zero: many Intel parts report no crystal frequency.
pub fn freq_from_cpuid_15(eax: u32, ebx: u32, ecx: u32) -> Option<NonZeroU64> {
    if eax == 0 || ebx == 0 || ecx == 0 {
        return None;
    }
    let hz = u128::from(ecx) * u128::from(ebx) / u128::from(eax);
    NonZeroU64::new(u64::try_from(hz).ok()?)
}

/// TSC frequency from one calibration interval measured with both counters.
pub fn freq_from_pair(tsc_delta: u64, qpc_delta: u64, qpc_hz: NonZeroU64) -> Option<NonZeroU64> {
    if qpc_delta == 0 {
        return None;
    }
    let hz = u128::from(tsc_delta) * u128::from(qpc_hz.get()) / u128::from(qpc_delta);
    NonZeroU64::new(u64::try_from(hz).ok()?)
}

/// Median (upper middle for an even count); `None` on an empty slice.
pub fn median(values: &mut [u64]) -> Option<u64> {
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

/// Gap between two frequencies in parts per million of `reference`.
pub fn gap_ppm(a: NonZeroU64, reference: NonZeroU64) -> u64 {
    let diff = u128::from(a.get().abs_diff(reference.get()));
    u64::try_from(diff * PPM / u128::from(reference.get())).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreqSource {
    Cpuid15,
    QpcCalibration,
}

/// Clock facts kept in the health baseline (lot 4 reads them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockBaseline {
    pub tsc_hz: u64,
    pub source: FreqSource,
    pub cpuid_hz: Option<u64>,
    pub calibrated_hz: Option<u64>,
    /// Gap between both values when both exist.
    pub gap_ppm: Option<u64>,
}

/// Picks the frequency: CPUID when available, unless a calibration disagrees by more than [`GAP_PPM_LIMIT`]
/// (the CPUID value can be nominal rather than actual), in which case the calibrated value wins.
pub fn choose_baseline(
    cpuid: Option<NonZeroU64>,
    calibrated: Option<NonZeroU64>,
) -> Option<ClockBaseline> {
    let gap = cpuid.zip(calibrated).map(|(a, b)| gap_ppm(a, b));
    let (hz, source) = match (cpuid, calibrated) {
        (None, None) => return None,
        (Some(c), None) => (c, FreqSource::Cpuid15),
        (None, Some(k)) => (k, FreqSource::QpcCalibration),
        (Some(_), Some(k)) if gap.is_some_and(|g| g > GAP_PPM_LIMIT) => {
            (k, FreqSource::QpcCalibration)
        }
        (Some(c), Some(_)) => (c, FreqSource::Cpuid15),
    };
    Some(ClockBaseline {
        tsc_hz: hz.get(),
        source,
        cpuid_hz: cpuid.map(NonZeroU64::get),
        calibrated_hz: calibrated.map(NonZeroU64::get),
        gap_ppm: gap,
    })
}

/// Offset-and-scale map between QPC and TSC ticks, anchored on one back-to-back pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QpcTscMap {
    tsc0: u64,
    qpc0: u64,
    tsc_hz: NonZeroU64,
    qpc_hz: NonZeroU64,
}

impl QpcTscMap {
    pub fn new(tsc0: u64, qpc0: u64, tsc_hz: NonZeroU64, qpc_hz: NonZeroU64) -> Self {
        Self {
            tsc0,
            qpc0,
            tsc_hz,
            qpc_hz,
        }
    }

    pub fn qpc_to_tsc(&self, qpc: u64) -> u64 {
        let delta = i128::from(qpc) - i128::from(self.qpc0);
        let tsc = i128::from(self.tsc0)
            + delta * i128::from(self.tsc_hz.get()) / i128::from(self.qpc_hz.get());
        u64::try_from(tsc.max(0)).unwrap_or(u64::MAX)
    }

    pub fn tsc_to_qpc(&self, tsc: u64) -> u64 {
        let delta = i128::from(tsc) - i128::from(self.tsc0);
        let qpc = i128::from(self.qpc0)
            + delta * i128::from(self.qpc_hz.get()) / i128::from(self.tsc_hz.get());
        u64::try_from(qpc.max(0)).unwrap_or(u64::MAX)
    }

    /// Signed error, in ns, of what this map predicted for a newly observed back-to-back pair (observed - predicted).
    pub fn drift_ns(&self, tsc: u64, qpc: u64) -> i64 {
        let diff = i128::from(tsc) - i128::from(self.qpc_to_tsc(qpc));
        let ns = diff * NS_PER_S as i128 / i128::from(self.tsc_hz.get());
        i64::try_from(ns).unwrap_or(if ns < 0 { i64::MIN } else { i64::MAX })
    }
}

/// Keeps a [`QpcTscMap`] fresh: re-estimated every [`RESYNC_SECONDS`], recording how far the old map had drifted.
#[derive(Debug, Clone, Copy)]
pub struct MapTracker {
    map: QpcTscMap,
    last_qpc: u64,
    pub last_drift_ns: Option<i64>,
}

impl MapTracker {
    pub fn new(tsc: u64, qpc: u64, tsc_hz: NonZeroU64, qpc_hz: NonZeroU64) -> Self {
        Self {
            map: QpcTscMap::new(tsc, qpc, tsc_hz, qpc_hz),
            last_qpc: qpc,
            last_drift_ns: None,
        }
    }

    pub fn map(&self) -> &QpcTscMap {
        &self.map
    }

    pub fn due(&self, qpc: u64) -> bool {
        qpc.saturating_sub(self.last_qpc) >= RESYNC_SECONDS.saturating_mul(self.map.qpc_hz.get())
    }

    /// Anchors the map on a new pair and returns (and records) the drift of the previous map.
    pub fn resync(&mut self, tsc: u64, qpc: u64) -> i64 {
        let drift = self.map.drift_ns(tsc, qpc);
        self.map = QpcTscMap::new(tsc, qpc, self.map.tsc_hz, self.map.qpc_hz);
        self.last_qpc = qpc;
        self.last_drift_ns = Some(drift);
        drift
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn nz(v: u64) -> NonZeroU64 {
        NonZeroU64::new(v).unwrap()
    }

    #[test]
    fn no_invariant_tsc_falls_back_to_qpc() {
        assert_eq!(Timebase::from_invariant(true), Timebase::Tsc);
        assert_eq!(Timebase::from_invariant(false), Timebase::Qpc);
    }

    #[test]
    fn cpuid_15_needs_all_three_fields() {
        // Values read on the target (i5-11400H): 38.4 MHz crystal, ratio 140/2.
        assert_eq!(
            freq_from_cpuid_15(2, 140, 38_400_000),
            Some(nz(2_688_000_000))
        );
        for (a, b, c) in [(0, 140, 38_400_000), (2, 0, 38_400_000), (2, 140, 0)] {
            assert_eq!(freq_from_cpuid_15(a, b, c), None, "{a} {b} {c}");
        }
    }

    #[test]
    fn calibration_pair_and_median() {
        // 200 ms at 3 GHz measured with a 10 MHz QPC.
        assert_eq!(
            freq_from_pair(600_000_000, 2_000_000, nz(10_000_000)),
            Some(nz(3_000_000_000))
        );
        assert_eq!(freq_from_pair(1, 0, nz(10_000_000)), None);
        assert_eq!(median(&mut [5, 1, 9, 3, 7]), Some(5));
        assert_eq!(median(&mut []), None);
    }

    #[test]
    fn conversion_boundaries() {
        assert_eq!(ticks_to_ns(0, nz(3)), 0);
        assert_eq!(ticks_to_ns(3_000_000_000, nz(3_000_000_000)), 1_000_000_000);
        // u64::MAX ticks at 1 Hz does not overflow.
        assert_eq!(
            ticks_to_ns(u64::MAX, nz(1)),
            u128::from(u64::MAX) * 1_000_000_000
        );
    }

    /// Independent formula: whole seconds first, then the remainder, so no big product is formed.
    fn reference_ns(ticks: u64, hz: u64) -> u128 {
        let (secs, rem) = (u128::from(ticks / hz), u128::from(ticks % hz));
        secs * NS_PER_S + rem * NS_PER_S / u128::from(hz)
    }

    /// Realistic clocks (QPC 10 MHz up to a 10 GHz TSC) plus the extremes: a uniform `u64` is almost always
    /// astronomically large, which would hide off-by-one errors in the divisor.
    fn any_hz() -> impl Strategy<Value = u64> {
        prop_oneof![1..=100u64, 1_000_000..=10_000_000_000u64, 1..=u64::MAX]
    }

    proptest! {
        #[test]
        fn conversion_matches_the_reference(ticks in any::<u64>(), hz in any_hz()) {
            prop_assert_eq!(ticks_to_ns(ticks, nz(hz)), reference_ns(ticks, hz));
        }

        #[test]
        fn conversion_is_monotonic(a in any::<u64>(), b in any::<u64>(), hz in any_hz()) {
            let (lo, hi) = (a.min(b), a.max(b));
            prop_assert!(ticks_to_ns(lo, nz(hz)) <= ticks_to_ns(hi, nz(hz)));
        }
    }

    #[test]
    fn a_disagreeing_calibration_wins_and_the_gap_is_recorded() {
        let close = choose_baseline(Some(nz(2_688_000_000)), Some(nz(2_688_100_000))).unwrap();
        assert_eq!(
            (close.source, close.tsc_hz),
            (FreqSource::Cpuid15, 2_688_000_000)
        );
        let far = choose_baseline(Some(nz(2_688_000_000)), Some(nz(2_700_000_000))).unwrap();
        assert_eq!(
            (far.source, far.tsc_hz),
            (FreqSource::QpcCalibration, 2_700_000_000)
        );
        assert!(far.gap_ppm.unwrap() > GAP_PPM_LIMIT);
        assert_eq!(
            choose_baseline(None, Some(nz(5))).unwrap().source,
            FreqSource::QpcCalibration
        );
        assert_eq!(
            choose_baseline(Some(nz(5)), None).unwrap().source,
            FreqSource::Cpuid15
        );
        assert!(choose_baseline(None, None).is_none());
    }

    #[test]
    fn baseline_round_trips_through_toml() {
        let b = choose_baseline(Some(nz(2_688_000_000)), Some(nz(2_688_050_000))).unwrap();
        let text = toml::to_string(&b).unwrap();
        assert_eq!(toml::from_str::<ClockBaseline>(&text).unwrap(), b);
    }

    #[test]
    fn map_converts_both_ways_and_tracks_drift_with_a_fake_clock() {
        let (tsc_hz, qpc_hz) = (nz(3_000_000_000), nz(10_000_000));
        let mut t = MapTracker::new(1_000, 500, tsc_hz, qpc_hz);
        assert_eq!(t.map().qpc_to_tsc(500 + 10_000_000), 1_000 + 3_000_000_000);
        assert_eq!(t.map().tsc_to_qpc(1_000 + 3_000_000_000), 500 + 10_000_000);
        assert!(!t.due(500 + 59 * 10_000_000) && t.due(500 + 60 * 10_000_000));
        // The real TSC runs 20 ppm fast: after 60 s it is 3e9 * 60 * 20e-6 = 3_600_000 ticks ahead = 1.2 ms.
        let qpc = 500 + 60 * 10_000_000;
        let tsc = 1_000 + 180_000_000_000 + 3_600_000;
        let drift = t.resync(tsc, qpc);
        assert_eq!(drift, 1_200_000);
        assert_eq!(t.last_drift_ns, Some(1_200_000));
        assert!(!t.due(qpc + 1), "a resync restarts the 60 s interval");
    }
}
