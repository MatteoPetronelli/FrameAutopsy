//! Memory latency probe and cache curve (ASM-03 kernel in `fa-asm`). Safe code only: the ring, the permutation and the
//! plateau detection live here, the loads live in `fa-asm::chase`.

use fa_asm::chase::{ChaseRing, LinkError};
use fa_win::mem::{PageBuf, PageSize};
use fa_win::topology::{Cache, CacheKind};
use fa_win::{clock::Clock, thread::pin_current_thread};
use std::io;

/// Cache line size: one slot per line, so every load touches a new line.
pub const LINE_BYTES: usize = 64;

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Sattolo's algorithm: a uniformly random permutation made of ONE cycle, returned as a successor table
/// (`next[i]` follows `i`). A plain shuffle can leave short cycles whose data fits in cache and hides the latency.
pub fn sattolo(n: usize, seed: u64) -> Vec<u32> {
    let mut next: Vec<u32> = (0..n as u32).collect();
    let mut state = seed;
    for i in (1..n).rev() {
        let j = (splitmix64(&mut state) % i as u64) as usize; // 0 <= j < i: never i itself
        next.swap(i, j);
    }
    next
}

/// One latency measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Latency {
    pub bytes: usize,
    pub ns_per_load: f64,
    pub page: PageSize,
}

/// Median ns per load of a ring over `bytes` (rounded down to whole lines) living in `buf`.
pub fn measure(buf: &mut PageBuf, bytes: usize, clock: &Clock, runs: usize) -> io::Result<Latency> {
    let page = buf.page_size();
    let slots = (bytes / LINE_BYTES).max(2);
    let stride_words = LINE_BYTES / size_of::<usize>();
    let next = sattolo(slots, 0xF4A3_5EED ^ slots as u64);
    let ring = ChaseRing::link(buf.as_mut_slice(), &next, stride_words).map_err(|e| match e {
        LinkError::TooSmall => io::Error::other("buffer too small for the working set"),
        e => io::Error::other(format!("{e:?}")),
    })?;
    let loads = (slots as u64).clamp(1_000_000, 4_000_000);
    ring.timed(slots as u64); // one lap warms the caches and the TLB
    let mut per_load: Vec<f64> = (0..runs.max(1))
        .map(|_| {
            let (done, ticks) = ring.timed(loads);
            clock.ticks_to_ns(ticks) as f64 / done as f64
        })
        .collect();
    per_load.sort_by(f64::total_cmp);
    Ok(Latency {
        bytes: slots * LINE_BYTES,
        ns_per_load: per_load[per_load.len() / 2],
        page,
    })
}

/// Working sets of the cache curve: 16 KiB to 256 MiB, powers of two and their 1.5x neighbours.
pub fn curve_sizes() -> Vec<usize> {
    let mut v = Vec::new();
    let mut s = 16 << 10;
    while s <= 256 << 20 {
        v.push(s);
        if s < 256 << 20 {
            v.push(s + s / 2);
        }
        s *= 2;
    }
    v
}

/// Measures the whole curve on `cpu` (pinned), allocating once for the largest set.
pub fn cache_curve(cpu: u32, try_large: bool) -> io::Result<Vec<Latency>> {
    let _pin = pin_current_thread(cpu)?;
    let clock = Clock::new();
    let sizes = curve_sizes();
    let mut buf = PageBuf::alloc(*sizes.last().unwrap(), try_large)?;
    sizes
        .iter()
        .map(|&s| measure(&mut buf, s, &clock, 3))
        .collect()
}

/// A latency step in a curve: the last working set that still ran fast, and how much slower the next plateau is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub bytes: usize,
    pub rise: f64,
}

/// Plateau detection. A transition is a run of consecutive points each more than 12 % slower than the previous one;
/// it counts when the whole run is at least 1.5x slower. `Step::bytes` is the working set just before the run:
/// the cache size, to within one point of the curve.
pub fn detect_steps(curve: &[(usize, f64)]) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut i = 0;
    while i + 1 < curve.len() {
        if curve[i + 1].1 > curve[i].1 * 1.12 {
            let start = i;
            while i + 1 < curve.len() && curve[i + 1].1 > curve[i].1 * 1.12 {
                i += 1;
            }
            let rise = curve[i].1 / curve[start].1;
            if rise >= 1.5 {
                steps.push(Step {
                    bytes: curve[start].0,
                    rise,
                });
            }
        } else {
            i += 1;
        }
    }
    steps
}

/// One cache level of the topology against the closest detected step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelCheck {
    pub level: u8,
    pub topology_bytes: u32,
    pub detected_bytes: Option<usize>,
    /// Detected size within a factor 1.6 of the reported size (the curve has points every 1.5x).
    pub agrees: bool,
}

/// Compares detected steps with the data and unified caches of the topology. A mismatch is reported, never fatal.
pub fn compare_with_topology(steps: &[Step], caches: &[Cache]) -> Vec<LevelCheck> {
    let mut levels: Vec<(u8, u32)> = caches
        .iter()
        .filter(|c| matches!(c.kind, CacheKind::Data | CacheKind::Unified))
        .map(|c| (c.level, c.size_bytes))
        .collect();
    levels.sort_unstable();
    levels.dedup();
    levels
        .into_iter()
        .map(|(level, size)| {
            let near = steps
                .iter()
                .min_by_key(|s| s.bytes.abs_diff(size as usize))
                .map(|s| s.bytes);
            let agrees =
                near.is_some_and(|b| (size as f64 / 1.6..=size as f64 * 1.6).contains(&(b as f64)));
            LevelCheck {
                level,
                topology_bytes: size,
                detected_bytes: near,
                agrees,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Follows `next` from 0: returns the number of steps to come back to 0.
    fn cycle_len(next: &[u32]) -> usize {
        let (mut i, mut n) = (next[0] as usize, 1);
        while i != 0 {
            i = next[i] as usize;
            n += 1;
        }
        n
    }

    #[test]
    fn sattolo_gives_one_single_cycle_for_many_sizes_and_seeds() {
        for n in [2usize, 3, 4, 10, 64, 1000, 4096] {
            for seed in [0u64, 1, 42, u64::MAX] {
                let next = sattolo(n, seed);
                assert_eq!(cycle_len(&next), n, "n={n} seed={seed}");
            }
        }
    }

    #[test]
    fn sattolo_is_deterministic_and_has_no_fixed_point() {
        assert_eq!(sattolo(100, 7), sattolo(100, 7));
        assert_ne!(sattolo(100, 7), sattolo(100, 8));
        assert!(sattolo(100, 7)
            .iter()
            .enumerate()
            .all(|(i, &x)| i != x as usize));
    }

    #[test]
    fn the_cycle_check_rejects_a_permutation_with_short_cycles() {
        // identity is a permutation but not one cycle: the check measures something
        let identity: Vec<u32> = (0..10).collect();
        assert_eq!(cycle_len(&identity), 1);
    }

    fn synthetic() -> Vec<(usize, f64)> {
        // L1 48 KiB at 1 ns, L2 1.25 MiB at 3 ns, L3 12 MiB at 10 ns, RAM at 80 ns, with smooth transitions
        vec![
            (16 << 10, 1.0),
            (24 << 10, 1.0),
            (32 << 10, 1.0),
            (48 << 10, 1.05),
            (64 << 10, 2.0),
            (96 << 10, 3.0),
            (512 << 10, 3.0),
            (1 << 20, 3.1),
            (1536 << 10, 5.0),
            (2 << 20, 8.0),
            (8 << 20, 10.0),
            (12 << 20, 10.5),
            (16 << 20, 30.0),
            (32 << 20, 70.0),
            (64 << 20, 80.0),
            (256 << 20, 82.0),
        ]
    }

    #[test]
    fn three_steps_are_found_on_a_synthetic_curve() {
        let s = detect_steps(&synthetic());
        let sizes: Vec<usize> = s.iter().map(|s| s.bytes).collect();
        assert_eq!(sizes, vec![48 << 10, 1 << 20, 12 << 20]);
    }

    #[test]
    fn a_flat_curve_has_no_step_and_a_small_rise_is_not_a_step() {
        assert!(detect_steps(&[(1, 5.0), (2, 5.1), (3, 5.2)]).is_empty());
        assert!(detect_steps(&[(1, 5.0), (2, 6.0), (3, 7.0)]).is_empty()); // 1.4x in total
        assert!(detect_steps(&[]).is_empty());
    }

    #[test]
    fn comparison_with_the_topology_accepts_close_sizes_and_flags_far_ones() {
        let c = |level, size_bytes| Cache {
            level,
            size_bytes,
            kind: CacheKind::Unified,
            group_mask: 1,
        };
        let steps = [
            Step {
                bytes: 48 << 10,
                rise: 3.0,
            },
            Step {
                bytes: 4 << 20,
                rise: 9.0,
            },
        ];
        let r = compare_with_topology(&steps, &[c(1, 48 << 10), c(2, 1280 << 10)]);
        assert!(r[0].agrees && r[0].detected_bytes == Some(48 << 10));
        assert!(!r[1].agrees, "4 MiB is not 1.25 MiB");
    }

    #[test]
    fn the_sizes_go_from_16_kib_to_256_mib() {
        let v = curve_sizes();
        assert_eq!((v[0], *v.last().unwrap()), (16 << 10, 256 << 20));
        assert!(v.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_real_small_measurement_is_plausible() {
        let mut buf = PageBuf::alloc(1 << 20, false).unwrap();
        let clock = Clock::new();
        let l = measure(&mut buf, 32 << 10, &clock, 3).unwrap();
        assert!((0.2..50.0).contains(&l.ns_per_load), "{} ns", l.ns_per_load);
        assert_eq!(l.page, PageSize::Small);
    }
}
