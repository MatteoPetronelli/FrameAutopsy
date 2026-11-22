//! The hot path (decode + push) allocates nothing, including when the rings are full and records are dropped.

use fa_capture::capture::Capture;
use fa_capture::synthetic::Synthetic;
use fa_sync::counting::{assert_no_alloc, count_allocs, CountingAlloc};
use fa_sync::spsc::ring;

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

const EVENTS: u64 = 1_000_000;

#[test]
fn one_million_events_allocate_nothing_even_when_the_rings_are_full() {
    let stream = Synthetic::new();
    // Small rings and no consumer for the first half of the run: the full-ring drop path is exercised.
    let (hp, mut hc) = ring(1024);
    let (cp, mut cc) = ring(64);
    let mut capture = Capture::new(hp, cp);
    assert_no_alloc(|| {
        for i in 0..EVENTS {
            capture.on_event(&stream.event(i));
            if i > EVENTS / 2 {
                // Second half: a consumer keeps up, so the same rings also take the non-full path.
                while hc.pop().is_some() {}
                while cc.pop().is_some() {}
            }
        }
    });
    let s = capture.stats();
    assert_eq!(s.events, EVENTS);
    assert_eq!(
        s.events,
        s.hot + s.cold + s.ignored + s.unknown_version + s.malformed,
        "every event is accounted for"
    );
    assert!(
        s.hot_dropped > 0 && s.cold_dropped > 0,
        "the full-ring path ran: {s:?}"
    );
    assert!(s.unknown_version > 0 && s.malformed > 0 && s.ignored > 0);
}

#[test]
fn the_counting_allocator_is_installed_in_this_binary() {
    // Guard: without it every count would be 0 and the test above would prove nothing.
    let (_, n) = count_allocs(|| Vec::<u64>::with_capacity(4));
    assert!(n >= 1);
}
