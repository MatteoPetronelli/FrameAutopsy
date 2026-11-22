//! Frame extraction, following PresentMon: the frametime of a swap chain is the interval between two consecutive
//! present starts of the same process and swap chain (`MsBetweenPresents`); the start is the event timestamp.
//!
//! Scope of lot 5: frametime only. Displayed time and present mode need the DxgKrnl flip, queue and vsync events and
//! are not computed yet (see the design notes). Runs on the worker: it may allocate.

use crate::record::HotRecord;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameSource {
    /// DXGI `Present_Start`.
    Dxgi,
    /// DxgKrnl `Present_Info`, used only by processes that never present through DXGI (OpenGL, Vulkan).
    DxgKrnl,
}

/// One frame: `frametime` is the interval since the previous present of the same swap chain, in raw ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub ts: u64,
    pub pid: u32,
    pub swap_chain: u64,
    pub frametime: u64,
    pub sync_interval: u8,
    pub source: FrameSource,
}

const MAX_KEYS: usize = 4096;

pub struct FrameExtractor {
    /// Last present start per (process, swap chain or window).
    last: HashMap<(u32, u64, bool), u64>,
    /// Last DXGI present per process: a process seen presenting through DXGI also emits DxgKrnl events for the same
    /// presents, which would count every frame twice.
    last_dxgi: HashMap<u32, u64>,
    /// Ticks: a DXGI present this recent hides the DxgKrnl events of the same process; also the idle expiry unit.
    window: u64,
}

impl FrameExtractor {
    /// `window_ticks`: about one second of ticks.
    pub fn new(window_ticks: u64) -> Self {
        Self {
            last: HashMap::new(),
            last_dxgi: HashMap::new(),
            window: window_ticks.max(1),
        }
    }

    /// Feeds one record; returns a frame when it completes an interval. Other records are ignored.
    pub fn push(&mut self, record: &HotRecord) -> Option<Frame> {
        let (ts, pid, id, dxgkrnl, sync_interval) = match *record {
            HotRecord::Present {
                ts,
                swap_chain,
                pid,
                sync_interval,
                ..
            } => {
                self.last_dxgi.insert(pid, ts);
                (ts, pid, swap_chain, false, sync_interval)
            }
            HotRecord::DxgPresent {
                ts,
                window,
                pid,
                flip_interval,
                ..
            } => {
                if self
                    .last_dxgi
                    .get(&pid)
                    .is_some_and(|d| ts.saturating_sub(*d) <= self.window)
                {
                    return None;
                }
                (ts, pid, window, true, flip_interval.min(255) as u8)
            }
            _ => return None,
        };
        if self.last.len() >= MAX_KEYS {
            let (window, now) = (self.window, ts);
            self.last
                .retain(|_, t| now.saturating_sub(*t) <= window * 10);
            self.last_dxgi
                .retain(|_, t| now.saturating_sub(*t) <= window * 10);
            if self.last.len() >= MAX_KEYS {
                self.last.clear();
            }
        }
        let prev = self.last.insert((pid, id, dxgkrnl), ts)?;
        // A timestamp that goes backwards (it cannot within one session) is dropped rather than made into a frame.
        let frametime = ts.checked_sub(prev)?;
        Some(Frame {
            ts,
            pid,
            swap_chain: id,
            frametime,
            sync_interval,
            source: if dxgkrnl {
                FrameSource::DxgKrnl
            } else {
                FrameSource::Dxgi
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: u64 = 1_000_000;

    fn dxgi(ts: u64, pid: u32, sc: u64) -> HotRecord {
        HotRecord::Present {
            ts,
            swap_chain: sc,
            pid,
            flags: 0,
            sync_interval: 1,
        }
    }
    fn krnl(ts: u64, pid: u32, window: u64) -> HotRecord {
        HotRecord::DxgPresent {
            ts,
            window,
            pid,
            flip_interval: 0,
            flags: 0,
        }
    }

    #[test]
    fn the_first_present_has_no_frametime_then_each_one_does() {
        let mut f = FrameExtractor::new(SEC);
        assert_eq!(f.push(&dxgi(1000, 1, 7)), None);
        let fr = f.push(&dxgi(17_667, 1, 7)).unwrap();
        assert_eq!(
            (fr.ts, fr.frametime, fr.pid, fr.swap_chain, fr.source),
            (17_667, 16_667, 1, 7, FrameSource::Dxgi)
        );
        assert_eq!(f.push(&dxgi(34_000, 1, 7)).unwrap().frametime, 16_333);
    }

    #[test]
    fn swap_chains_and_processes_are_independent() {
        let mut f = FrameExtractor::new(SEC);
        for (ts, pid, sc) in [(0, 1, 7), (10, 1, 8), (20, 2, 7)] {
            assert_eq!(
                f.push(&dxgi(ts, pid, sc)),
                None,
                "each key starts its own sequence"
            );
        }
        assert_eq!(f.push(&dxgi(100, 1, 7)).unwrap().frametime, 100);
        assert_eq!(f.push(&dxgi(130, 1, 8)).unwrap().frametime, 120);
        assert_eq!(f.push(&dxgi(500, 2, 7)).unwrap().frametime, 480);
    }

    #[test]
    fn a_dropped_frame_shows_as_one_long_frametime() {
        let mut f = FrameExtractor::new(SEC);
        f.push(&dxgi(0, 1, 7));
        f.push(&dxgi(16_000, 1, 7));
        let long = f.push(&dxgi(16_000 + 66_000, 1, 7)).unwrap();
        assert_eq!(long.frametime, 66_000);
    }

    #[test]
    fn a_dxgi_process_is_not_counted_twice_through_dxgkrnl() {
        let mut f = FrameExtractor::new(SEC);
        f.push(&dxgi(0, 1, 7));
        assert_eq!(
            f.push(&krnl(5, 1, 99)),
            None,
            "same present seen by the kernel"
        );
        let fr = f.push(&dxgi(16_000, 1, 7)).unwrap();
        assert_eq!(fr.frametime, 16_000);
        assert_eq!(f.push(&krnl(16_005, 1, 99)), None);
    }

    #[test]
    fn a_process_that_never_uses_dxgi_is_measured_through_dxgkrnl() {
        let mut f = FrameExtractor::new(SEC);
        assert_eq!(f.push(&krnl(0, 4, 0x55)), None);
        let fr = f.push(&krnl(8_000, 4, 0x55)).unwrap();
        assert_eq!((fr.frametime, fr.source), (8_000, FrameSource::DxgKrnl));
    }

    #[test]
    fn a_process_that_stopped_using_dxgi_falls_back_after_the_window() {
        let mut f = FrameExtractor::new(SEC);
        f.push(&dxgi(0, 1, 7));
        assert_eq!(
            f.push(&krnl(2 * SEC, 1, 3)),
            None,
            "first sample of that key"
        );
        assert!(f.push(&krnl(2 * SEC + 10, 1, 3)).is_some());
    }

    #[test]
    fn a_timestamp_going_backwards_makes_no_frame() {
        let mut f = FrameExtractor::new(SEC);
        f.push(&dxgi(1000, 1, 7));
        assert_eq!(f.push(&dxgi(900, 1, 7)), None);
    }

    #[test]
    fn other_records_are_ignored_and_the_table_stays_bounded() {
        let mut f = FrameExtractor::new(10);
        assert_eq!(
            f.push(&HotRecord::CSwitch {
                ts: 1,
                new_tid: 1,
                old_tid: 2,
                cpu: 0,
                new_priority: 0,
                old_priority: 0
            }),
            None
        );
        for sc in 0..(MAX_KEYS as u64 * 3) {
            f.push(&dxgi(sc * 1000, 1, sc));
        }
        assert!(f.last.len() <= MAX_KEYS);
    }
}
