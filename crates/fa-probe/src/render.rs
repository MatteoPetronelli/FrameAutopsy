//! The French text report and the JSON of a [`HealthReport`].

use crate::health::{fr_num, Confidence, HealthReport, Verdict};
use std::fmt::Write;

pub fn to_json(r: &HealthReport) -> String {
    // A `HealthReport` has no map with non-string keys and no non-finite number is ever put in it by `fa check`.
    serde_json::to_string_pretty(r).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

fn yes(b: bool) -> &'static str {
    if b {
        "oui"
    } else {
        "non"
    }
}

fn confidence(c: Confidence) -> &'static str {
    match c {
        Confidence::Low => "basse",
        Confidence::Medium => "moyenne",
        Confidence::High => "haute",
    }
}

fn evidence(v: &Verdict) -> String {
    v.evidence
        .iter()
        .map(|(k, x)| format!("{k}={}", fr_num(*x, 2)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn opt(v: Option<f64>, unit: &str) -> String {
    v.map_or("inconnu".to_owned(), |x| format!("{} {unit}", fr_num(x, 1)))
}

/// Median of the curve over blocks of 5 s, one line per block.
fn curve_lines(curve: &[f64]) -> Vec<String> {
    curve
        .chunks(5)
        .enumerate()
        .map(|(i, c)| {
            let mut s = c.to_vec();
            s.sort_by(f64::total_cmp);
            let (a, b) = (i * 5, i * 5 + c.len() - 1);
            format!("    {a:>2}-{b:<2} s : {} GHz", fr_num(s[s.len() / 2], 2))
        })
        .collect()
}

/// Latency at the working set closest to `kib`.
fn latency_near(r: &HealthReport, kib: u64) -> Option<f64> {
    r.memory
        .latency_curve
        .iter()
        .min_by_key(|p| p.kib.abs_diff(kib))
        .map(|p| p.ns_per_load)
}

pub fn render_fr(r: &HealthReport) -> String {
    let mut s = String::new();
    let w = &mut s;
    let _ = writeln!(
        w,
        "FrameAutopsy : bilan de santé de la machine ({})",
        r.timestamp
    );
    let _ = writeln!(w);

    let p = &r.power;
    let source = match p.source.as_str() {
        "ac" => "secteur",
        "battery" => "BATTERIE (la baseline n'est pas mise à jour)",
        _ => "inconnue",
    };
    let _ = writeln!(w, "Alimentation");
    let _ = writeln!(w, "  source : {source}");
    let _ = writeln!(w, "  plan : {}", p.scheme);
    let _ = writeln!(
        w,
        "  mode (overlay) : {}",
        p.overlay.as_deref().unwrap_or("inconnu")
    );

    let c = &r.cpu;
    let _ = writeln!(w, "\nProcesseur");
    let _ = writeln!(
        w,
        "  {} : {} cœurs, {} threads",
        c.model, c.physical_cores, c.logical_cpus
    );
    let _ = writeln!(
        w,
        "  TSC invariant : {} ({} GHz) ; AVX2 {}, AVX-512 {}, WAITPKG {}, MWAITX {}",
        yes(c.invariant_tsc),
        fr_num(c.tsc_hz as f64 / 1e9, 3),
        yes(c.avx2),
        yes(c.avx512),
        yes(c.waitpkg),
        yes(c.mwaitx)
    );

    let f = &r.frequency;
    let _ = writeln!(w, "\nFréquence");
    let peaks: Vec<String> = f
        .single_core_peak_ghz
        .iter()
        .map(|(cpu, g)| format!("CPU {cpu} : {}", fr_num(*g, 2)))
        .collect();
    let _ = writeln!(w, "  pic mono-cœur (GHz) : {}", peaks.join(" ; "));
    let _ = writeln!(
        w,
        "  charge FMA {} pendant {} s sur {} CPU (un par autre cœur physique, le jumeau du CPU mesuré reste libre) ; mesure sur le CPU {}",
        f.load_isa,
        f.curve_ghz.len(),
        f.load_cpus.len(),
        f.sampler_cpu
    );
    for l in curve_lines(&f.curve_ghz) {
        let _ = writeln!(w, "{l}");
    }

    let m = &r.memory;
    let _ = writeln!(w, "\nMémoire");
    let _ = writeln!(
        w,
        "  latence (pages de {}) : L1 {} ; L2 {} ; RAM {}",
        m.page_size,
        opt(latency_near(r, 32), "ns"),
        opt(latency_near(r, 512), "ns"),
        opt(latency_near(r, 65_536), "ns"),
    );
    let _ = writeln!(
        w,
        "  débit sur {} cœurs : lecture {} ; écriture {} ; copie {}",
        m.bandwidth_threads,
        opt(m.read_gbs, "Go/s"),
        opt(m.write_gbs, "Go/s"),
        opt(m.copy_gbs, "Go/s")
    );
    // cache sizes against the topology are in the JSON and in `fa probe curve`, not here: the coarse sweep of
    // `fa check` flags "gaps" on healthy machines and that reads like a fault.
    match (&m.modules, &m.modules_error) {
        (Some(mods), _) => {
            let _ = writeln!(w, "  modules (WMI) : {}", mods.len());
            for x in mods {
                let _ = writeln!(
                    w,
                    "    {} Gio, configuré {} MT/s, nominal {} MT/s, banque {}",
                    x.capacity_bytes >> 30,
                    x.configured_speed.map_or("?".to_owned(), |v| v.to_string()),
                    x.rated_speed.map_or("?".to_owned(), |v| v.to_string()),
                    x.bank
                );
            }
        }
        (None, e) => {
            let _ = writeln!(
                w,
                "  modules (WMI) : inconnu ({})",
                e.as_deref().unwrap_or("pas de réponse")
            );
        }
    }

    let _ = writeln!(w, "\nOrdonnancement");
    match &r.scheduling {
        Some(x) => {
            let _ = writeln!(
                w,
                "  réveil d'un timer haute résolution (1 ms) : p50 {:.0} µs, p99 {} ms, max {} ms",
                x.p50_us,
                fr_num(x.p99_us / 1000.0, 2),
                fr_num(x.max_us / 1000.0, 2)
            );
        }
        None => {
            let _ = writeln!(w, "  non mesuré");
        }
    }

    if let Some(g) = &r.gpu {
        let _ = writeln!(
            w,
            "\nGPU (NVML ; la lecture peut réveiller un GPU en veille)"
        );
        let _ = writeln!(
            w,
            "  {} : état {}, limite de puissance {}",
            g.name,
            g.performance_state,
            opt(g.power_limit_watts, "W")
        );
    }

    let _ = writeln!(w, "\nVerdicts");
    let fired: Vec<&Verdict> = r.verdicts.iter().filter(|v| v.fired).collect();
    if fired.is_empty() {
        let _ = writeln!(w, "  Aucun problème trouvé. Ce qui a été vérifié :");
        for check in &r.checks {
            let _ = writeln!(w, "   - {check}");
        }
        for v in &r.verdicts {
            let _ = writeln!(
                w,
                "   > {} (confiance {})",
                v.text,
                confidence(v.confidence)
            );
        }
    } else {
        for v in &fired {
            let _ = writeln!(
                w,
                "  [!] {} (confiance {})",
                v.text,
                confidence(v.confidence)
            );
            let _ = writeln!(w, "      preuves : {}", evidence(v));
        }
        for v in r.verdicts.iter().filter(|v| !v.fired) {
            let _ = writeln!(
                w,
                "  [ok] {} (confiance {})",
                v.text,
                confidence(v.confidence)
            );
        }
    }

    let _ = writeln!(
        w,
        "\nLimites : seuls les jeux de la matrice sont détectés avant le test ; les seuils sont des valeurs de départ."
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::fixtures;
    use std::path::PathBuf;

    /// Compares `actual` with `tests/golden/<name>`; `UPDATE_GOLDEN=1` rewrites the file (review the diff before commit).
    fn golden(name: &str, actual: &str) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(name);
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, actual).unwrap();
        }
        let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (run with UPDATE_GOLDEN=1 to create it)",
                path.display()
            )
        });
        // golden files may be checked out with CRLF
        assert_eq!(
            want.replace("\r\n", "\n"),
            actual,
            "golden mismatch: {name}"
        );
    }

    #[test]
    fn the_healthy_report_text_is_stable_and_lists_what_was_checked() {
        let t = render_fr(&fixtures::healthy());
        assert!(t.contains("Aucun problème trouvé. Ce qui a été vérifié :"));
        golden("healthy.txt", &t);
    }

    #[test]
    fn the_throttling_report_text_is_stable_and_names_the_onset() {
        let t = render_fr(&fixtures::throttling_laptop());
        assert!(t.contains("bridage après 25 s, −26 %"));
        assert!(t.contains("[!] "));
        golden("throttling.txt", &t);
    }

    #[test]
    fn the_json_is_stable_and_round_trips() {
        for (name, r) in [
            ("healthy.json", fixtures::healthy()),
            ("throttling.json", fixtures::throttling_laptop()),
        ] {
            let j = to_json(&r);
            golden(name, &j);
            assert_eq!(serde_json::from_str::<HealthReport>(&j).unwrap(), r);
        }
    }

    #[test]
    fn missing_inputs_are_printed_as_unknown_not_hidden() {
        let mut r = fixtures::healthy();
        r.memory.modules = None;
        r.memory.modules_error = Some("WMI did not answer within 3 s".into());
        r.memory.read_gbs = None;
        r.scheduling = None;
        r.gpu = None;
        let t = render_fr(&r);
        assert!(t.contains("modules (WMI) : inconnu (WMI did not answer within 3 s)"));
        assert!(t.contains("lecture inconnu"));
        assert!(t.contains(
            "Ordonnancement
  non mesuré"
        ));
        assert!(!t.contains("GPU"));
    }

    #[test]
    fn a_battery_run_says_so_in_capitals() {
        let mut r = fixtures::healthy();
        r.power.source = "battery".into();
        assert!(render_fr(&r).contains("BATTERIE"));
    }
}
