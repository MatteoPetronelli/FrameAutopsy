//! Injectors driven from the render loop's hook. `frame-stall` is the first one (design D-7, D-9).

use super::journal::Journal;
use super::render_loop::Stage;
use super::scenario::{Cause, Injection};
use fa_win::clock::Clock;
use serde_json::{json, Value};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

enum State {
    Waiting,
    /// The stall is over; its end line is written after the next `Present`, so the write does not lengthen the frame.
    PendingEnd {
        start_tsc: u64,
        end_tsc: u64,
    },
    Done,
}

/// Burns `stall_ms` before the `Present` of the first frame drawn at or after `start` seconds.
pub struct FrameStall {
    start_s: f64,
    stall_ms: f64,
    params: Value,
    /// Tick of the last `Present` seen: what came before the stall in its frame is `now` minus this.
    last_present: u64,
    state: State,
}

impl FrameStall {
    /// `inj.cause` must be `FrameStall` and `stall_ms` set (the guards check both before a run).
    pub fn new(inj: &Injection) -> Self {
        let stall_ms = inj.stall_ms.unwrap_or(0.0);
        FrameStall {
            start_s: inj.start,
            stall_ms,
            params: json!({ "stall_ms": stall_ms }),
            last_present: 0,
            state: State::Waiting,
        }
    }

    pub fn hook<W: Write>(
        &mut self,
        journal: &mut Journal<W>,
        clock: &Clock,
        elapsed_ticks: u64,
        stage: Stage,
    ) -> io::Result<()> {
        let hz = clock.hz().get() as f64;
        match (&self.state, stage) {
            (State::Waiting, Stage::BeforePresent) if elapsed_ticks as f64 >= self.start_s * hz => {
                // The frame already holds its draw and per-frame work: the captured frame time is that plus the stall.
                let pre_ms = elapsed_ticks.saturating_sub(self.last_present) as f64 * 1000.0 / hz;
                self.params = json!({ "stall_ms": self.stall_ms, "pre_ms": pre_ms });
                let start_tsc = clock.now();
                let target = start_tsc + (self.stall_ms / 1000.0 * hz) as u64;
                // The start line is written inside the stall: its cost is part of the burn, not added to it.
                journal.start(Cause::FrameStall, start_tsc, &self.params)?;
                while clock.now() < target {
                    std::hint::spin_loop();
                }
                self.state = State::PendingEnd {
                    start_tsc,
                    end_tsc: clock.now(),
                };
            }
            (State::PendingEnd { start_tsc, end_tsc }, Stage::AfterPresent) => {
                journal.end(Cause::FrameStall, *start_tsc, *end_tsc, &self.params)?;
                self.state = State::Done;
            }
            _ => {}
        }
        if stage == Stage::AfterPresent {
            self.last_present = elapsed_ticks;
        }
        Ok(())
    }
}

pub type Work = Arc<dyn Fn(&AtomicBool) + Send + Sync>;
/// Given the CPU the render thread is on, the CPUs to use and the journal parameters.
pub type Pick = Box<dyn Fn(u32) -> (Vec<u32>, Value)>;

enum TimedState {
    Waiting,
    Running {
        start_tsc: u64,
        /// Own page-fault counter at the start: the end line reports the rise.
        faults: u64,
        stop: Arc<AtomicBool>,
        handles: Vec<JoinHandle<()>>,
    },
    Done,
}

/// A worker injection: one thread per CPU runs `work` between `start_s` and `end_s` (seconds into the scenario). Both
/// transitions happen after `Present`, so spawning, joining and the journal writes never lengthen a frame. Dropping a
/// running injection (Ctrl+C, error) stops and joins its threads.
pub struct Timed {
    cause: Cause,
    start_s: f64,
    end_s: f64,
    work: Work,
    pick: Pick,
    params: Value,
    state: TimedState,
}

impl Timed {
    pub fn new(cause: Cause, start_s: f64, end_s: f64, work: Work, pick: Pick) -> Self {
        Timed {
            cause,
            start_s,
            end_s,
            work,
            pick,
            params: Value::Null,
            state: TimedState::Waiting,
        }
    }

    pub fn hook<W: Write>(
        &mut self,
        journal: &mut Journal<W>,
        clock: &Clock,
        elapsed_ticks: u64,
        stage: Stage,
    ) -> io::Result<()> {
        if stage != Stage::AfterPresent {
            return Ok(());
        }
        let hz = clock.hz().get() as f64;
        let elapsed = elapsed_ticks as f64;
        match &mut self.state {
            TimedState::Waiting if elapsed >= self.start_s * hz => {
                // The hook runs on the render thread: this is the CPU the burners must stay away from.
                let (cpus, params) = (self.pick)(fa_win::thread::current_cpu());
                self.params = params;
                let start_tsc = clock.now();
                journal.start(self.cause, start_tsc, &self.params)?;
                let stop = Arc::new(AtomicBool::new(false));
                let handles = cpus
                    .iter()
                    .map(|&cpu| {
                        let (stop, work) = (stop.clone(), self.work.clone());
                        std::thread::spawn(move || {
                            let _pin = fa_win::thread::pin_current_thread(cpu);
                            work(&stop);
                        })
                    })
                    .collect();
                let faults = fa_win::labsys::own_page_faults().unwrap_or(0);
                self.state = TimedState::Running {
                    start_tsc,
                    faults,
                    stop,
                    handles,
                };
            }
            TimedState::Running { .. } if elapsed >= self.end_s * hz => {
                if let TimedState::Running {
                    start_tsc,
                    faults,
                    stop,
                    handles,
                } = std::mem::replace(&mut self.state, TimedState::Done)
                {
                    stop.store(true, Ordering::Relaxed);
                    handles.into_iter().for_each(|h| {
                        let _ = h.join();
                    });
                    // Page faults of the whole Lab process during the window: the ground truth of the storm.
                    let rise = fa_win::labsys::own_page_faults()
                        .unwrap_or(0)
                        .saturating_sub(faults);
                    let mut params = self.params.clone();
                    params["page_faults"] = rise.into();
                    journal.end(self.cause, start_tsc, clock.now(), &params)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        if let TimedState::Running { stop, handles, .. } =
            std::mem::replace(&mut self.state, TimedState::Done)
        {
            stop.store(true, Ordering::Relaxed);
            handles.into_iter().for_each(|h| {
                let _ = h.join();
            });
        }
    }
}
