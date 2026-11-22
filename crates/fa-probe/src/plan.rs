//! Pure planning pieces of `fa check`: which CPUs carry the load, the deadline, and the refusal when a game runs.

use crate::bandwidth::physical_core_cpus;
use fa_core::matrix::{GameId, Matrix};
use fa_win::topology::LogicalCpu;
use std::time::{Duration, Instant};

/// One logical CPU of every physical core except the sampler's core: the sampler's SMT sibling must stay idle, because
/// a busy sibling makes the frequency probe read about 9 % low (lot 3 measurement).
pub fn load_cpus(cpus: &[LogicalCpu], sampler: u32) -> Vec<u32> {
    let core_of = |i: u32| cpus.iter().find(|c| c.index == i).map(|c| c.core);
    let mine = core_of(sampler);
    physical_core_cpus(cpus)
        .into_iter()
        .filter(|c| core_of(*c) != mine)
        .collect()
}

/// Total budget of a run. `fa check` must end within 90 s: it aborts cleanly at 85 s.
pub const TOTAL_BUDGET: Duration = Duration::from_secs(85);

#[derive(Clone, Copy, Debug)]
pub struct Deadline {
    start: Instant,
    total: Duration,
}

impl Deadline {
    pub fn new(start: Instant, total: Duration) -> Deadline {
        Deadline { start, total }
    }

    /// Time left at `now`.
    pub fn remaining_at(&self, now: Instant) -> Duration {
        self.total
            .saturating_sub(now.saturating_duration_since(self.start))
    }

    pub fn remaining(&self) -> Duration {
        self.remaining_at(Instant::now())
    }

    /// The part of `wanted` that still fits at `now` once `reserve` is kept for what comes after.
    pub fn budget_at(&self, now: Instant, wanted: Duration, reserve: Duration) -> Duration {
        wanted.min(self.remaining_at(now).saturating_sub(reserve))
    }

    pub fn budget(&self, wanted: Duration, reserve: Duration) -> Duration {
        self.budget_at(Instant::now(), wanted, reserve)
    }
}

/// Names of the matrix games that are running: by full image name, and Minecraft by window. `images` are the process
/// image names; `windows` are `(owner image name, window class)` pairs of the visible top-level windows.
pub fn running_games(
    matrix: &Matrix,
    images: &[String],
    windows: &[(String, String)],
) -> Vec<(GameId, String)> {
    let mut ids = matrix.running_games(images);
    for g in matrix.running_games_by_window(windows) {
        if !ids.contains(&g) {
            ids.push(g);
        }
    }
    ids.into_iter()
        .map(|g| (g, matrix.game(g).name.clone()))
        .collect()
}

/// The French refusal message, one line per running game.
pub fn refusal_message(running: &[(GameId, String)]) -> String {
    let names: Vec<&str> = running.iter().map(|(_, n)| n.as_str()).collect();
    format!(
        "fa check ne se lance pas pendant un jeu : {} est en cours. Fermez-le puis relancez la commande.",
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu(index: u32, core: u32) -> LogicalCpu {
        LogicalCpu {
            index,
            cpu_set_id: index,
            core,
            llc: 0,
            efficiency_class: 0,
            numa_node: 0,
            group: 0,
        }
    }

    fn smt(cores: u32) -> Vec<LogicalCpu> {
        (0..cores * 2).map(|i| cpu(i, i / 2)).collect()
    }

    #[test]
    fn the_target_machine_loads_one_cpu_on_each_of_the_five_other_cores() {
        // 6 cores, 12 threads, sampler CPU 4 (core 2, siblings 4 and 5)
        assert_eq!(load_cpus(&smt(6), 4), vec![0, 2, 6, 8, 10]);
    }

    #[test]
    fn the_sampler_sibling_is_never_loaded_whichever_sibling_samples() {
        for sampler in [4, 5] {
            let l = load_cpus(&smt(6), sampler);
            assert!(!l.contains(&4) && !l.contains(&5), "{l:?}");
            assert_eq!(l.len(), 5);
        }
    }

    #[test]
    fn without_smt_every_other_cpu_is_loaded() {
        let v: Vec<LogicalCpu> = (0..4).map(|i| cpu(i, i)).collect();
        assert_eq!(load_cpus(&v, 1), vec![0, 2, 3]);
    }

    #[test]
    fn a_single_core_machine_loads_nothing() {
        assert!(load_cpus(&[cpu(0, 0), cpu(1, 0)], 0).is_empty());
        assert!(load_cpus(&[], 0).is_empty());
    }

    #[test]
    fn the_hybrid_fixture_loads_one_cpu_per_other_core_and_spares_the_sibling() {
        // 2 P cores with SMT (cpus 0..4) and 4 E cores without (cpus 4..8)
        let mut v = vec![cpu(0, 0), cpu(1, 0), cpu(2, 1), cpu(3, 1)];
        v.extend((4..8).map(|i| cpu(i, i - 2)));
        assert_eq!(load_cpus(&v, 0), vec![2, 4, 5, 6, 7]);
    }

    #[test]
    fn the_deadline_counts_down_and_clamps_the_phases() {
        let t0 = Instant::now();
        let d = Deadline::new(t0, Duration::from_secs(85));
        let at = |s| t0 + Duration::from_secs(s);
        assert_eq!(d.remaining_at(at(0)), Duration::from_secs(85));
        assert_eq!(d.remaining_at(at(20)), Duration::from_secs(65));
        assert_eq!(d.remaining_at(at(500)), Duration::ZERO);
        let sec = Duration::from_secs;
        // a 60 s load with 13 s reserved for the phases after it
        assert_eq!(d.budget_at(at(10), sec(60), sec(13)), sec(60));
        assert_eq!(d.budget_at(at(30), sec(60), sec(13)), sec(42));
        assert_eq!(d.budget_at(at(90), sec(60), sec(13)), Duration::ZERO);
    }

    #[test]
    fn a_running_league_of_legends_is_refused_by_name() {
        let m = Matrix::embedded().unwrap();
        let images = vec![
            "explorer.exe".to_owned(),
            "League of Legends.exe".to_owned(),
        ];
        let r = running_games(&m, &images, &[]);
        assert_eq!(r.len(), 1);
        assert!(refusal_message(&r).contains("League of Legends"));
    }

    #[test]
    fn minecraft_is_refused_by_window_and_a_bare_javaw_is_not() {
        let m = Matrix::embedded().unwrap();
        let w = |i: &str, c: &str| (i.to_owned(), c.to_owned());
        let javaw = vec!["javaw.exe".to_owned()];
        assert_eq!(
            running_games(&m, &javaw, &[w("javaw.exe", "GLFW30")]).len(),
            1
        );
        assert!(running_games(&m, &javaw, &[w("javaw.exe", "SunAwtFrame")]).is_empty());
        assert!(running_games(&m, &javaw, &[]).is_empty());
    }

    #[test]
    fn two_games_are_both_named_without_duplicates() {
        let m = Matrix::embedded().unwrap();
        let images = vec!["League of Legends.exe".to_owned(), "javaw.exe".to_owned()];
        let w = vec![("javaw.exe".to_owned(), "GLFW30".to_owned())];
        let r = running_games(&m, &images, &w);
        assert_eq!(r.len(), 2);
        assert!(refusal_message(&r).contains(", "));
    }
}
