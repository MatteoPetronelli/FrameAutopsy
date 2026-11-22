//! `fa capture compare OURS.csv PRESENTMON.csv [--tolerance-ms 0.1]`: compares the frametimes of `fa capture
//! --frames-csv` with a PresentMon CSV captured at the same time, swap chain by swap chain.
//!
//! PresentMon's time stamps are relative to its own start, so frames are matched by sequence: the shift (in frames)
//! between the two sequences that maximises the number of frametimes equal within the tolerance is searched, then the
//! frames that overlap are compared one by one.

use fa_probe::health::fr_num;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::process::ExitCode;

/// Target of the lot 5 parity criterion (calibrate).
const TARGET_PERCENT: f64 = 99.0;
const MAX_SHIFT: i64 = 700;

pub struct Args {
    ours: String,
    presentmon: String,
    tolerance_ms: f64,
}

pub fn parse(args: &[String]) -> Option<Args> {
    let mut it = args.iter();
    let ours = it.next()?.clone();
    let presentmon = it.next()?.clone();
    let mut tolerance_ms = 0.1;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--tolerance-ms" => {
                tolerance_ms = it
                    .next()?
                    .parse()
                    .ok()
                    .filter(|t: &f64| *t > 0.0 && *t < 100.0)?
            }
            _ => return None,
        }
    }
    Some(Args {
        ours,
        presentmon,
        tolerance_ms,
    })
}

type Key = (u32, String);

fn column(header: &[&str], name: &str) -> io::Result<usize> {
    header
        .iter()
        .position(|h| *h == name)
        .ok_or_else(|| io::Error::other(format!("colonne {name} introuvable")))
}

/// Frametimes in ms per (pid, swap chain), in file order. `pid_col`, `chain_col`, `ms_col` are column names.
fn read_frametimes(
    path: &Path,
    pid_col: &str,
    chain_col: &str,
    ms_col: &str,
) -> io::Result<BTreeMap<Key, Vec<f64>>> {
    let text = std::fs::read_to_string(path)?;
    let mut lines = text
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| io::Error::other("fichier vide"))?
        .split(',')
        .collect();
    let (p, c, m) = (
        column(&header, pid_col)?,
        column(&header, chain_col)?,
        column(&header, ms_col)?,
    );
    let mut out: BTreeMap<Key, Vec<f64>> = BTreeMap::new();
    for line in lines {
        let f: Vec<&str> = line.split(',').collect();
        let (Some(pid), Some(chain), Some(ms)) = (f.get(p), f.get(c), f.get(m)) else {
            continue;
        };
        // PresentMon writes NA for the first present of a swap chain.
        let (Ok(pid), Ok(ms)) = (pid.parse::<u32>(), ms.parse::<f64>()) else {
            continue;
        };
        out.entry((pid, chain.to_ascii_lowercase()))
            .or_default()
            .push(ms);
    }
    Ok(out)
}

#[derive(Debug, PartialEq)]
pub struct Parity {
    pub shift: i64,
    pub overlap: usize,
    pub matched: usize,
    pub diffs_ms: Vec<f64>,
}

/// `ours[i + shift]` is compared with `reference[i]`; the shift with the most matches (then the longest overlap) wins.
pub fn compare(ours: &[f64], reference: &[f64], tolerance_ms: f64) -> Parity {
    let mut best = Parity {
        shift: 0,
        overlap: 0,
        matched: 0,
        diffs_ms: Vec::new(),
    };
    for shift in -MAX_SHIFT..=MAX_SHIFT {
        let (mut overlap, mut matched) = (0, 0);
        for (i, r) in reference.iter().enumerate() {
            let Some(o) = usize::try_from(i as i64 + shift)
                .ok()
                .and_then(|j| ours.get(j))
            else {
                continue;
            };
            overlap += 1;
            if (o - r).abs() <= tolerance_ms {
                matched += 1;
            }
        }
        if matched > best.matched || (matched == best.matched && overlap > best.overlap) {
            best = Parity {
                shift,
                overlap,
                matched,
                diffs_ms: Vec::new(),
            };
        }
    }
    best.diffs_ms = reference
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            usize::try_from(i as i64 + best.shift)
                .ok()
                .and_then(|j| ours.get(j))
                .map(|o| (o - r).abs())
        })
        .collect();
    best.diffs_ms.sort_by(f64::total_cmp);
    best
}

pub fn run(args: Args) -> ExitCode {
    match run_inner(&args) {
        Ok(ok) => {
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("fa capture compare : {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(args: &Args) -> io::Result<bool> {
    let ours = read_frametimes(Path::new(&args.ours), "pid", "swap_chain", "frametime_ms")?;
    let pm = read_frametimes(
        Path::new(&args.presentmon),
        "ProcessID",
        "SwapChainAddress",
        "MsBetweenPresents",
    )?;
    println!(
        "Comparaison des frametimes (tolérance {} ms), chaîne d'échange par chaîne :",
        fr_num(args.tolerance_ms, 2)
    );
    let (mut all_ok, mut compared) = (true, 0);
    for (key, reference) in &pm {
        let Some(mine) = ours.get(key) else { continue };
        if reference.len() < 10 || mine.len() < 10 {
            continue;
        }
        compared += 1;
        let p = compare(mine, reference, args.tolerance_ms);
        let pct = 100.0 * p.matched as f64 / p.overlap.max(1) as f64;
        let q = |f: f64| {
            p.diffs_ms
                .get(
                    ((p.diffs_ms.len() as f64 * f) as usize)
                        .min(p.diffs_ms.len().saturating_sub(1)),
                )
                .copied()
                .unwrap_or(0.0)
        };
        println!(
            "  pid {} chaîne {} : nous {} frames, PresentMon {}, décalage {}, recouvrement {}, concordantes {} ({} %) ; écart médian {} ms, p99 {} ms, max {} ms",
            key.0,
            key.1,
            mine.len(),
            reference.len(),
            p.shift,
            p.overlap,
            p.matched,
            fr_num(pct, 1),
            fr_num(q(0.5), 4),
            fr_num(q(0.99), 3),
            fr_num(q(1.0), 3)
        );
        if pct < TARGET_PERCENT {
            all_ok = false;
        }
    }
    if compared == 0 {
        println!("  aucune chaîne commune d'au moins 10 frames : les deux captures ont-elles tourné en même temps ?");
        return Ok(false);
    }
    println!(
        "Critère : au moins {} % de frames concordantes (à calibrer) : {}.",
        fr_num(TARGET_PERCENT, 0),
        if all_ok { "atteint" } else { "NON atteint" }
    );
    Ok(all_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_sequences_match_fully() {
        let a: Vec<f64> = (0..50).map(|i| 10.0 + f64::from(i % 7)).collect();
        let p = compare(&a, &a, 0.1);
        assert_eq!((p.shift, p.overlap, p.matched), (0, 50, 50));
    }

    #[test]
    fn a_shifted_start_is_found() {
        let ours: Vec<f64> = (0..60)
            .map(|i| 10.0 + f64::from((i * 7919 + i * i * 31) % 997) / 10.0)
            .collect();
        let reference = &ours[5..45];
        let p = compare(&ours, reference, 0.1);
        assert_eq!((p.shift, p.matched, p.overlap), (5, 40, 40));
        // And the other way round: the reference started earlier than ours.
        let p = compare(reference, &ours, 0.1);
        assert_eq!((p.shift, p.matched), (-5, 40));
    }

    #[test]
    fn a_few_different_frames_are_counted_not_hidden() {
        let ours: Vec<f64> = (0..100).map(|i| 8.0 + f64::from(i % 5)).collect();
        let mut reference = ours.clone();
        reference[10] += 5.0;
        reference[11] -= 5.0;
        let p = compare(&ours, &reference, 0.1);
        assert_eq!((p.shift, p.overlap, p.matched), (0, 100, 98));
        assert!((p.diffs_ms.last().unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn arguments_are_validated() {
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert!(parse(&s(&["a.csv", "b.csv"])).is_some());
        assert_eq!(
            parse(&s(&["a.csv", "b.csv", "--tolerance-ms", "0.5"]))
                .unwrap()
                .tolerance_ms,
            0.5
        );
        for bad in [
            &["a.csv"][..],
            &["a", "b", "--tolerance-ms"],
            &["a", "b", "--tolerance-ms", "0"],
            &["a", "b", "--x"],
        ] {
            assert!(parse(&s(bad)).is_none(), "{bad:?}");
        }
    }
}
