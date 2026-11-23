//! The Stutter Lab (lot 6): own render loop and known-cause injectors. The scorer, `lab/score.rs`, is lot 10.

mod journal;
mod render_loop;

use render_loop::Fps;

/// Parsed `fa lab loop` arguments.
pub struct LoopArgs {
    fps: Fps,
    seconds: u64,
}

pub fn parse(args: &[String]) -> Option<LoopArgs> {
    let [first, rest @ ..] = args else {
        return None;
    };
    if first != "loop" {
        return None;
    }
    let (mut fps, mut seconds) = (Fps::Fixed(60), 10);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match (flag.as_str(), it.next()) {
            ("--fps", Some(v)) => fps = Fps::parse(v)?,
            ("--seconds", Some(v)) => {
                seconds = v.parse().ok().filter(|s| (1..=3600).contains(s))?
            }
            _ => return None,
        }
    }
    Some(LoopArgs { fps, seconds })
}

pub fn run(a: LoopArgs) -> std::io::Result<()> {
    fa_win::console::install_ctrl_c()?;
    let r = render_loop::run(a.fps, a.seconds)?;
    let h = journal::Header::new("loop", a.fps, r.tsc_hz, &r.adapter, &r.baseline);
    println!("{}", h.line());
    Ok(())
}
