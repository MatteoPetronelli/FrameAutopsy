//! Clock-domain check (D9): does the clock of core A change when core B is loaded?
//!
//! From user mode this tells "coupled" (a shared clock domain, or a shared turbo/power limit that lowers every core
//! when more cores are busy) from "independent". It cannot tell those two causes apart.

use crate::freq;
use fa_win::thread::pin_current_thread;
use std::io;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A change of the median clock above this fraction counts as "followed the load" (calibrate).
pub const FOLLOW_LIMIT: f64 = 0.03;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    /// Core A kept its clock while core B was loaded.
    Independent,
    /// Core A's clock moved by more than [`FOLLOW_LIMIT`] when core B was loaded.
    Coupled,
}

/// Decision from the median clock of A with B idle and with B loaded (both in Hz, both positive).
pub fn decide(a_b_idle_hz: f64, a_b_loaded_hz: f64) -> Domain {
    if (a_b_idle_hz - a_b_loaded_hz).abs() / a_b_idle_hz > FOLLOW_LIMIT {
        Domain::Coupled
    } else {
        Domain::Independent
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DomainReport {
    pub idle_hz: f64,
    pub loaded_hz: f64,
    pub verdict: Domain,
}

fn median_of(batches: &[f64]) -> f64 {
    let mut v = batches.to_vec();
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Alternates `rounds` times: probe A for `batch` with B idle, then with B spinning (our own thread pinned on B).
pub fn check(
    a: u32,
    b: u32,
    tsc_hz: NonZeroU64,
    batch: Duration,
    rounds: usize,
) -> io::Result<DomainReport> {
    let (mut idle, mut loaded) = (Vec::new(), Vec::new());
    for _ in 0..rounds {
        if let Some(x) = freq::run_batch(a, batch, tsc_hz)? {
            idle.push(x.median_hz);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let spin = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let _pin = pin_current_thread(b);
                let mut x = 3u64;
                while !stop.load(Ordering::Relaxed) {
                    for _ in 0..100_000 {
                        x = std::hint::black_box(x.wrapping_mul(x));
                    }
                }
                x
            })
        };
        std::thread::sleep(Duration::from_millis(300)); // let the load settle
        let r = freq::run_batch(a, batch, tsc_hz);
        stop.store(true, Ordering::Relaxed);
        let _ = spin.join();
        if let Some(x) = r? {
            loaded.push(x.median_hz);
        }
    }
    if idle.is_empty() || loaded.is_empty() {
        return Err(io::Error::other("no usable samples"));
    }
    let (idle_hz, loaded_hz) = (median_of(&idle), median_of(&loaded));
    Ok(DomainReport {
        idle_hz,
        loaded_hz,
        verdict: decide(idle_hz, loaded_hz),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_above_three_percent_is_coupled() {
        assert_eq!(decide(4.0e9, 3.87e9), Domain::Coupled);
        assert_eq!(decide(4.0e9, 3.89e9), Domain::Independent);
    }

    #[test]
    fn a_rise_counts_too() {
        assert_eq!(decide(3.0e9, 3.2e9), Domain::Coupled);
    }

    #[test]
    fn the_median_of_three_ignores_one_odd_batch() {
        assert_eq!(median_of(&[4.0e9, 1.0e9, 4.1e9]), 4.0e9);
    }
}
