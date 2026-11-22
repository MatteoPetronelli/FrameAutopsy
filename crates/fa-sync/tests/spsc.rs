//! Two-thread stress tests for the SPSC ring (SYNC-01). They also run under Miri:
//! `cargo +nightly miri test -p fa-sync` (optionally `MIRIFLAGS=-Zmiri-many-seeds=0..8`).

use fa_sync::spsc::{ring, Full};
use std::thread;

/// 32 bytes, like the records the ETW callback will push. Every word derives from `seq`, so a torn or stale read is
/// detected whatever the interleaving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Record([u64; 4]);

impl Record {
    fn new(seq: u64) -> Self {
        Self([
            seq,
            !seq,
            seq.wrapping_mul(0x9E37_79B9_7F4A_7C15),
            seq ^ 0xA5A5_A5A5_A5A5_A5A5,
        ])
    }
    fn is_consistent(&self) -> bool {
        *self == Self::new(self.0[0])
    }
}

const fn count() -> u64 {
    if cfg!(miri) {
        300
    } else {
        1_000_000
    }
}

const fn capacity() -> usize {
    if cfg!(miri) {
        8
    } else {
        256
    }
}

#[test]
fn every_record_arrives_once_intact_and_in_order_when_the_producer_retries() {
    let n = count();
    let (mut p, mut c) = ring::<Record>(capacity());
    let producer = thread::spawn(move || {
        for seq in 0..n {
            let mut r = Record::new(seq);
            while let Err(Full(back)) = p.push(r) {
                r = back;
                thread::yield_now();
            }
        }
    });
    let mut expected = 0;
    while expected < n {
        match c.pop() {
            Some(r) => {
                assert!(r.is_consistent(), "torn record {r:?}");
                assert_eq!(r.0[0], expected, "out of order or duplicated");
                expected += 1;
            }
            None => thread::yield_now(),
        }
    }
    producer.join().unwrap();
    assert_eq!(c.pop(), None);
}

#[test]
fn a_producer_that_never_waits_loses_records_but_counts_them_and_keeps_order() {
    let n = count();
    let (mut p, mut c) = ring::<Record>(capacity());
    let producer = thread::spawn(move || {
        for seq in 0..n {
            let _ = p.push(Record::new(seq));
        }
        p
    });
    let mut received = 0u64;
    let mut last: Option<u64> = None;
    let p = loop {
        while let Some(r) = c.pop() {
            assert!(r.is_consistent(), "torn record {r:?}");
            assert!(
                last.is_none_or(|l| r.0[0] > l),
                "order broken: {last:?} then {}",
                r.0[0]
            );
            last = Some(r.0[0]);
            received += 1;
        }
        if producer.is_finished() {
            break producer.join().unwrap();
        }
        thread::yield_now();
    };
    while let Some(r) = c.pop() {
        assert!(r.is_consistent());
        received += 1;
    }
    assert_eq!(
        received + p.dropped(),
        n,
        "every record is either delivered or counted as dropped"
    );
    assert_eq!(p.dropped(), c.dropped());
}
