//! Source test (spec `compliance-matrix`, "No handle on game processes"): no code may open a process handle except
//! inside `fa_win::process::open_target`. Textual by design; `unsafe` is confined to three crates and the
//! `frameautopsy-tos-guard` review covers macro-generated names.

use std::fs;
use std::path::{Path, PathBuf};

/// The only place allowed to open a process handle (created by lot 12; absent until then).
const ALLOWED_FILE: &str = "crates/fa-win/src/process.rs";
const ALLOWED_FN: &str = "fn open_target";
const OPENERS: [&str; 4] = [
    "OpenProcess",
    "OpenProcessToken",
    "NtOpenProcess",
    "OpenThread",
];

/// Blanks out comments, string literals and char literals (keeps newlines) so tokens inside them are ignored.
fn strip(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < b.len() && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if b[i] == '*' && b.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(b[i]));
                    i += 1;
                }
            }
        } else if c == 'r'
            && matches!(next, Some('"' | '#'))
            && (i == 0 || !b[i - 1].is_alphanumeric() && b[i - 1] != '_')
        {
            // Raw string r"..." / r#"..."#.
            let mut j = i + 1;
            let mut hashes = 0;
            while b.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) != Some(&'"') {
                out.push(c);
                i += 1;
                continue;
            }
            j += 1;
            loop {
                if j >= b.len() {
                    break;
                }
                if b[j] == '"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&'#')) {
                    j += 1 + hashes;
                    break;
                }
                j += 1;
            }
            out.extend(b[i..j.min(b.len())].iter().map(|&ch| blank(ch)));
            i = j;
        } else if c == '"' {
            out.push(' ');
            i += 1;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' && i + 1 < b.len() {
                    out.push(blank(b[i]));
                    i += 1;
                }
                out.push(blank(b[i]));
                i += 1;
            }
            out.push(' ');
            i += 1;
        } else if c == '\'' {
            // Char literal ('x', '\n', '\u{..}') versus lifetime ('a): a literal closes within a few chars.
            let close = if next == Some('\\') {
                b[i + 2..]
                    .iter()
                    .take(10)
                    .position(|&ch| ch == '\'')
                    .map(|p| i + 2 + p)
            } else if b.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            match close {
                Some(end) => {
                    out.extend(b[i..=end].iter().map(|&ch| blank(ch)));
                    i = end + 1;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Byte range of the body of `fn open_target`, from its first `{` to the matching `}`.
fn fn_body(stripped: &str) -> Option<(usize, usize)> {
    let start = stripped.find(ALLOWED_FN)?;
    let open = start + stripped[start..].find('{')?;
    let mut depth = 0;
    for (k, ch) in stripped[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, open + k));
                }
            }
            _ => {}
        }
    }
    None
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Offsets of every whole-word occurrence of an opener in already-stripped source.
fn hits(stripped: &str) -> Vec<usize> {
    let bytes = stripped.as_bytes();
    let mut out = Vec::new();
    for tok in OPENERS {
        for (at, _) in stripped.match_indices(tok) {
            let before = at == 0 || !is_ident(bytes[at - 1]);
            let after = bytes.get(at + tok.len()).is_none_or(|c| !is_ident(*c));
            if before && after {
                out.push(at);
            }
        }
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "target") {
                rust_files(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Violations in one file: `rel` is its path relative to the repository root, with forward slashes.
fn violations(rel: &str, src: &str) -> Vec<usize> {
    let s = strip(src);
    let h = hits(&s);
    if rel != ALLOWED_FILE {
        return h;
    }
    let body = fn_body(&s);
    h.into_iter()
        .filter(|at| !body.is_some_and(|(a, b)| (a..b).contains(at)))
        .collect()
}

#[test]
fn no_open_process_outside_open_target() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    assert!(files.len() >= 4, "scanner found no sources: wrong root?");
    let mut bad = Vec::new();
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let src = fs::read_to_string(&f).unwrap();
        for at in violations(&rel, &src) {
            let line = src[..at.min(src.len())].matches('\n').count() + 1;
            bad.push(format!("{rel}:{line}"));
        }
    }
    assert!(
        bad.is_empty(),
        "process-handle opener outside open_target: {bad:?}"
    );
}

#[test]
fn scanner_ignores_comments_and_strings_but_catches_code() {
    let other = "crates/fa-probe/src/lib.rs";
    assert!(violations(other, "// OpenProcess\n/* NtOpenProcess /* nested */ */\nlet s = \"OpenProcess\";\nlet r = r#\"OpenThread \"x\"\"#;\nlet c = '\"'; let l: &'static str = \"\";").is_empty());
    assert_eq!(
        violations(other, "unsafe { OpenProcess(0, 0, pid) }").len(),
        1
    );
    assert_eq!(
        violations(
            other,
            "use windows_sys::Win32::System::Threading::NtOpenProcess;"
        )
        .len(),
        1
    );
    assert!(
        violations(other, "fn MyOpenProcessor() {}").is_empty(),
        "whole words only"
    );
}

#[test]
fn only_open_target_is_allowed_to_open_a_handle() {
    let src = "fn other() { OpenProcess(); }\nfn open_target(t: &T) { if x { OpenProcess(); } }\nfn after() { OpenThread(); }";
    assert_eq!(
        violations(ALLOWED_FILE, src).len(),
        2,
        "inside open_target is allowed, outside is not"
    );
    assert_eq!(
        violations(
            "crates/fa-win/src/lib.rs",
            "fn open_target() { OpenProcess(); }"
        )
        .len(),
        1
    );
}
