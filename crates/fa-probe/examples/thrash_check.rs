//! Throwaway spike for ASM-07 (lot 6): does a cache sweep with `clflush` make a victim's memory latency jump, and by how
//! much more than a plain sweep? Victim: a 2 MiB pointer-chase ring on CPU 2. Thrasher: a 64 MiB sweep on CPU 4 (another
//! physical core, same last-level cache). Deleted once the verdict is in the catalog (the history keeps it).
//!   cargo run -p fa-probe --release --example thrash_check

use fa_asm::cache::sweep;
use fa_asm::chase::ChaseRing;
use fa_probe::mem::{sattolo, LINE_BYTES};
use fa_win::clock::Clock;
use fa_win::mem::PageBuf;
use fa_win::thread::pin_current_thread;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VICTIM_BYTES: usize = 2 << 20;
const THRASH_BYTES: usize = 64 << 20;

/// Victim median latency (ns per load) and thrasher throughput (million lines per second) for one mode.
fn run(mode: Option<bool>, clock: &Clock) -> (f64, f64) {
    let stop = Arc::new(AtomicBool::new(false));
    let lines = Arc::new(AtomicU64::new(0));
    let t0 = Instant::now();
    let handle = mode.map(|flush| {
        let (stop, lines) = (stop.clone(), lines.clone());
        std::thread::spawn(move || {
            let _pin = pin_current_thread(4).expect("pin thrasher");
            let mut buf = PageBuf::alloc(THRASH_BYTES, false).expect("thrash buffer");
            let words = buf.as_mut_u64();
            while !stop.load(Ordering::Relaxed) {
                lines.fetch_add(sweep(words, flush), Ordering::Relaxed);
            }
        })
    });
    std::thread::sleep(Duration::from_millis(300));
    // The victim behaves like a game's per-frame working set: one lap to load it, a pause during which the thrasher
    // runs, then a timed lap. A ring walked without pause keeps its lines recent and survives the sweep.
    let mut victim = PageBuf::alloc(VICTIM_BYTES, false).expect("victim buffer");
    let slots = VICTIM_BYTES / LINE_BYTES;
    let next = sattolo(slots, 0xF4A3_5EED);
    let ring = ChaseRing::link(victim.as_mut_slice(), &next, LINE_BYTES / 8).expect("ring");
    let mut laps: Vec<f64> = (0..15)
        .map(|_| {
            ring.timed(slots as u64);
            // Spin, not sleep: a sleeping core idles and wakes slowly, which would drown the cache effect.
            let until = clock.now() + clock.hz().get() / 200;
            while clock.now() < until {
                std::hint::spin_loop();
            }
            let (done, ticks) = ring.timed(slots as u64);
            clock.ticks_to_ns(ticks) as f64 / done as f64
        })
        .collect();
    laps.sort_by(f64::total_cmp);
    let lat = laps[laps.len() / 2];
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = handle {
        h.join().unwrap();
    }
    let rate = lines.load(Ordering::Relaxed) as f64 / t0.elapsed().as_secs_f64() / 1e6;
    (lat, rate)
}

fn main() {
    let _pin = pin_current_thread(2).expect("pin victim");
    let clock = Clock::new();
    println!("run,mode,victim_ns_per_load,thrasher_mlines_per_s");
    for r in 1..=3 {
        for (name, mode) in [
            ("none", None),
            ("plain", Some(false)),
            ("clflush", Some(true)),
        ] {
            let (ns, rate) = run(mode, &clock);
            println!("{r},{name},{ns:.2},{rate:.1}");
        }
    }
}
