//! Injectors driven from the render loop's hook. `frame-stall` is the first one (design D-7, D-9).

use super::journal::Journal;
use super::render_loop::Stage;
use super::scenario::{Cause, Injection};
use fa_win::clock::Clock;
use serde_json::{json, Value};
use std::io::{self, Write};

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
        Ok(())
    }
}
