//! The Lab render loop: pacing (vsync, fixed rate, uncapped) and the baseline jitter it records.
//!
//! Fixed rates do not trust the timer alone (p99 wake-up lateness is 1.3 to 3.3 ms on this machine, a large part of a
//! 144 FPS frame): a high-resolution timer sleeps until about 1.5 ms before the deadline, then the loop spins on the TSC.
//! The spinning thread burns a core, so a Lab burner must not use the render thread's CPU (design D-5).

use fa_win::clock::Clock;
use fa_win::console::ctrl_c_requested;
use fa_win::render::LabWindow;
use fa_win::timer::HiResTimer;
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

/// Runs the loop for `seconds`, or until Ctrl-C or the window is closed, and returns the measured baseline.
pub fn run(fps: Fps, seconds: u64) -> io::Result<LoopReport> {
    let clock = Clock::new();
    let hz = clock.hz().get();
    let window = LabWindow::new(960, 540)?;
    let timer = HiResTimer::new()?;
    let period = match fps {
        Fps::Fixed(f) => Some(hz / u64::from(f)),
        _ => None,
    };
    let margin = hz * 3 / 2000; // 1.5 ms of ticks
    let end = clock.now() + hz * seconds;
    let mut frame_ns = Vec::with_capacity(usize::try_from(seconds).unwrap_or(0) * 1000);
    let mut deadline = clock.now();
    let mut last: Option<u64> = None;
    let mut frame = 0u64;
    while window.pump() && !ctrl_c_requested() && clock.now() < end {
        window.draw(frame);
        if let Some(p) = period {
            deadline += p;
            let now = clock.now();
            if now > deadline + p {
                deadline = now; // fell more than a frame behind: resynchronise instead of catching up in a burst
            }
            wait_until(&clock, &timer, deadline, margin, hz)?;
        }
        window.present(fps == Fps::Vsync, fps == Fps::Uncapped)?;
        let now = clock.now();
        if let Some(prev) = last {
            frame_ns.push(u64::try_from(clock.ticks_to_ns(now - prev)).unwrap_or(u64::MAX));
        }
        last = Some(now);
        frame += 1;
    }
    let target_ns = period.map(|p| u64::try_from(clock.ticks_to_ns(p)).unwrap_or(0));
    Ok(LoopReport {
        adapter: window.adapter_name().to_string(),
        tsc_hz: hz,
        baseline: Baseline::from_frames(&frame_ns, target_ns),
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
