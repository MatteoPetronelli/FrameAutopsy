use fa_sync::counting::{assert_no_alloc, count_allocs, CountingAlloc};

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

#[test]
fn growing_a_vec_is_counted() {
    let (_, n) = count_allocs(|| {
        let mut v: Vec<u64> = Vec::new();
        for i in 0..4 {
            v.push(i);
        }
        v
    });
    assert!(n >= 1, "Vec::push past capacity must allocate, got {n}");
}

#[test]
fn pushing_into_a_presized_vec_counts_zero() {
    let mut v: Vec<u64> = Vec::with_capacity(8);
    assert_no_alloc(|| {
        for i in 0..8 {
            v.push(i);
        }
    });
}

#[test]
#[should_panic(expected = "expected zero allocations")]
fn assert_no_alloc_panics_on_allocation() {
    assert_no_alloc(|| Box::new(1u8));
}
