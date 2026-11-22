//! Single-producer single-consumer ring (SYNC-01).
//!
//! Power-of-two capacity, `T: Copy`, `MaybeUninit<T>` slots, head and tail on separate cache lines. `push` never
//! blocks: when the ring is full it hands the value back and counts a drop. The two ends are distinct owned values
//! (`Producer`, `Consumer`) whose methods take `&mut self`, so "one producer, one consumer" is enforced by the type
//! system and the public API is safe.
//!
//! Memory orderings (indices only ever grow, wrapping at `usize::MAX`; a slot is `index & mask`):
//! - the producer writes the slot, THEN stores `tail` with `Release`;
//! - the consumer loads `tail` with `Acquire`, THEN reads the slot: the read sees the completed write;
//! - the consumer reads the slot, THEN stores `head` with `Release`;
//! - the producer loads `head` with `Acquire` before reusing a slot: the consumer has finished reading it.
//!
//! An index is only ever *written* by its owner, which therefore reads it back with `Relaxed` (here: from its own
//! local copy). `drops` is a statistic and uses `Relaxed`.

use std::cell::UnsafeCell;
use std::fmt;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Aligns its content on a cache line (64 bytes on x86-64) so the two indices never share one.
#[repr(align(64))]
struct Padded<T>(T);

struct Inner<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    mask: usize,
    /// Next slot to read. Written only by the consumer.
    head: Padded<AtomicUsize>,
    /// Next slot to write. Written only by the producer.
    tail: Padded<AtomicUsize>,
    /// Values refused because the ring was full. Written only by the producer.
    drops: Padded<AtomicU64>,
}

// SAFETY: a slot is written only by the unique `Producer` while it lies outside [head, tail), and read only by the
// unique `Consumer` while it lies inside it; the Release/Acquire pairs on `tail` and `head` order those accesses, so
// no slot is ever accessed concurrently. `T: Send` lets the values cross threads.
unsafe impl<T: Send> Sync for Inner<T> {}

/// The ring was full; the value is handed back.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Full<T>(pub T);

impl<T> fmt::Debug for Full<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Full(..)")
    }
}

impl<T> fmt::Display for Full<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ring is full")
    }
}

/// Producer end: pushes, never blocks.
pub struct Producer<T: Copy + Send> {
    inner: Arc<Inner<T>>,
    /// Our own `tail` (we are its only writer).
    tail: usize,
    /// Last `head` we observed; refreshed with Acquire only when the ring looks full.
    head_cache: usize,
}

/// Consumer end: pops, never blocks.
pub struct Consumer<T: Copy + Send> {
    inner: Arc<Inner<T>>,
    /// Our own `head` (we are its only writer).
    head: usize,
    /// Last `tail` we observed; refreshed with Acquire only when the ring looks empty.
    tail_cache: usize,
}

/// Creates a ring holding at least `capacity` values (rounded up to a power of two, at least 1). The only allocation.
pub fn ring<T: Copy + Send>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    assert!(capacity <= 1 << 30, "ring capacity out of range");
    let capacity = capacity.max(1).next_power_of_two();
    let slots: Box<[UnsafeCell<MaybeUninit<T>>]> = (0..capacity)
        .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
        .collect();
    let inner = Arc::new(Inner {
        slots,
        mask: capacity - 1,
        head: Padded(AtomicUsize::new(0)),
        tail: Padded(AtomicUsize::new(0)),
        drops: Padded(AtomicU64::new(0)),
    });
    (
        Producer {
            inner: Arc::clone(&inner),
            tail: 0,
            head_cache: 0,
        },
        Consumer {
            inner,
            head: 0,
            tail_cache: 0,
        },
    )
}

impl<T: Copy + Send> Producer<T> {
    /// Appends `value`, or returns it inside `Full` (and counts a drop) when the ring is full.
    pub fn push(&mut self, value: T) -> Result<(), Full<T>> {
        let capacity = self.inner.mask + 1;
        if self.tail.wrapping_sub(self.head_cache) == capacity {
            self.head_cache = self.inner.head.0.load(Ordering::Acquire);
            if self.tail.wrapping_sub(self.head_cache) == capacity {
                self.inner.drops.0.fetch_add(1, Ordering::Relaxed);
                return Err(Full(value));
            }
        }
        let slot = self.inner.slots[self.tail & self.inner.mask]
            .get()
            .cast::<T>();
        // SAFETY: tail - head < capacity (checked above, head read with Acquire), so this slot is outside
        // [head, tail): the consumer is not reading it and has finished with its previous content. `&mut self` makes
        // us the only producer. `slot` points at an element of the slice, so it is valid and aligned.
        unsafe { slot.write(value) };
        self.tail = self.tail.wrapping_add(1);
        // Release: the slot write above is visible to whoever Acquire-loads this tail.
        self.inner.tail.0.store(self.tail, Ordering::Release);
        Ok(())
    }

    /// Number of values refused so far because the ring was full.
    pub fn dropped(&self) -> u64 {
        self.inner.drops.0.load(Ordering::Relaxed)
    }

    pub fn capacity(&self) -> usize {
        self.inner.mask + 1
    }
}

impl<T: Copy + Send> Consumer<T> {
    /// Removes and returns the oldest value, or `None` when the ring is empty.
    pub fn pop(&mut self) -> Option<T> {
        if self.head == self.tail_cache {
            self.tail_cache = self.inner.tail.0.load(Ordering::Acquire);
            if self.head == self.tail_cache {
                return None;
            }
        }
        let slot = self.inner.slots[self.head & self.inner.mask]
            .get()
            .cast::<T>();
        // SAFETY: head != tail_cache, and tail_cache came from an Acquire load that pairs with the producer's Release
        // store done after it wrote every slot below tail_cache: this slot is initialised and the producer will not
        // touch it until we publish a larger head. `&mut self` makes us the only consumer. `T: Copy`, so the bitwise
        // read leaves nothing to drop.
        let value = unsafe { slot.read() };
        self.head = self.head.wrapping_add(1);
        // Release: our read above is finished before the producer may reuse the slot.
        self.inner.head.0.store(self.head, Ordering::Release);
        Some(value)
    }

    /// Number of values refused so far because the ring was full.
    pub fn dropped(&self) -> u64 {
        self.inner.drops.0.load(Ordering::Relaxed)
    }

    pub fn capacity(&self) -> usize {
        self.inner.mask + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_is_rounded_up_to_a_power_of_two() {
        assert_eq!(ring::<u32>(1).0.capacity(), 1);
        assert_eq!(ring::<u32>(0).0.capacity(), 1);
        assert_eq!(ring::<u32>(5).0.capacity(), 8);
        assert_eq!(ring::<u32>(8).1.capacity(), 8);
    }

    #[test]
    fn values_come_out_in_the_order_they_went_in() {
        let (mut p, mut c) = ring::<u32>(8);
        assert_eq!(c.pop(), None);
        for i in 0..5 {
            p.push(i).unwrap();
        }
        for i in 0..5 {
            assert_eq!(c.pop(), Some(i));
        }
        assert_eq!(c.pop(), None);
    }

    #[test]
    fn a_full_ring_hands_the_value_back_and_counts_a_drop() {
        let (mut p, mut c) = ring::<u32>(4);
        for i in 0..4 {
            p.push(i).unwrap();
        }
        assert_eq!(p.push(99), Err(Full(99)));
        assert_eq!(p.push(100), Err(Full(100)));
        assert_eq!(p.dropped(), 2);
        assert_eq!(c.dropped(), 2);
        // The refused values were not stored.
        for i in 0..4 {
            assert_eq!(c.pop(), Some(i));
        }
        assert_eq!(c.pop(), None);
        // Space is available again.
        p.push(7).unwrap();
        assert_eq!(c.pop(), Some(7));
    }

    #[test]
    fn indices_wrap_around_the_buffer_many_times() {
        let (mut p, mut c) = ring::<u64>(4);
        for i in 0..1000u64 {
            p.push(i).unwrap();
            if i % 3 == 0 {
                p.push(i + 10_000).unwrap();
                assert_eq!(c.pop(), Some(i));
                assert_eq!(c.pop(), Some(i + 10_000));
            } else {
                assert_eq!(c.pop(), Some(i));
            }
        }
        assert_eq!(c.pop(), None);
        assert_eq!(p.dropped(), 0);
    }

    #[test]
    fn indices_survive_usize_wrap_around() {
        let (mut p, mut c) = ring::<u32>(4);
        // Start near the end of the index space, as a very long run eventually would.
        p.tail = usize::MAX - 2;
        p.head_cache = usize::MAX - 2;
        c.head = usize::MAX - 2;
        c.tail_cache = usize::MAX - 2;
        p.inner.tail.0.store(usize::MAX - 2, Ordering::Relaxed);
        p.inner.head.0.store(usize::MAX - 2, Ordering::Relaxed);
        for i in 0..10u32 {
            p.push(i).unwrap();
            assert_eq!(c.pop(), Some(i));
        }
        for i in 0..4u32 {
            p.push(i).unwrap();
        }
        assert_eq!(p.push(9), Err(Full(9)));
        for i in 0..4u32 {
            assert_eq!(c.pop(), Some(i));
        }
    }
}
