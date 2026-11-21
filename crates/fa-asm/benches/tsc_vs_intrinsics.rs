//! ASM-01 bench: `asm!` candidate against the intrinsics reference (catalog `docs/asm-unsafe-catalog.md`, section 2).
//! Run pinned on one core, on AC power, 3 times, with `FA_FORCE_SCALAR=1` once; see the ASM-01 row for the protocol.
//! `dispatch` is the public `tsc::now()`, which adds the selection branch on top of the chosen implementation.

use criterion::{criterion_group, criterion_main, Criterion};
use fa_asm::tsc;
use std::hint::black_box;

/// A bench of a wrong kernel is void: both implementations must read the same counter before anything is timed.
fn parity_first() {
    let min_gap = (0..2_000)
        .map(|_| {
            let a = tsc::asm::now();
            let b = tsc::reference::now();
            a.abs_diff(b)
        })
        .min()
        .expect("non-empty");
    assert!(
        min_gap < 5_000,
        "asm and reference disagree (min gap {min_gap} ticks): bench void"
    );
}

fn bench(c: &mut Criterion) {
    parity_first();
    let mut g = c.benchmark_group("tsc_now");
    g.bench_function("reference", |b| b.iter(|| black_box(tsc::reference::now())));
    g.bench_function("asm", |b| b.iter(|| black_box(tsc::asm::now())));
    g.bench_function("dispatch", |b| b.iter(|| black_box(tsc::now())));
    g.finish();

    let mut g = c.benchmark_group("tsc_now_serialized");
    g.bench_function("reference", |b| {
        b.iter(|| black_box(tsc::reference::now_serialized()))
    });
    g.bench_function("asm", |b| b.iter(|| black_box(tsc::asm::now_serialized())));
    g.bench_function("dispatch", |b| b.iter(|| black_box(tsc::now_serialized())));
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
