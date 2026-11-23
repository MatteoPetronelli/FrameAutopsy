//! Lab guards (design D-8): refuse while a matrix game runs, cap injections, clean up temporary files.
//! The 10 GB free-space check of disk bursts arrives with the injector (task 6.1).

use super::render_loop::Fps;
use super::scenario::{Cause, Scenario};
use fa_core::matrix::Matrix;
use fa_probe::plan::running_games;
use fa_win::{procs, windows};
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::PathBuf;

/// Longest injection, in seconds.
pub const MAX_INJECTION_SECS: f64 = 30.0;
/// Biggest disk burst, in MiB (1 GiB).
pub const MAX_DISK_MIB: u64 = 1024;
/// Biggest buffer of a memory injector, in MiB per thread.
pub const MAX_MEMORY_MIB: u64 = 512;

#[derive(Debug, PartialEq)]
pub enum GuardError {
    BadFps(String),
    BadTiming { index: usize },
    TooLong { index: usize, seconds: f64 },
    PastTheEnd { index: usize },
    DiskBurstSize { index: usize },
    BadIntensity { index: usize },
    FrameStallLength { index: usize },
    MemorySize { index: usize },
}

impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardError::BadFps(s) => write!(f, "fps inconnu : {s}"),
            GuardError::BadTiming { index } => {
                write!(f, "injection {index} : début négatif ou durée nulle")
            }
            GuardError::TooLong { index, seconds } => write!(
                f,
                "injection {index} : {seconds} s, le maximum est {MAX_INJECTION_SECS} s"
            ),
            GuardError::PastTheEnd { index } => {
                write!(f, "injection {index} : finit après la fin du scénario")
            }
            GuardError::DiskBurstSize { index } => write!(
                f,
                "injection {index} : disk-burst demande size_mib entre 1 et {MAX_DISK_MIB} (1 Gio)"
            ),
            GuardError::BadIntensity { index } => {
                write!(f, "injection {index} : intensity hors de 0 à 1")
            }
            GuardError::MemorySize { index } => write!(
                f,
                "injection {index} : size_mib doit être entre 1 et {MAX_MEMORY_MIB} (par thread)"
            ),
            GuardError::FrameStallLength { index } => write!(
                f,
                "injection {index} : frame-stall demande stall_ms entre 0 (exclu) et la durée de l'injection"
            ),
        }
    }
}

/// Checks a scenario against the caps; nothing starts when this fails.
pub fn validate(s: &Scenario) -> Result<(), GuardError> {
    if Fps::parse(&s.fps).is_none() {
        return Err(GuardError::BadFps(s.fps.clone()));
    }
    for (index, inj) in s.injections.iter().enumerate() {
        if !(inj.start >= 0.0 && inj.duration > 0.0) {
            return Err(GuardError::BadTiming { index });
        }
        if inj.duration > MAX_INJECTION_SECS {
            return Err(GuardError::TooLong {
                index,
                seconds: inj.duration,
            });
        }
        if inj.start + inj.duration > s.duration {
            return Err(GuardError::PastTheEnd { index });
        }
        if inj.intensity.is_some_and(|i| !(0.0..=1.0).contains(&i)) {
            return Err(GuardError::BadIntensity { index });
        }
        if inj.cause == Cause::FrameStall
            && !inj
                .stall_ms
                .is_some_and(|ms| ms > 0.0 && ms <= inj.duration * 1000.0)
        {
            return Err(GuardError::FrameStallLength { index });
        }
        let memory = matches!(
            inj.cause,
            Cause::CacheThrash | Cause::TlbThrash | Cause::MemBandwidth | Cause::PageFaultStorm
        );
        if memory
            && inj
                .size_mib
                .is_some_and(|m| !(1..=MAX_MEMORY_MIB).contains(&m))
        {
            return Err(GuardError::MemorySize { index });
        }
        if inj.cause == Cause::DiskBurst
            && !inj
                .size_mib
                .is_some_and(|m| (1..=MAX_DISK_MIB).contains(&m))
        {
            return Err(GuardError::DiskBurstSize { index });
        }
    }
    Ok(())
}

/// The refusal, naming the game, when one of the matrix games is among `images` / `windows`.
pub fn refuse_if_game(
    matrix: &Matrix,
    images: &[String],
    windows: &[(String, String)],
) -> Result<(), String> {
    let running = running_games(matrix, images, windows);
    if running.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = running.iter().map(|(_, n)| n.as_str()).collect();
    Err(format!(
        "fa lab ne se lance pas pendant un jeu : {} est en cours. Fermez-le puis relancez la commande.",
        names.join(", ")
    ))
}

/// Same refusal against the live process list (image names and window classes only, no process handle).
pub fn ensure_no_game() -> io::Result<()> {
    let matrix = Matrix::embedded().map_err(io::Error::other)?;
    let list = procs::process_list()?;
    let owners: HashMap<u32, String> = list.iter().cloned().collect();
    let images: Vec<String> = list.into_iter().map(|(_, n)| n).collect();
    let wins: Vec<(String, String)> = windows::visible_windows()
        .into_iter()
        .filter_map(|w| owners.get(&w.pid).map(|n| (n.clone(), w.class)))
        .collect();
    refuse_if_game(&matrix, &images, &wins).map_err(io::Error::other)
}

/// A temporary file of the Lab, deleted when this guard drops (normal end, Ctrl+C unwinding or error).
#[allow(dead_code)] // first user: the disk-burst injector (task 6.1)
pub struct TempFile(pub PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::scenario::Injection;

    fn scenario(inj: Injection) -> Scenario {
        Scenario {
            name: "t".into(),
            fps: "144".into(),
            duration: 100.0,
            injections: vec![inj],
            expected_causes: vec![],
        }
    }

    fn inj(cause: Cause, duration: f64, size_mib: Option<u64>) -> Injection {
        Injection {
            cause,
            start: 1.0,
            duration,
            intensity: None,
            cpus: None,
            size_mib,
            stall_ms: None,
            fma: None,
        }
    }

    #[test]
    fn the_injection_cap_is_30_seconds_inclusive() {
        assert_eq!(validate(&scenario(inj(Cause::CpuBurn, 30.0, None))), Ok(()));
        assert!(matches!(
            validate(&scenario(inj(Cause::CpuBurn, 30.01, None))),
            Err(GuardError::TooLong { .. })
        ));
    }

    #[test]
    fn the_disk_cap_is_1_gib_inclusive_and_the_size_is_required() {
        assert_eq!(
            validate(&scenario(inj(Cause::DiskBurst, 5.0, Some(1024)))),
            Ok(())
        );
        assert!(validate(&scenario(inj(Cause::DiskBurst, 5.0, Some(1025)))).is_err());
        assert!(validate(&scenario(inj(Cause::DiskBurst, 5.0, Some(0)))).is_err());
        assert!(validate(&scenario(inj(Cause::DiskBurst, 5.0, None))).is_err());
    }

    #[test]
    fn the_memory_cap_is_512_mib_inclusive_per_thread() {
        for cause in [
            Cause::CacheThrash,
            Cause::TlbThrash,
            Cause::MemBandwidth,
            Cause::PageFaultStorm,
        ] {
            assert_eq!(validate(&scenario(inj(cause, 5.0, Some(512)))), Ok(()));
            assert_eq!(
                validate(&scenario(inj(cause, 5.0, None))),
                Ok(()),
                "the size has a default"
            );
            assert!(matches!(
                validate(&scenario(inj(cause, 5.0, Some(513)))),
                Err(GuardError::MemorySize { .. })
            ));
            assert!(validate(&scenario(inj(cause, 5.0, Some(0)))).is_err());
        }
    }

    #[test]
    fn a_running_matrix_game_is_refused_by_name() {
        let matrix = Matrix::embedded().unwrap();
        let err = refuse_if_game(
            &matrix,
            &["explorer.exe".into(), "Overwatch.exe".into()],
            &[],
        )
        .unwrap_err();
        assert!(err.contains("Overwatch"), "{err}");
        assert!(
            refuse_if_game(&matrix, &["explorer.exe".into(), "chrome.exe".into()], &[]).is_ok()
        );
    }

    #[test]
    fn a_temp_file_is_deleted_when_its_guard_drops() {
        let path = std::env::temp_dir().join(format!("fa-lab-guard-{}.tmp", std::process::id()));
        std::fs::write(&path, b"x").unwrap();
        drop(TempFile(path.clone()));
        assert!(!path.exists());
    }
}
