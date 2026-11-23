//! Bodies of the CPU and memory injectors (design D-5, D-6, D-7) and the choice of the CPUs they run on.
//! Every worker runs until `stop` is set and touches only memory it allocated itself.

use super::inject::{Timed, Work};
use super::scenario::{Cause, Injection};
use fa_asm::bw::{best_intrinsics, fill};
use fa_asm::fma;
use fa_probe::plan::load_cpus;
use fa_win::labsys::{fault_cycle, trim_working_set};
use fa_win::mem::PageBuf;
use fa_win::topology::LogicalCpu;
use serde_json::json;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Default buffer sizes in MiB (per thread), well above the 12 MiB last-level cache where that matters.
const CACHE_MIB: u64 = 64;
const TLB_MIB: u64 = 256;
const BANDWIDTH_MIB: u64 = 64;
const FAULT_MIB: u64 = 64;

/// CPUs for an injection. Explicit `cpus` are taken as given. Otherwise one logical CPU per physical core except the
/// render thread's core (its SMT sibling stays idle too: the SMT bias is about 9 %, `fa_probe::plan::load_cpus`).
/// `intensity` keeps that fraction of them, at least one.
pub fn pick_cpus(
    all: &[LogicalCpu],
    render: u32,
    explicit: Option<&[u32]>,
    intensity: f64,
) -> Vec<u32> {
    let mut cpus = match explicit {
        Some(e) => e.to_vec(),
        None => load_cpus(all, render),
    };
    let keep = ((cpus.len() as f64 * intensity).ceil() as usize).clamp(1, cpus.len().max(1));
    cpus.truncate(keep);
    cpus
}

/// Safe busy loop; with `fma`, the FMA power load of ASM-06 (intrinsics) instead.
fn cpu_burn(stop: &AtomicBool, use_fma: bool) {
    if use_fma {
        let isa = fma::best_isa();
        fma::burn(isa, stop, crate::check::load_block(isa));
        return;
    }
    let mut x = 1u64;
    while !stop.load(Ordering::Relaxed) {
        for _ in 0..4096 {
            x = black_box(x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1));
        }
    }
}

/// One pass over `buf`: every 64-byte line once, in a scattered order (a prime step in lines), dirtied. ASM-07: the
/// plain sweep produced the victim latency jump, `clflush` did not (see the catalog).
fn sweep(buf: &mut [u64]) {
    const LINE_WORDS: usize = 8;
    const STEP_LINES: usize = 67;
    let lines = buf.len() / LINE_WORDS;
    let mut line = 0usize;
    for _ in 0..lines {
        let i = line * LINE_WORDS;
        buf[i] = buf[i].wrapping_add(1);
        line = (line + STEP_LINES) % lines;
    }
}

/// One pass touching one word per 4 KiB page, hopping by an odd number of pages so that a power-of-two page count is
/// fully visited: far more pages than the TLB holds.
fn tlb_pass(buf: &mut [u64]) {
    const PAGE_WORDS: usize = 512;
    const STEP_PAGES: usize = 4099;
    let pages = buf.len() / PAGE_WORDS;
    let mut page = 0usize;
    for _ in 0..pages {
        let i = page * PAGE_WORDS;
        buf[i] = buf[i].wrapping_add(1);
        page = (page + STEP_PAGES) % pages;
    }
}

fn with_buffer(mib: u64, stop: &AtomicBool, mut pass: impl FnMut(&mut [u64])) {
    let bytes = usize::try_from(mib << 20).unwrap_or(0);
    match PageBuf::alloc(bytes, false) {
        Ok(mut buf) => {
            let words = buf.as_mut_u64();
            while !stop.load(Ordering::Relaxed) {
                pass(words);
            }
        }
        Err(e) => eprintln!("fa lab : allocation de {mib} Mio impossible : {e}"),
    }
}

fn page_fault_storm(stop: &AtomicBool, mib: u64) {
    let bytes = usize::try_from(mib << 20).unwrap_or(0);
    while !stop.load(Ordering::Relaxed) {
        if fault_cycle(bytes).and_then(|_| trim_working_set()).is_err() {
            break;
        }
    }
}

/// Builds the injector for `inj`, or `None` for a cause that is not a timed worker injection (frame-stall) or not
/// implemented yet. The CPUs are chosen when the injection starts, against the CPU the render thread is on then.
pub fn timed(inj: &Injection, all: &[LogicalCpu]) -> Option<Timed> {
    let intensity = inj.intensity.unwrap_or(1.0);
    let (size, fma_on) = (inj.size_mib, inj.fma.unwrap_or(false));
    // (all the CPUs of `intensity`, or a single CPU), the work, the parameters specific to the cause
    let (wide, work, params): (bool, Work, _) = match inj.cause {
        Cause::CpuBurn => (
            true,
            Arc::new(move |s: &AtomicBool| cpu_burn(s, fma_on)),
            json!({ "fma": fma_on }),
        ),
        Cause::CacheThrash => {
            let mib = size.unwrap_or(CACHE_MIB);
            (
                false,
                Arc::new(move |s: &AtomicBool| with_buffer(mib, s, sweep)),
                json!({ "size_mib": mib }),
            )
        }
        Cause::TlbThrash => {
            let mib = size.unwrap_or(TLB_MIB);
            (
                false,
                Arc::new(move |s: &AtomicBool| with_buffer(mib, s, tlb_pass)),
                json!({ "size_mib": mib }),
            )
        }
        Cause::MemBandwidth => {
            let mib = size.unwrap_or(BANDWIDTH_MIB);
            let kernel = best_intrinsics();
            (
                true,
                Arc::new(move |s: &AtomicBool| {
                    with_buffer(mib, s, |b| fill(b, 0x5555_5555_5555_5555, kernel))
                }),
                json!({ "size_mib_per_thread": mib }),
            )
        }
        Cause::PageFaultStorm => {
            let mib = size.unwrap_or(FAULT_MIB);
            (
                false,
                Arc::new(move |s: &AtomicBool| page_fault_storm(s, mib)),
                json!({ "size_mib": mib }),
            )
        }
        _ => return None,
    };
    let (all, explicit) = (all.to_vec(), inj.cpus.clone());
    let pick = Box::new(move |render: u32| {
        let cpus = pick_cpus(
            &all,
            render,
            explicit.as_deref(),
            if wide { intensity } else { 0.0 },
        );
        let mut params = params.clone();
        params["cpus"] = json!(cpus);
        params["render_cpu"] = json!(render);
        params["intensity"] = json!(intensity);
        (cpus, params)
    });
    Some(Timed::new(
        inj.cause,
        inj.start,
        inj.start + inj.duration,
        work,
        pick,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 6 physical cores, 2 logical CPUs each; siblings are adjacent (0-1, 2-3, ...), as on the target machine.
    fn topology() -> Vec<LogicalCpu> {
        (0..12)
            .map(|i| LogicalCpu {
                index: i,
                cpu_set_id: i,
                core: i / 2,
                llc: 0,
                efficiency_class: 0,
                numa_node: 0,
                group: 0,
            })
            .collect()
    }

    #[test]
    fn burners_never_use_the_render_core_or_its_smt_sibling() {
        for render in 0..12u32 {
            let cpus = pick_cpus(&topology(), render, None, 1.0);
            assert_eq!(cpus.len(), 5, "one logical CPU per other physical core");
            assert!(
                !cpus.contains(&render) && !cpus.contains(&(render ^ 1)),
                "render {render}: {cpus:?}"
            );
        }
    }

    #[test]
    fn intensity_keeps_a_fraction_but_at_least_one_cpu() {
        let all = topology();
        assert_eq!(pick_cpus(&all, 2, None, 0.5).len(), 3);
        assert_eq!(pick_cpus(&all, 2, None, 0.0).len(), 1);
        assert_eq!(
            pick_cpus(&all, 2, Some(&[8, 10]), 1.0),
            [8, 10],
            "explicit CPUs are kept as given"
        );
    }

    #[test]
    fn the_sweeps_visit_each_line_and_each_page_exactly_once() {
        let mut a = vec![0u64; 4096 * 8]; // 4096 lines
        sweep(&mut a);
        assert!(a
            .chunks(8)
            .all(|l| l[0] == 1 && l[1..].iter().all(|&w| w == 0)));
        let mut b = vec![0u64; 1024 * 512]; // 1024 pages
        tlb_pass(&mut b);
        assert!(b
            .chunks(512)
            .all(|p| p[0] == 1 && p[1..].iter().all(|&w| w == 0)));
    }
}
