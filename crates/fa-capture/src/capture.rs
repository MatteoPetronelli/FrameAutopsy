//! The consumer-side pipeline: decode one event, push the record, count what happened. Allocation-free.

use crate::decode::{decode, Decoded};
use crate::record::{ColdRecord, HotRecord};
use crate::view::EventView;
use fa_sync::spsc::Producer;

/// Everything the capture counts. Shown by `fa capture`; a rising `*_dropped` means the consumer is too slow.
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

pub struct Capture {
    hot: Producer<HotRecord>,
    cold: Producer<ColdRecord>,
    stats: Stats,
}

impl Capture {
    pub fn new(hot: Producer<HotRecord>, cold: Producer<ColdRecord>) -> Self {
        Self {
            hot,
            cold,
            stats: Stats::default(),
        }
    }

    /// Called for every event, from the ETW callback.
    pub fn on_event(&mut self, view: &EventView<'_>) {
        self.stats.events += 1;
        match decode(view) {
            Decoded::Hot(r) => {
                self.stats.hot += 1;
                if self.hot.push(r).is_err() {
                    self.stats.hot_dropped += 1;
                }
            }
            Decoded::Cold(r) => {
                self.stats.cold += 1;
                if self.cold.push(r).is_err() {
                    self.stats.cold_dropped += 1;
                }
            }
            Decoded::Ignored => self.stats.ignored += 1,
            Decoded::UnknownVersion => self.stats.unknown_version += 1,
            Decoded::Malformed => self.stats.malformed += 1,
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }
}
