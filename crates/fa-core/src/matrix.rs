//! Compliance matrix: which observer or action may run for which game (`compliance.toml`, spec `compliance-matrix`).
//! Fails safe: a missing or malformed entry is a load error, an invalid override is ignored as a whole.

use serde::Deserialize;
use std::collections::BTreeMap;

const EMBEDDED: &str = include_str!("../../../compliance.toml");
/// Cap on an override file: it comes from a user-writable path, so it is untrusted input.
pub const OVERRIDE_MAX_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Allowed,
    Gray,
    Forbidden,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum GameId {
    #[serde(rename = "lol")]
    Lol,
    #[serde(rename = "valorant")]
    Valorant,
    #[serde(rename = "fortnite")]
    Fortnite,
    #[serde(rename = "overwatch2")]
    Overwatch2,
    #[serde(rename = "minecraft-java")]
    MinecraftJava,
    #[serde(rename = "fa-lab")]
    FaLab,
    #[serde(rename = "unknown")]
    Unknown,
}

impl GameId {
    pub const ALL: [GameId; 7] = [
        GameId::Lol,
        GameId::Valorant,
        GameId::Fortnite,
        GameId::Overwatch2,
        GameId::MinecraftJava,
        GameId::FaLab,
        GameId::Unknown,
    ];
}

/// Observers O1..O7 and actions A1..A5, A7, A8 (A6, timer resolution, is dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum Item {
    O1,
    O2,
    O3,
    O4,
    O5,
    O6,
    O7,
    A1,
    A2,
    A3,
    A4,
    A5,
    A7,
    A8,
}

impl Item {
    pub const ALL: [Item; 14] = [
        Item::O1,
        Item::O2,
        Item::O3,
        Item::O4,
        Item::O5,
        Item::O6,
        Item::O7,
        Item::A1,
        Item::A2,
        Item::A3,
        Item::A4,
        Item::A5,
        Item::A7,
        Item::A8,
    ];
    /// Baseline observers: on for every game unless `forbidden` (present events, kernel events, probes, counters).
    pub const BASELINE: [Item; 4] = [Item::O1, Item::O2, Item::O4, Item::O5];
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub verdict: Verdict,
    #[serde(default)]
    pub sources: Vec<String>,
    pub date: Option<String>,
    pub test_record: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AntiCheat {
    pub label: String,
    #[serde(default)]
    pub processes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Game {
    pub name: String,
    #[serde(default)]
    pub detect: Vec<String>,
    pub window_title_prefix: Option<String>,
    pub anti_cheat: Option<AntiCheat>,
    pub items: BTreeMap<Item, Cell>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MatrixError {
    #[error("compliance matrix does not parse: {0}")]
    Parse(String),
    #[error("game {0:?} has no entry in the matrix")]
    MissingGame(GameId),
    #[error("game {0:?} has no entry for item {1:?}")]
    MissingItem(GameId, Item),
    #[error("{0:?}/{1:?} is `allowed` without a source URL or a valid ISO date")]
    AllowedWithoutSource(GameId, Item),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OverrideError {
    #[error("override file is larger than {OVERRIDE_MAX_BYTES} bytes")]
    TooLarge,
    #[error("override does not parse (unknown game or item, or wrong shape): {0}")]
    Parse(String),
    #[error("override may only set `forbidden`; {0:?}/{1:?} tries {2:?}")]
    CannotEnable(GameId, Item, Verdict),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    games: BTreeMap<GameId, Game>,
}

#[derive(Debug, Clone)]
pub struct Matrix {
    games: BTreeMap<GameId, Game>,
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

impl Matrix {
    /// The matrix compiled into the binary; not replaceable at runtime by a non-admin user.
    pub fn embedded() -> Result<Matrix, MatrixError> {
        Matrix::parse(EMBEDDED)
    }

    fn parse(text: &str) -> Result<Matrix, MatrixError> {
        let file: File = toml::from_str(text).map_err(|e| MatrixError::Parse(e.to_string()))?;
        for g in GameId::ALL {
            let game = file.games.get(&g).ok_or(MatrixError::MissingGame(g))?;
            for it in Item::ALL {
                let cell = game.items.get(&it).ok_or(MatrixError::MissingItem(g, it))?;
                let sourced =
                    !cell.sources.is_empty() && cell.date.as_deref().is_some_and(is_iso_date);
                if cell.verdict == Verdict::Allowed && !sourced {
                    return Err(MatrixError::AllowedWithoutSource(g, it));
                }
            }
        }
        Ok(Matrix { games: file.games })
    }

    pub fn game(&self, g: GameId) -> &Game {
        &self.games[&g] // complete by construction (checked in `parse`)
    }

    pub fn verdict(&self, g: GameId, it: Item) -> Verdict {
        self.game(g).items[&it].verdict
    }

    /// Baseline observers run unless `forbidden`; everything else only when `allowed`.
    pub fn enabled(&self, g: GameId, it: Item) -> bool {
        let v = self.verdict(g, it);
        if Item::BASELINE.contains(&it) {
            v != Verdict::Forbidden
        } else {
            v == Verdict::Allowed
        }
    }

    /// Applies a user override (`[game]` tables of `ITEM = "forbidden"`). Any other value, unknown game or item,
    /// oversize or malformed file is rejected as a whole; the caller keeps the embedded matrix (fail closed).
    pub fn with_override(&self, text: &str) -> Result<Matrix, OverrideError> {
        if text.len() > OVERRIDE_MAX_BYTES {
            return Err(OverrideError::TooLarge);
        }
        let ov: BTreeMap<GameId, BTreeMap<Item, Verdict>> =
            toml::from_str(text).map_err(|e| OverrideError::Parse(e.to_string()))?;
        let mut out = self.clone();
        for (g, items) in ov {
            for (it, v) in items {
                if v != Verdict::Forbidden {
                    return Err(OverrideError::CannotEnable(g, it, v));
                }
                if let Some(cell) = out
                    .games
                    .get_mut(&g)
                    .and_then(|game| game.items.get_mut(&it))
                {
                    cell.verdict = v;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> Matrix {
        Matrix::embedded().expect("embedded matrix loads")
    }

    #[test]
    fn every_game_has_every_item() {
        let m = m();
        for g in GameId::ALL {
            assert_eq!(m.game(g).items.len(), Item::ALL.len(), "{g:?}");
        }
    }

    #[test]
    fn baseline_runs_unless_forbidden_other_items_need_allowed() {
        let mut m = m();
        for (v, base, other) in [
            (Verdict::Allowed, true, true),
            (Verdict::Unverified, true, false),
            (Verdict::Gray, true, false),
            (Verdict::Forbidden, false, false),
        ] {
            for it in [Item::O1, Item::O3, Item::A1] {
                m.games
                    .get_mut(&GameId::Lol)
                    .unwrap()
                    .items
                    .get_mut(&it)
                    .unwrap()
                    .verdict = v;
            }
            assert_eq!(m.enabled(GameId::Lol, Item::O1), base, "{v:?}");
            assert_eq!(m.enabled(GameId::Lol, Item::O3), other, "{v:?}");
            assert_eq!(m.enabled(GameId::Lol, Item::A1), other, "{v:?}");
        }
    }

    #[test]
    fn unknown_game_runs_baseline_only() {
        let m = m();
        for it in Item::ALL {
            assert_eq!(
                m.enabled(GameId::Unknown, it),
                Item::BASELINE.contains(&it),
                "{it:?}"
            );
        }
    }

    #[test]
    fn no_action_is_enabled_for_a_matrix_game() {
        let m = m();
        for g in [
            GameId::Lol,
            GameId::Valorant,
            GameId::Fortnite,
            GameId::Overwatch2,
            GameId::MinecraftJava,
        ] {
            for it in [Item::A1, Item::A2, Item::A3, Item::A4, Item::A5, Item::A7] {
                assert!(!m.enabled(g, it), "{g:?}/{it:?}");
            }
        }
    }

    #[test]
    fn allowed_cells_have_sources_and_a_date() {
        let m = m();
        for g in GameId::ALL {
            for it in Item::ALL {
                let c = &m.game(g).items[&it];
                if c.verdict == Verdict::Allowed {
                    assert!(
                        !c.sources.is_empty()
                            && c.sources.iter().all(|s| s.starts_with("https://"))
                    );
                    assert!(is_iso_date(c.date.as_deref().unwrap()));
                }
            }
        }
    }

    #[test]
    fn allowed_without_source_is_rejected() {
        let bad = EMBEDDED.replacen(
            "sources = [\"https://github.com/MatteoPetronelli/FrameAutopsy\"]",
            "sources = []",
            1,
        );
        assert!(matches!(
            Matrix::parse(&bad),
            Err(MatrixError::AllowedWithoutSource(..))
        ));
    }

    #[test]
    fn missing_item_is_a_load_error() {
        let bad = EMBEDDED.replacen("[games.lol.items.A8]", "[games.lol.items.A8_gone]", 1);
        assert!(Matrix::parse(&bad).is_err());
    }

    #[test]
    fn override_can_disable() {
        let m = m().with_override("[lol]\nO1 = \"forbidden\"\n").unwrap();
        assert!(!m.enabled(GameId::Lol, Item::O1));
        assert!(m.enabled(GameId::Valorant, Item::O1));
    }

    #[test]
    fn override_cannot_enable_and_names_the_item() {
        let err = m()
            .with_override("[valorant]\nA1 = \"allowed\"\n")
            .unwrap_err();
        assert_eq!(
            err,
            OverrideError::CannotEnable(GameId::Valorant, Item::A1, Verdict::Allowed)
        );
        assert!(err.to_string().contains("A1"));
    }

    #[test]
    fn override_rejects_unknown_game_unknown_item_and_oversize() {
        let m = m();
        assert!(matches!(
            m.with_override("[nope]\nA1 = \"forbidden\"\n"),
            Err(OverrideError::Parse(_))
        ));
        assert!(matches!(
            m.with_override("[lol]\nZ9 = \"forbidden\"\n"),
            Err(OverrideError::Parse(_))
        ));
        let big = format!("# {}\n", "x".repeat(OVERRIDE_MAX_BYTES));
        assert_eq!(m.with_override(&big).unwrap_err(), OverrideError::TooLarge);
    }
}
