//! The Lab render loop: pacing (vsync, fixed rate, uncapped) and the baseline jitter it records.
//!
//! Fixed rates do not trust the timer alone (p99 wake-up lateness is 1.3 to 3.3 ms on this machine, a large part of a
//! 144 FPS frame): a high-resolution timer sleeps until about 1.5 ms before the deadline, then the loop spins on the TSC.
//! The spinning thread burns a core, so a Lab burner must not use the render thread's CPU (design D-5).

use super::frame_work::{compute, Work};
use fa_win::clock::Clock;
use fa_win::console::ctrl_c_requested;
use fa_win::render::LabWindow;
use fa_win::timer::HiResTimer;
use std::hint::black_box;
use std::io;
use std::time::Duration;

/// How the loop is paced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fps {
    /// One vertical blank per frame: the display's refresh rate.
    Vsync,
    /// A fixed rate in frames per second, paced on TSC deadlines.
    Fixed(u32),
    /// No wait at all.
    Uncapped,
}

impl Fps {
    pub fn parse(s: &str) -> Option<Fps> {
        match s {
            "vsync" => Some(Fps::Vsync),
            "uncapped" => Some(Fps::Uncapped),
            n => n
                .parse()
                .ok()
                .filter(|hz| (10..=1000).contains(hz))
                .map(Fps::Fixed),
        }
    }

    pub fn label(self) -> String {
        match self {
            Fps::Vsync => "vsync".into(),
            Fps::Fixed(hz) => hz.to_string(),
            Fps::Uncapped => "uncapped".into(),
        }
    }
}

/// Frame-time statistics of one run, in nanoseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Baseline {
    pub frames: usize,
    pub p50_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
    /// 99th percentile of the distance between a frame time and its target (the target period for a fixed rate,
    /// the median otherwise): the smallest stall the loop can tell from its own noise.
    pub jitter_p99_ns: u64,
}

fn percentile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

impl Baseline {
    /// `frame_ns` are the durations between consecutive presents; `target_ns` is the fixed period when there is one.
    pub fn from_frames(frame_ns: &[u64], target_ns: Option<u64>) -> Baseline {
        let mut sorted = frame_ns.to_vec();
        sorted.sort_unstable();
        let p50 = percentile(&sorted, 0.5);
        let target = target_ns.unwrap_or(p50);
        let mut off: Vec<u64> = frame_ns.iter().map(|&d| d.abs_diff(target)).collect();
        off.sort_unstable();
        Baseline {
            frames: frame_ns.len(),
            p50_ns: p50,
            p99_ns: percentile(&sorted, 0.99),
            max_ns: sorted.last().copied().unwrap_or(0),
            jitter_p99_ns: percentile(&off, 0.99),
        }
    }
}

/// Everything a run produces.
pub struct LoopReport {
    pub adapter: String,
    pub tsc_hz: u64,
    pub baseline: Baseline,
}

/// When in a frame an injection hook is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The frame is drawn; pacing and `Present` come next. A stall burnt here lengthens this frame.
    BeforePresent,
    /// `Present` returned: the place for slow work (journal writes) that must not lengthen the frame.
    AfterPresent,
}

/// The window, the timer and the pacing state, so that several phases (baseline, scenario) share one window.
pub struct Runner {
    pub clock: Clock,
    window: LabWindow,
    timer: HiResTimer,
    fps: Fps,
    frame: u64,
    /// Whether every frame also does the work of `frame_work` (scenarios do, the bare loop does not).
    work: bool,
}

/// What one phase of the loop produced.
pub struct Phase {
    pub baseline: Baseline,
    /// `(present tick, frame time in ms)` of every frame after the first.
    pub frames: Vec<(u64, f64)>,
}

impl Runner {
    pub fn open(fps: Fps, work: bool) -> io::Result<Self> {
        Ok(Runner {
            work,
            clock: Clock::new(),
            window: LabWindow::new(960, 540)?,
            timer: HiResTimer::new()?,
            fps,
            frame: 0,
        })
    }

    pub fn adapter(&self) -> &str {
        self.window.adapter_name()
    }

    /// Runs for `seconds`, or until Ctrl-C or the window is closed, and returns the frame-time statistics. `hook` gets
    /// the ticks elapsed since this call started, at both stages of every frame.
    pub fn run_for(
        &mut self,
        seconds: f64,
        hook: &mut dyn FnMut(&Clock, u64, Stage) -> io::Result<()>,
    ) -> io::Result<Phase> {
        let clock = &self.clock;
        let hz = clock.hz().get();
        let period = match self.fps {
            Fps::Fixed(f) => Some(hz / u64::from(f)),
            _ => None,
        };
        let margin = hz * 3 / 2000; // 1.5 ms of ticks
        let began = clock.now();
        let end = began + (hz as f64 * seconds) as u64;
        let mut frame_ns = Vec::with_capacity((seconds * 1000.0) as usize);
        let mut frames = Vec::with_capacity(frame_ns.capacity());
        let mut work = if self.work {
            Some(Work::new(clock)?)
        } else {
            None
        };
        let (slots, iters) = work.as_ref().map_or((0, 0), |w| (w.slots, w.iters));
        let ring = work.as_mut().map(Work::ring).transpose()?;
        if let Some(r) = &ring {
            r.timed(slots); // one lap loads the ring before the first frame
        }
        let mut deadline = began;
        let mut last: Option<u64> = None;
        while self.window.pump() && !ctrl_c_requested() && clock.now() < end {
            self.window.draw(self.frame);
            if let Some(r) = &ring {
                black_box(r.chase(slots));
                black_box(compute(iters));
            }
            hook(clock, clock.now() - began, Stage::BeforePresent)?;
            if let Some(p) = period {
                deadline += p;
                let now = clock.now();
                if now > deadline + p {
                    deadline = now; // fell more than a frame behind: resynchronise instead of catching up in a burst
                }
                wait_until(clock, &self.timer, deadline, margin, hz)?;
            }
            self.window
                .present(self.fps == Fps::Vsync, self.fps == Fps::Uncapped)?;
            let now = clock.now();
            if let Some(prev) = last {
                let ns = u64::try_from(clock.ticks_to_ns(now - prev)).unwrap_or(u64::MAX);
                frame_ns.push(ns);
                frames.push((now, ns as f64 / 1e6));
            }
            last = Some(now);
            self.frame += 1;
            hook(clock, now - began, Stage::AfterPresent)?;
        }
        let target_ns = period.map(|p| u64::try_from(clock.ticks_to_ns(p)).unwrap_or(0));
        Ok(Phase {
            baseline: Baseline::from_frames(&frame_ns, target_ns),
            frames,
        })
    }
}

/// Runs the loop alone for `seconds` and returns the measured baseline.
pub fn run(fps: Fps, seconds: u64) -> io::Result<LoopReport> {
    let mut runner = Runner::open(fps, false)?;
    let baseline = runner
        .run_for(seconds as f64, &mut |_, _, _| Ok(()))?
        .baseline;
    Ok(LoopReport {
        adapter: runner.adapter().to_string(),
        tsc_hz: runner.clock.hz().get(),
        baseline,
    })
}

/// Coarse timer sleep, then a TSC spin for the last `margin` ticks.
fn wait_until(
    clock: &Clock,
    timer: &HiResTimer,
    deadline: u64,
    margin: u64,
    hz: u64,
) -> io::Result<()> {
    let now = clock.now();
    if deadline > now + margin {
        let sleep_ticks = deadline - now - margin;
        timer.wait(Duration::from_nanos(
            sleep_ticks.saturating_mul(1_000_000_000) / hz,
        ))?;
    }
    while clock.now() < deadline {
        std::hint::spin_loop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_parses_modes_and_rates() {
        assert_eq!(Fps::parse("vsync"), Some(Fps::Vsync));
        assert_eq!(Fps::parse("uncapped"), Some(Fps::Uncapped));
        assert_eq!(Fps::parse("144"), Some(Fps::Fixed(144)));
        assert_eq!(Fps::parse("0"), None);
        assert_eq!(Fps::parse("fast"), None);
    }

    #[test]
    fn baseline_measures_distance_to_the_target_period() {
        // 99 perfect frames and one 10 ms outlier: p50 on target, max is the outlier, p99 jitter still small.
        let mut frames = vec![6_944_444u64; 99];
        frames.push(16_944_444);
        let b = Baseline::from_frames(&frames, Some(6_944_444));
        assert_eq!(b.frames, 100);
        assert_eq!(b.p50_ns, 6_944_444);
        assert_eq!(b.max_ns, 16_944_444);
        assert_eq!(b.jitter_p99_ns, 0);
        // Without a target the median is the reference.
        let b = Baseline::from_frames(&[10, 10, 10, 40], None);
        assert_eq!(b.jitter_p99_ns, 30);
    }

    #[test]
    fn an_empty_run_gives_zeros_not_a_panic() {
        assert_eq!(Baseline::from_frames(&[], Some(5)).max_ns, 0);
    }
}
