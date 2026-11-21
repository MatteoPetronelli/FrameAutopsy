use criterion::{criterion_group, criterion_main, Criterion};
use fa_sync::counting::{count_allocs, CountingAlloc};
use std::hint::black_box;

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

// Reference = the same work without the counting wrapper; candidate = wrapped in `count_allocs`.
fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("counting_vs_system");
    g.bench_function("system", |b| {
        b.iter(|| black_box(Vec::<u64>::with_capacity(16)))
    });
    g.bench_function("counting", |b| {
        b.iter(|| black_box(count_allocs(|| Vec::<u64>::with_capacity(16))))
    });
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
