//! The three verdict rules of `fa check`, as pure functions from measurements to a [`Verdict`] with its evidence.
//! Starting thresholds, all `(calibrate)`: they are frozen after the user has seen real AC and battery reports.
//! A missing input never fires a verdict: it lowers the confidence and the text says the value is unknown.

use crate::health::{Confidence, Verdict};
use crate::sysinfo::{theoretical_gbs, MemoryModule};
use crate::wake::WakeStats;
use std::collections::BTreeMap;

/// Seconds at the start of the load that make the reference clock.
pub const BASELINE_SECONDS: usize = 5;
/// A sustained clock this far below the reference counts as throttling.
pub const THROTTLE_DROP: f64 = 0.10;
/// Window of the moving median that decides "sustained".
pub const MOVING_SECONDS: usize = 3;
/// Read bandwidth under this fraction of the dual-channel theoretical value confirms a single channel.
pub const RAM_FRACTION: f64 = 0.60;
/// Wake-up p99 above this (microseconds) means scheduling interference. The idle target machine measures about 2 to 3 ms.
pub const WAKE_P99_US: f64 = 5_000.0;

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[s.len() / 2]
}

fn verdict(
    id: &str,
    fired: bool,
    confidence: Confidence,
    ev: &[(&str, f64)],
    text: String,
) -> Verdict {
    Verdict {
        id: id.to_owned(),
        fired,
        confidence,
        evidence: ev
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect::<BTreeMap<_, _>>(),
        text,
    }
}

/// Throttling from the per-second clock of the loaded run, in GHz.
///
/// Reference = median of the first [`BASELINE_SECONDS`]. A sustained drop = a [`MOVING_SECONDS`]-second moving median,
/// over windows that start after the reference, below `(1 - THROTTLE_DROP)` of it. "Bridage après X s" counts the
/// seconds before the first low second of the first low window; "−Y %" is the worst sustained level against the
/// reference. High confidence when the clock is still low at the end, medium when it recovered.
pub fn throttling(curve_ghz: &[f64], on_battery: bool) -> Verdict {
    let batt = if on_battery { " (sur batterie)" } else { "" };
    if curve_ghz.len() < BASELINE_SECONDS + MOVING_SECONDS + 2 {
        return verdict(
            "throttling",
            false,
            Confidence::Low,
            &[("seconds", curve_ghz.len() as f64)],
            format!(
                "Mesure du bridage trop courte ({} s) : pas de verdict.",
                curve_ghz.len()
            ),
        );
    }
    let base = median(&curve_ghz[..BASELINE_SECONDS]);
    let limit = base * (1.0 - THROTTLE_DROP);
    let moving: Vec<f64> = (BASELINE_SECONDS..=curve_ghz.len() - MOVING_SECONDS)
        .map(|i| median(&curve_ghz[i..i + MOVING_SECONDS]))
        .collect();
    let worst = moving.iter().copied().fold(f64::INFINITY, f64::min);
    let drop_pct = ((1.0 - worst / base) * 100.0).max(0.0);
    let ev = [
        ("baseline_ghz", base),
        ("worst_sustained_ghz", worst),
        ("drop_percent", drop_pct),
        ("threshold_percent", THROTTLE_DROP * 100.0),
        ("on_battery", f64::from(on_battery)),
    ];
    let Some(pos) = moving.iter().position(|m| *m < limit) else {
        let conf = if curve_ghz.len() >= 60 {
            Confidence::High
        } else {
            Confidence::Medium
        };
        return verdict(
            "throttling",
            false,
            conf,
            &ev,
            format!(
                "Pas de bridage : fréquence stable autour de {base:.2} GHz (pire écart −{drop_pct:.0} %){batt}."
            ),
        );
    };
    let start = BASELINE_SECONDS + pos;
    let onset = start
        + curve_ghz[start..start + MOVING_SECONDS]
            .iter()
            .position(|v| *v < limit)
            .unwrap_or(0);
    let recovered = moving.last().is_some_and(|m| *m >= limit);
    let conf = if recovered {
        Confidence::Medium
    } else {
        Confidence::High
    };
    let mut ev = ev.to_vec();
    ev.push(("onset_s", onset as f64));
    verdict(
        "throttling",
        true,
        conf,
        &ev,
        format!(
            "bridage après {onset} s, −{drop_pct:.0} % ({base:.1} → {worst:.1} GHz){batt}{}",
            if recovered {
                " ; la fréquence est ensuite remontée."
            } else {
                "."
            }
        ),
    )
}

/// Single-channel RAM. The module count decides: one module fires (high confidence when the all-core read bandwidth
/// is also under [`RAM_FRACTION`] of the dual-channel theoretical value, medium otherwise); several modules do not fire.
/// Without WMI there is no module count and no configured speed, so there is no verdict.
pub fn single_channel_ram(modules: Option<&[MemoryModule]>, read_gbs: Option<f64>) -> Verdict {
    let Some(mods) = modules.filter(|m| !m.is_empty()) else {
        return verdict(
            "single_channel_ram",
            false,
            Confidence::Low,
            &[],
            "Modules mémoire inconnus (WMI) : pas de verdict sur la mémoire.".to_owned(),
        );
    };
    let speed = mods
        .iter()
        .filter_map(|m| m.configured_speed.or(m.rated_speed))
        .max();
    let theo = speed.map(|s| theoretical_gbs(s, 2));
    let bw_low = match (read_gbs, theo) {
        (Some(r), Some(t)) => Some(r < RAM_FRACTION * t),
        _ => None,
    };
    let mut ev = vec![
        ("modules", mods.len() as f64),
        ("threshold_fraction", RAM_FRACTION),
    ];
    if let Some(r) = read_gbs {
        ev.push(("read_gbs", r));
    }
    if let Some(t) = theo {
        ev.push(("dual_channel_theoretical_gbs", t));
    }
    if mods.len() == 1 {
        let conf = if bw_low == Some(true) {
            Confidence::High
        } else {
            Confidence::Medium
        };
        let detail = match (read_gbs, theo) {
            (Some(r), Some(t)) => {
                format!(" Lecture mesurée {r:.0} Go/s pour {t:.0} Go/s en double canal.")
            }
            _ => String::new(),
        };
        return verdict(
            "single_channel_ram",
            true,
            conf,
            &ev,
            format!("Mémoire probablement en simple canal : un seul module détecté.{detail}"),
        );
    }
    verdict(
        "single_channel_ram",
        false,
        Confidence::Medium,
        &ev,
        format!(
            "Pas de signe de mémoire en simple canal : {} modules détectés.",
            mods.len()
        ),
    )
}

/// Scheduling interference from the wake-up latency of the high-resolution timer.
pub fn scheduling(wake: Option<&WakeStats>) -> Verdict {
    let Some(w) = wake else {
        return verdict(
            "scheduling_interference",
            false,
            Confidence::Low,
            &[],
            "Latence de réveil non mesurée : pas de verdict sur l'ordonnancement.".to_owned(),
        );
    };
    let ev = [
        ("p50_us", w.p50_us),
        ("p99_us", w.p99_us),
        ("max_us", w.max_us),
        ("threshold_p99_us", WAKE_P99_US),
    ];
    if w.p99_us > WAKE_P99_US {
        verdict(
            "scheduling_interference",
            true,
            Confidence::Medium,
            &ev,
            format!(
                "Interférence d'ordonnancement probable : réveil p99 = {:.1} ms (seuil {:.0} ms). Fermez les autres applications et recommencez.",
                w.p99_us / 1000.0,
                WAKE_P99_US / 1000.0
            ),
        )
    } else {
        verdict(
            "scheduling_interference",
            false,
            Confidence::Medium,
            &ev,
            format!(
                "Ordonnancement normal : réveil p50 = {:.0} µs, p99 = {:.1} ms.",
                w.p50_us,
                w.p99_us / 1000.0
            ),
        )
    }
}

/// All three verdicts, in report order.
pub fn evaluate(
    curve_ghz: &[f64],
    on_battery: bool,
    modules: Option<&[MemoryModule]>,
    read_gbs: Option<f64>,
    wake: Option<&WakeStats>,
) -> Vec<Verdict> {
    vec![
        throttling(curve_ghz, on_battery),
        single_channel_ram(modules, read_gbs),
        scheduling(wake),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 25 s at 4.2 GHz then 35 s at 3.1: the spec scenario.
    fn laptop() -> Vec<f64> {
        let mut v = vec![4.2; 25];
        v.extend(vec![3.1; 35]);
        v
    }

    fn module(speed: u32) -> MemoryModule {
        MemoryModule {
            capacity_bytes: 16 << 30,
            configured_speed: Some(speed),
            rated_speed: Some(speed),
            bank: "BANK 0".into(),
        }
    }

    fn wake(p50: f64, p99: f64) -> WakeStats {
        WakeStats {
            n: 2000,
            p50_us: p50,
            p99_us: p99,
            max_us: p99 * 2.0,
        }
    }

    #[test]
    fn the_spec_scenario_says_bridage_apres_25_s_moins_26_pourcent() {
        let v = throttling(&laptop(), false);
        assert!(v.fired);
        assert!(
            v.text.starts_with("bridage après 25 s, −26 %"),
            "{}",
            v.text
        );
        assert_eq!(v.confidence, Confidence::High);
        assert_eq!(v.evidence["onset_s"], 25.0);
    }

    #[test]
    fn a_healthy_flat_curve_does_not_fire_and_says_so() {
        let v = throttling(&vec![4.1; 60], false);
        assert!(!v.fired && v.text.starts_with("Pas de bridage"));
        assert_eq!(v.confidence, Confidence::High);
    }

    #[test]
    fn a_recovery_lowers_the_confidence_to_medium() {
        let mut c = vec![4.2; 20];
        c.extend(vec![3.0; 10]);
        c.extend(vec![4.2; 30]);
        let v = throttling(&c, false);
        assert!(v.fired && v.confidence == Confidence::Medium, "{v:?}");
        assert!(v.text.contains("remontée"));
    }

    #[test]
    fn the_ten_percent_threshold_is_strict() {
        // 4.0 reference; a sustained 3.61 is -9.75 % (no), 3.59 is -10.25 % (yes)
        let mut ok = vec![4.0; 10];
        ok.extend(vec![3.61; 20]);
        assert!(!throttling(&ok, false).fired);
        let mut bad = vec![4.0; 10];
        bad.extend(vec![3.59; 20]);
        assert!(throttling(&bad, false).fired);
    }

    #[test]
    fn a_one_second_dip_is_not_sustained() {
        let mut c = vec![4.0; 30];
        c[15] = 2.0;
        assert!(
            !throttling(&c, false).fired,
            "the 3 s moving median hides one bad second"
        );
    }

    #[test]
    fn the_first_five_seconds_are_the_reference_not_part_of_the_drop() {
        // starts low for 5 s (turbo not yet reached) then settles higher: the reference is the first 5 s
        let mut c = vec![3.0; 5];
        c.extend(vec![4.0; 40]);
        assert!(!throttling(&c, false).fired, "a rise is not throttling");
        // known limit: a drop already under way within the first 5 s is absorbed by the reference median
        let mut d = vec![4.0, 3.0, 3.0, 3.0, 3.0];
        d.extend(vec![3.0; 40]);
        assert!(!throttling(&d, false).fired);
    }

    #[test]
    fn the_reference_is_exactly_the_first_five_seconds() {
        // first 5 s: median 4.0; seconds 6 to 8 are low. A reference over 8 s would be 3.0 and hide the drop.
        let mut c = vec![4.0, 4.0, 4.0, 3.0, 3.0];
        c.extend(vec![3.0; 40]);
        assert!(throttling(&c, false).fired);
    }

    #[test]
    fn a_clock_exactly_at_the_limit_is_not_throttling() {
        let mut c = vec![1.0; 10];
        c.extend(vec![0.9; 20]); // exactly 10 % below
        assert!(!throttling(&c, false).fired);
    }

    #[test]
    fn a_curve_that_is_too_short_gives_no_verdict() {
        let v = throttling(&[4.0; 8], false);
        assert!(!v.fired && v.confidence == Confidence::Low);
    }

    #[test]
    fn battery_is_named_in_the_text_and_the_evidence() {
        let v = throttling(&laptop(), true);
        assert!(v.text.contains("sur batterie") && v.evidence["on_battery"] == 1.0);
    }

    #[test]
    fn one_module_fires_ram_with_confidence_following_the_bandwidth() {
        let m = [module(3200)];
        // 51.2 GB/s theoretical, 60 % = 30.72
        let high = single_channel_ram(Some(&m), Some(25.0));
        assert!(high.fired && high.confidence == Confidence::High);
        let med = single_channel_ram(Some(&m), Some(35.0));
        assert!(med.fired && med.confidence == Confidence::Medium);
        let unknown_bw = single_channel_ram(Some(&m), None);
        assert!(unknown_bw.fired && unknown_bw.confidence == Confidence::Medium);
    }

    #[test]
    fn two_modules_never_fire_even_with_a_low_bandwidth() {
        let m = [module(3200), module(3200)];
        assert!(!single_channel_ram(Some(&m), Some(5.0)).fired);
        assert!(!single_channel_ram(Some(&m), Some(32.0)).fired);
    }

    #[test]
    fn the_sixty_percent_boundary() {
        let m = [module(3200)];
        let t = 51.2 * RAM_FRACTION;
        assert_eq!(
            single_channel_ram(Some(&m), Some(t - 0.1)).confidence,
            Confidence::High
        );
        assert_eq!(
            single_channel_ram(Some(&m), Some(t + 0.1)).confidence,
            Confidence::Medium
        );
    }

    #[test]
    fn without_wmi_there_is_no_ram_verdict() {
        for m in [None, Some(&[][..])] {
            let v = single_channel_ram(m, Some(5.0));
            assert!(!v.fired && v.confidence == Confidence::Low);
        }
    }

    #[test]
    fn the_wake_threshold_is_five_milliseconds_on_the_p99() {
        assert!(!scheduling(Some(&wake(500.0, 5_000.0))).fired);
        assert!(scheduling(Some(&wake(500.0, 5_001.0))).fired);
        // the idle target machine (p50 0.5 ms, p99 2 to 3 ms) must not fire
        assert!(!scheduling(Some(&wake(520.0, 3_300.0))).fired);
        assert!(!scheduling(None).fired);
    }

    #[test]
    fn a_healthy_desktop_fires_nothing() {
        let m = [module(3200), module(3200)];
        let v = evaluate(
            &vec![4.1; 60],
            false,
            Some(&m),
            Some(32.0),
            Some(&wake(520.0, 2_500.0)),
        );
        assert_eq!(v.len(), 3);
        assert!(v.iter().all(|x| !x.fired), "{v:?}");
    }
}
