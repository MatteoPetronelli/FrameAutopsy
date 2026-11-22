//! `fa windows`: dev command that lists visible windows with their owner process, title and class name.
//! Read-only, no process handle. Used to read the real window of a game before writing a detection rule.

use fa_win::{procs, windows};
use std::collections::HashMap;

/// Image names that count as "Java" for the default listing.
const JAVA: [&str; 2] = ["java.exe", "javaw.exe"];

pub fn run(all: bool) -> std::io::Result<()> {
    let names: HashMap<u32, String> = procs::process_list()?.into_iter().collect();
    println!("pid,image,class,title");
    let mut shown = 0;
    for w in windows::visible_windows() {
        let image = names.get(&w.pid).map_or("?", String::as_str);
        let is_java = JAVA.iter().any(|j| image.eq_ignore_ascii_case(j));
        if (all || is_java) && !w.title.is_empty() {
            println!(
                "{},{},{},\"{}\"",
                w.pid,
                image,
                w.class,
                w.title.replace('"', "'")
            );
            shown += 1;
        }
    }
    if shown == 0 {
        println!(
            "(aucune fenêtre visible {})",
            if all {
                "avec un titre"
            } else {
                "de java.exe ou javaw.exe"
            }
        );
    }
    Ok(())
}

pub fn parse(rest: &[String]) -> Option<bool> {
    match rest {
        [] => Some(false),
        [f] if f == "--all" => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_all_flag_is_accepted() {
        assert_eq!(parse(&[]), Some(false));
        assert_eq!(parse(&["--all".to_string()]), Some(true));
        assert_eq!(parse(&["--x".to_string()]), None);
        assert_eq!(parse(&["--all".to_string(), "x".to_string()]), None);
    }
}
