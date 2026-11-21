//! Session handoff notes: the file where every Claude session leaves "what I did, what is left,
//! what is next". Sietch reads it and shows a briefing when you sit down.
//!
//! The file is append-only Markdown, one entry per session or day:
//!
//! ```text
//! ## 2026-11-21 14:05 · rust-remaster · Rust remaster
//! Did: rebuilt the launcher in the portfolio palette
//! Left: confirm the clone fix on Windows
//! Next: run sietch --reset-config and open the menu
//! ```
//!
//! `Did:`, `Left:` and `Next:` may also be followed by `- bullet` lines.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub when: String,
    pub project: String,
    pub title: String,
    pub did: Vec<String>,
    pub left: Vec<String>,
    pub next: Vec<String>,
}

#[derive(Clone, Copy)]
enum Label {
    Did,
    Left,
    Next,
}

fn label_of(line: &str) -> Option<(Label, &str)> {
    let (head, rest) = line.split_once(':')?;
    let label = match head.trim().to_lowercase().as_str() {
        "did" => Label::Did,
        "left" => Label::Left,
        "next" => Label::Next,
        _ => return None,
    };
    Some((label, rest.trim()))
}

/// Entries in file order (oldest first).
pub fn parse(text: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut current: Option<Label> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(head) = line.strip_prefix("## ") {
            let mut parts = head.splitn(3, " · ");
            entries.push(Entry {
                when: parts.next().unwrap_or("").trim().to_string(),
                project: parts.next().unwrap_or("").trim().to_string(),
                title: parts.next().unwrap_or("").trim().to_string(),
                ..Entry::default()
            });
            current = None;
            continue;
        }
        let Some(entry) = entries.last_mut() else {
            continue;
        };
        fn target(l: Label, e: &mut Entry) -> &mut Vec<String> {
            match l {
                Label::Did => &mut e.did,
                Label::Left => &mut e.left,
                Label::Next => &mut e.next,
            }
        }
        if let Some((label, rest)) = label_of(line) {
            current = Some(label);
            if !rest.is_empty() {
                target(label, entry).push(rest.to_string());
            }
        } else if let (Some(label), Some(bullet)) = (
            current,
            line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")),
        ) {
            target(label, entry).push(bullet.trim().to_string());
        }
    }
    entries
}

/// The newest entry of every project, newest first: one "worker" per project.
pub fn latest_per_project(entries: &[Entry]) -> Vec<Entry> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for e in entries.iter().rev() {
        let key = e.project.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(e.clone());
    }
    out
}

pub fn load(path: &Path) -> Vec<Entry> {
    std::fs::read_to_string(path)
        .map(|t| parse(&t))
        .unwrap_or_default()
}

/// `git pull` in the notes repo, giving up after `timeout` (offline, slow network).
pub fn pull(dir: &Path, timeout: Duration) -> Result<(), String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["pull", "--ff-only", "-q"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("git not available: {e}"))?;
    let end = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("git pull failed".into()),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git pull timed out".into());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Handoff

## 2026-11-20 18:00 · logbook · Old session
Did: first thing
Left: nothing
Next: ship it

## 2026-11-21 14:05 · rust-remaster · Rust remaster
Did: restyled the launcher
Left:
- confirm the clone fix on Windows
- delete the zip copy
Next: run sietch --reset-config

## 2026-11-21 16:30 · logbook · New session
Did: planned the phone release
Next: build the PWA manifest
";

    #[test]
    fn parses_entries_labels_and_bullets() {
        let e = parse(SAMPLE);
        assert_eq!(e.len(), 3);
        assert_eq!(e[1].when, "2026-11-21 14:05");
        assert_eq!(e[1].project, "rust-remaster");
        assert_eq!(e[1].title, "Rust remaster");
        assert_eq!(e[1].did, ["restyled the launcher"]);
        assert_eq!(
            e[1].left,
            ["confirm the clone fix on Windows", "delete the zip copy"]
        );
        assert_eq!(e[1].next, ["run sietch --reset-config"]);
    }

    #[test]
    fn the_newest_entry_of_each_project_wins() {
        let latest = latest_per_project(&parse(SAMPLE));
        let names: Vec<&str> = latest.iter().map(|e| e.project.as_str()).collect();
        assert_eq!(names, ["logbook", "rust-remaster"]);
        assert_eq!(latest[0].title, "New session");
    }

    #[test]
    fn junk_and_empty_files_are_fine() {
        assert!(parse("").is_empty());
        assert!(parse("random text\nDid: before any heading").is_empty());
        let e = parse("## only a date\nNext: x");
        assert_eq!(e[0].when, "only a date");
        assert_eq!(e[0].project, "");
        assert_eq!(e[0].next, ["x"]);
    }

    #[test]
    fn a_missing_file_loads_as_no_entries() {
        assert!(load(Path::new("/definitely/not/here/HANDOFF.md")).is_empty());
    }
}
