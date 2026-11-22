//! SYNC-01 bench: our ring against `rtrb`, 32-byte records, producer and consumer pinned on two different physical
//! cores (logical CPUs 2 and 4 on the target i5-11400H, where SMT siblings are adjacent: 2-3 and 4-5).
//! Throughput = records per second over a one-way transfer. Latency = half a round trip between two rings.
//! Run from the repository root, AC power, no game, 3 runs, median and spread (docs/asm-unsafe-catalog.md section 2).

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use fa_win::thread::pin_current_thread;
use std::hint::{black_box, spin_loop};
use std::thread;
use std::time::{Duration, Instant};

type Record = [u64; 4];
const CAP: usize = 1024;
const PRODUCER_CPU: u32 = 2;
const CONSUMER_CPU: u32 = 4;
const TRANSFER: u64 = 2_000_000;
const ROUND_TRIPS: u64 = 200_000;

trait Tx: Send + 'static {
    fn try_push(&mut self, r: Record) -> bool;
}
trait Rx: Send + 'static {
    fn try_pop(&mut self) -> Option<Record>;
}

impl Tx for fa_sync::spsc::Producer<Record> {
    fn try_push(&mut self, r: Record) -> bool {
        self.push(r).is_ok()
    }
}
impl Rx for fa_sync::spsc::Consumer<Record> {
    fn try_pop(&mut self) -> Option<Record> {
        self.pop()
    }
}
impl Tx for rtrb::Producer<Record> {
    fn try_push(&mut self, r: Record) -> bool {
        self.push(r).is_ok()
    }
}
impl Rx for rtrb::Consumer<Record> {
    fn try_pop(&mut self) -> Option<Record> {
        self.pop().ok()
    }
}

fn ours() -> (impl Tx, impl Rx) {
    fa_sync::spsc::ring::<Record>(CAP)
}
fn theirs() -> (impl Tx, impl Rx) {
    rtrb::RingBuffer::<Record>::new(CAP)
}

/// One-way transfer of `TRANSFER` records; returns the elapsed time.
fn throughput(mut tx: impl Tx, mut rx: impl Rx) -> Duration {
    let producer = thread::spawn(move || {
        let _pin = pin_current_thread(PRODUCER_CPU);
        for seq in 0..TRANSFER {
            while !tx.try_push([seq, 0, 0, 0]) {
                spin_loop();
            }
        }
    });
    let _pin = pin_current_thread(CONSUMER_CPU);
    let start = Instant::now();
    let mut got = 0;
    while got < TRANSFER {
        match rx.try_pop() {
            Some(r) => {
                black_box(r);
                got += 1;
            }
            None => spin_loop(),
        }
    }
    let elapsed = start.elapsed();
    producer.join().unwrap();
    elapsed
}

/// `ROUND_TRIPS` ping-pongs through two rings; returns the elapsed time (one way = half of it per trip).
fn round_trips(
    (mut ping_tx, mut ping_rx): (impl Tx, impl Rx),
    (mut pong_tx, mut pong_rx): (impl Tx, impl Rx),
) -> Duration {
    let echo = thread::spawn(move || {
        let _pin = pin_current_thread(PRODUCER_CPU);
        for _ in 0..ROUND_TRIPS {
            let r = loop {
                if let Some(r) = ping_rx.try_pop() {
                    break r;
                }
                spin_loop();
            };
            while !pong_tx.try_push(r) {
                spin_loop();
            }
        }
    });
    let _pin = pin_current_thread(CONSUMER_CPU);
    let start = Instant::now();
    for seq in 0..ROUND_TRIPS {
        while !ping_tx.try_push([seq, 0, 0, 0]) {
            spin_loop();
        }
        loop {
            if pong_rx.try_pop().is_some() {
                break;
            }
            spin_loop();
        }
    }
    let elapsed = start.elapsed();
    echo.join().unwrap();
    elapsed
}

fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("spsc_throughput");
    g.sample_size(10).throughput(Throughput::Elements(TRANSFER));
    g.bench_function("fa_sync", |b| {
        b.iter_custom(|iters| {
            (0..iters)
                .map(|_| {
                    let (t, r) = ours();
                    throughput(t, r)
                })
                .sum()
        })
    });
    g.bench_function("rtrb", |b| {
        b.iter_custom(|iters| {
            (0..iters)
                .map(|_| {
                    let (t, r) = theirs();
                    throughput(t, r)
                })
                .sum()
        })
    });
    g.finish();

    // Reported time per iteration = ROUND_TRIPS round trips; one-way latency = time / ROUND_TRIPS / 2.
    let mut g = c.benchmark_group("spsc_round_trips_200k");
    g.sample_size(10);
    g.bench_function("fa_sync", |b| {
        b.iter_custom(|iters| (0..iters).map(|_| round_trips(ours(), ours())).sum())
    });
    g.bench_function("rtrb", |b| {
        b.iter_custom(|iters| (0..iters).map(|_| round_trips(theirs(), theirs())).sum())
    });
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
