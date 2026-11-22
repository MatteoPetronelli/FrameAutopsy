//! The consumer-side pipeline: decode one event, push the record, count what happened. Allocation-free.

use crate::decode::{decode, Decoded};
use crate::record::{ColdRecord, HotRecord};
use crate::view::EventView;
use fa_sync::spsc::Producer;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A snapshot of the counters. Shown by `fa capture`; a rising `*_dropped` means the consumer is too slow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub events: u64,
    pub hot: u64,
    pub cold: u64,
    pub ignored: u64,
    pub unknown_version: u64,
    pub malformed: u64,
    pub hot_dropped: u64,
    pub cold_dropped: u64,
}

/// The live counters, readable from any thread while the callback updates them.
#[derive(Debug, Default)]
pub struct SharedStats {
    events: AtomicU64,
    hot: AtomicU64,
    cold: AtomicU64,
    ignored: AtomicU64,
    unknown_version: AtomicU64,
    malformed: AtomicU64,
    hot_dropped: AtomicU64,
    cold_dropped: AtomicU64,
}

/// One writer (the callback thread), so a plain load and store is enough: no locked instruction on the hot path.
fn bump(c: &AtomicU64) {
    c.store(c.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
}

impl SharedStats {
    pub fn snapshot(&self) -> Stats {
        let g = |c: &AtomicU64| c.load(Ordering::Relaxed);
        Stats {
            events: g(&self.events),
            hot: g(&self.hot),
            cold: g(&self.cold),
            ignored: g(&self.ignored),
            unknown_version: g(&self.unknown_version),
            malformed: g(&self.malformed),
            hot_dropped: g(&self.hot_dropped),
            cold_dropped: g(&self.cold_dropped),
        }
    }
}

pub struct Capture {
    hot: Producer<HotRecord>,
    cold: Producer<ColdRecord>,
    stats: Arc<SharedStats>,
}

impl Capture {
    pub fn new(hot: Producer<HotRecord>, cold: Producer<ColdRecord>) -> Self {
        Self {
            hot,
            cold,
            stats: Arc::new(SharedStats::default()),
        }
    }

    /// Called for every event, from the ETW callback.
    pub fn on_event(&mut self, view: &EventView<'_>) {
        let s = &*self.stats;
        bump(&s.events);
        match decode(view) {
            Decoded::Hot(r) => {
                bump(&s.hot);
                if self.hot.push(r).is_err() {
                    bump(&s.hot_dropped);
                }
            }
            Decoded::Cold(r) => {
                bump(&s.cold);
                if self.cold.push(r).is_err() {
                    bump(&s.cold_dropped);
                }
            }
            Decoded::Ignored => bump(&s.ignored),
            Decoded::UnknownVersion => bump(&s.unknown_version),
            Decoded::Malformed => bump(&s.malformed),
        }
    }

    /// A handle on the live counters, to read them while the capture runs.
    pub fn shared_stats(&self) -> Arc<SharedStats> {
        Arc::clone(&self.stats)
    }

    pub fn stats(&self) -> Stats {
        self.stats.snapshot()
    }
}
