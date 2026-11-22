//! `TargetProcess`: the only value the governor may act on. A game, an anti-cheat, a critical process or one of our
//! own cannot be represented, so "act on the game" is a type error instead of a discipline (spec `compliance-matrix`).
//! Adding a refusal is a code change, never a config change.

use crate::matrix::{GameId, Matrix};

const CRITICAL: [&str; 9] = [
    "system",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "dwm.exe",
    "audiodg.exe",
];
const OWN: [&str; 4] = ["fa.exe", "fa-collector.exe", "fa-watchdog.exe", "fa-ui.exe"];
/// Minecraft is identified by window class, which this type does not carry: any bare Java launcher is refused.
const JAVA: [&str; 2] = ["java.exe", "javaw.exe"];
/// Classic kernel process events truncate image names to 15 bytes.
const KERNEL_NAME_LEN: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetProcess {
    pid: u32,
    image_name: String,
    creation_time: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TargetError {
    #[error("empty image name")]
    Empty,
    #[error("image name {0:?} is not a bare file name")]
    NotBareName(String),
    #[error("image name {0:?} is {KERNEL_NAME_LEN} bytes: possibly truncated by the kernel, refused (fail closed)")]
    MaybeTruncated(String),
    #[error("{0:?} is a matrix game")]
    Game(String),
    #[error("{0:?} is an anti-cheat process")]
    AntiCheat(String),
    #[error("{0:?} is a critical system process")]
    Critical(String),
    #[error("{0:?} is a FrameAutopsy process")]
    Own(String),
    #[error("{0:?} may be Minecraft (identified by window class only): refused")]
    JavaLauncher(String),
}

impl TargetProcess {
    pub fn try_new(
        pid: u32,
        image_name: &str,
        creation_time: u64,
        matrix: &Matrix,
    ) -> Result<Self, TargetError> {
        if image_name.is_empty() {
            return Err(TargetError::Empty);
        }
        if image_name.contains(['\\', '/', ':']) {
            return Err(TargetError::NotBareName(image_name.to_owned()));
        }
        let n = image_name.to_ascii_lowercase();
        let same = |s: &str| s.eq_ignore_ascii_case(&n);
        let e = || image_name.to_owned();
        if pid == std::process::id() || OWN.iter().any(|s| same(s)) {
            return Err(TargetError::Own(e()));
        }
        if image_name.len() == KERNEL_NAME_LEN {
            return Err(TargetError::MaybeTruncated(image_name.to_owned()));
        }
        if CRITICAL.iter().any(|s| same(s)) {
            return Err(TargetError::Critical(e()));
        }
        if JAVA.iter().any(|s| same(s)) {
            return Err(TargetError::JavaLauncher(e()));
        }
        for g in GameId::ALL {
            let game = matrix.game(g);
            if game.detect.iter().any(|s| same(s)) {
                return Err(TargetError::Game(e()));
            }
            if game
                .anti_cheat
                .as_ref()
                .is_some_and(|a| a.processes.iter().any(|s| same(s)))
            {
                return Err(TargetError::AntiCheat(e()));
            }
        }
        Ok(TargetProcess {
            pid,
            image_name: image_name.to_owned(),
            creation_time,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn image_name(&self) -> &str {
        &self.image_name
    }

    /// Distinguishes a reused PID from the process the target was built for.
    pub fn creation_time(&self) -> u64 {
        self.creation_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_new(name: &str) -> Result<TargetProcess, TargetError> {
        // PID 1 is never the test process, so only the name decides.
        TargetProcess::try_new(1, name, 0, &Matrix::embedded().unwrap())
    }

    #[test]
    fn refuses_the_matrix_games() {
        for n in [
            "League of Legends.exe",
            "VALORANT-Win64-Shipping.exe",
            "FortniteClient-Win64-Shipping.exe",
            "Overwatch.exe",
        ] {
            assert!(matches!(try_new(n), Err(TargetError::Game(_))), "{n}");
        }
        for n in ["java.exe", "javaw.exe"] {
            assert!(
                matches!(try_new(n), Err(TargetError::JavaLauncher(_))),
                "{n}"
            );
        }
    }

    #[test]
    fn refuses_the_listed_anti_cheats() {
        for n in [
            "vgc.exe",
            "vgtray.exe",
            "EasyAntiCheat.exe",
            "EasyAntiCheat_EOS.exe",
            "BEService.exe",
        ] {
            assert!(matches!(try_new(n), Err(TargetError::AntiCheat(_))), "{n}");
        }
    }

    #[test]
    fn refuses_the_critical_processes() {
        // Hard-coded on purpose: iterating over `CRITICAL` would shrink with the code under test.
        let names = [
            "System",
            "smss.exe",
            "csrss.exe",
            "wininit.exe",
            "winlogon.exe",
            "services.exe",
            "lsass.exe",
            "dwm.exe",
            "audiodg.exe",
        ];
        for n in names {
            assert!(matches!(try_new(n), Err(TargetError::Critical(_))), "{n}");
        }
    }

    #[test]
    fn refuses_own_binaries_and_own_pid() {
        for n in OWN {
            assert!(matches!(try_new(n), Err(TargetError::Own(_))), "{n}");
        }
        let m = Matrix::embedded().unwrap();
        let own = TargetProcess::try_new(std::process::id(), "notepad.exe", 0, &m);
        assert!(matches!(own, Err(TargetError::Own(_))));
    }

    #[test]
    fn refuses_a_truncated_kernel_name() {
        assert_eq!("VALORANT-Win64-".len(), 15);
        assert!(matches!(
            try_new("VALORANT-Win64-"),
            Err(TargetError::MaybeTruncated(_))
        ));
        assert!(matches!(
            try_new("abcdefghijklmno"),
            Err(TargetError::MaybeTruncated(_))
        ));
        // 14 and 16 bytes are not truncations.
        assert!(try_new("abcdefghijklmn").is_ok() && try_new("abcdefghijklmnop").is_ok());
    }

    #[test]
    fn handles_empty_path_and_case_variants() {
        assert_eq!(try_new("").unwrap_err(), TargetError::Empty);
        assert!(matches!(
            try_new("C:\\Windows\\notepad.exe"),
            Err(TargetError::NotBareName(_))
        ));
        assert!(matches!(
            try_new("dir/notepad.exe"),
            Err(TargetError::NotBareName(_))
        ));
        assert!(matches!(try_new("VGC.EXE"), Err(TargetError::AntiCheat(_))));
        assert!(matches!(
            try_new("LSASS.exe"),
            Err(TargetError::Critical(_))
        ));
        assert!(matches!(
            try_new("OVERWATCH.EXE"),
            Err(TargetError::Game(_))
        ));
    }

    #[test]
    fn accepts_an_ordinary_background_process() {
        let t = try_new("notepad.exe").unwrap();
        assert_eq!(
            (t.pid(), t.image_name(), t.creation_time()),
            (1, "notepad.exe", 0)
        );
    }
}
