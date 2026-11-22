//! A consumer that never reads must not slow the callback down: drops grow, the producer keeps returning at once.

use fa_capture::capture::Capture;
use fa_capture::synthetic::Synthetic;
use fa_sync::spsc::ring;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn a_stalled_consumer_makes_drops_grow_without_blocking_the_producer() {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stream = Synthetic::new();
        let (hp, _hot_consumer_never_reads) = ring(256);
        let (cp, _cold_consumer_never_reads) = ring(16);
        let mut capture = Capture::new(hp, cp);
        let mut drops_at = Vec::new();
        for i in 0..400_000u64 {
            capture.on_event(&stream.event(i));
            if i % 100_000 == 99_999 {
                drops_at.push(capture.stats().hot_dropped);
            }
        }
        let _ = tx.send((drops_at, capture.stats()));
    });
    // If a push ever blocked, the thread would never answer.
    let (drops_at, stats) = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the producer must never block");
    assert!(
        drops_at.windows(2).all(|w| w[1] > w[0]),
        "drops keep growing while the consumer is stalled: {drops_at:?}"
    );
    assert_eq!(stats.events, 400_000);
    assert_eq!(
        stats.events,
        stats.hot + stats.cold + stats.ignored + stats.unknown_version + stats.malformed
    );
}
