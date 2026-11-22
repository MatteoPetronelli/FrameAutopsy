//! Lot 5, task 4.2: checks the real session on this machine. Run from an ELEVATED terminal:
//! `cargo run -p fa-win --example etw_check`.
//!
//! 1. starts a session, collects events for 3 s, stops it (drop);
//! 2. starts again straight away (a clean restart works);
//! 3. simulates a crash (the session is leaked, still running) and starts a new one (the stale session is stopped);
//! 4. prints how far the event timestamps are from a TSC read taken in the callback.
//!
//! Safe code only: all the unsafe lives in `fa_win::etw`.

use fa_win::clock::Clock;
use fa_win::etw::Session;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn run_for(seconds: u64, label: &str, clock: &Clock) -> std::io::Result<()> {
    let deltas: Arc<Mutex<Vec<i64>>> = Arc::default();
    let sink = Arc::clone(&deltas);
    let session = Session::start(move |v| {
        // Ticks between the event being stamped by the kernel and this callback reading the TSC.
        let now = fa_asm::tsc::now() as i64;
        if let Ok(mut d) = sink.lock() {
            if d.len() < 200_000 {
                d.push(now - v.timestamp as i64);
            }
        }
    })?;
    println!(
        "[{label}] session started, timebase {:?}",
        session.timebase()
    );
    std::thread::sleep(Duration::from_secs(seconds));
    println!(
        "[{label}] events {}, handler panics {}",
        session.events(),
        session.handler_panics()
    );
    drop(session);
    let mut d = deltas.lock().map(|d| d.clone()).unwrap_or_default();
    d.sort_unstable();
    if let (Some(min), Some(max)) = (d.first(), d.last()) {
        let ms = |ticks: i64| {
            clock.ticks_to_ns(ticks.unsigned_abs()) as f64 / 1e6
                * if ticks < 0 { -1.0 } else { 1.0 }
        };
        println!(
            "[{label}] callback delay min {:.3} ms, median {:.3} ms, p99 {:.3} ms, max {:.3} ms ({} samples)",
            ms(*min),
            ms(d[d.len() / 2]),
            ms(d[d.len() * 99 / 100]),
            ms(*max),
            d.len()
        );
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let clock = Clock::new();
    println!("clock timebase {:?}, {} Hz", clock.timebase(), clock.hz());
    run_for(3, "first", &clock)?;
    run_for(1, "restart", &clock)?;
    // Simulated crash: the session is leaked and keeps running in the kernel.
    let leaked = Session::start(|_| {})?;
    std::mem::forget(leaked);
    println!("[crash] a session was left running");
    run_for(1, "after-crash", &clock)?;
    println!("OK: restart and stale-session recovery work");
    Ok(())
}
