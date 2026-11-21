use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

// Placeholder proving the harness links; lot 2 replaces it with `tsc_vs_intrinsics`.
fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("harness_vs_reference");
    g.bench_function("reference", |b| b.iter(|| black_box(1u64) + black_box(1)));
    g.bench_function("candidate", |b| b.iter(|| black_box(1u64) + black_box(1)));
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
