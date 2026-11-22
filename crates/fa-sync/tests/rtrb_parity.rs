//! Parity of `fa_sync::spsc` with the reference `rtrb` (SYNC-01): the same sequence of operations gives the same results.
//! Skipped under Miri: `rtrb` is only a reference here and the stress tests already cover our ring.
#![cfg(not(miri))]

use fa_sync::spsc::ring;

#[test]
fn same_operations_give_the_same_results_as_rtrb() {
    const CAP: usize = 8;
    let (mut p, mut c) = ring::<[u64; 4]>(CAP);
    let (mut rp, mut rc) = rtrb::RingBuffer::<[u64; 4]>::new(CAP);
    // xorshift: deterministic, no dependency.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for seq in 0..200_000u64 {
        let record = [seq, !seq, seq << 1, seq >> 1];
        if next() % 5 < 3 {
            let ours = p.push(record).is_ok();
            let theirs = rp.push(record).is_ok();
            assert_eq!(ours, theirs, "push accepted differently at step {seq}");
        } else {
            let ours = c.pop();
            let theirs = rc.pop().ok();
            assert_eq!(ours, theirs, "pop differs at step {seq}");
        }
    }
}
