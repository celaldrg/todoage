//! todoage: find TODO/FIXME/HACK comments in a git repository and report who wrote them and when.

use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub const DEFAULT_TAGS: &[&str] = &["TODO", "FIXME", "HACK", "XXX"];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Item {
    pub file: String,
    pub line: usize,
    pub tag: String,
    pub text: String,
    pub author: String,
    /// Unix seconds of the commit that introduced the line; None if uncommitted.
    pub timestamp: Option<i64>,
    pub age_days: Option<i64>,
    pub commit: Option<String>,
}

/// Build the marker regex: a tag as a whole word, optionally followed by (owner), then ':' or whitespace
/// or end of line. Requiring that separator skips string literals such as `"TODO"`.
pub fn marker_regex(tags: &[String]) -> Regex {
    let alt = tags
        .iter()
        .map(|t| regex::escape(t))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&format!(r"\b({alt})(?:\(([^)]*)\))?(?::|\s+|$)\s*(.*)$")).expect("valid regex")
}

/// Find markers in one file's text. Returns (line number, tag, text).
pub fn scan_text(text: &str, re: &Regex) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some(c) = re.captures(line) {
            let tag = c.get(1).unwrap().as_str().to_string();
            let mut body = c
                .get(3)
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            // strip trailing comment closers
            for closer in ["*/", "-->", "#}", "%}"] {
                if let Some(s) = body.strip_suffix(closer) {
                    body = s.trim_end().to_string();
                }
            }
            if let Some(owner) = c.get(2) {
                body = format!("({}) {}", owner.as_str(), body).trim().to_string();
            }
            if body.chars().count() > 160 {
                body = body.chars().take(157).collect::<String>() + "...";
            }
            out.push((i + 1, tag, body));
        }
    }
    out
}

pub struct BlameLine {
    pub author: String,
    pub time: Option<i64>,
    pub commit: Option<String>,
}

/// Parse `git blame --line-porcelain` output into a map from final line number to author/time.
pub fn parse_blame(porcelain: &str) -> HashMap<usize, BlameLine> {
    let mut map = HashMap::new();
    let mut cur_line = 0usize;
    let mut sha = String::new();
    let mut author = String::new();
    let mut time: Option<i64> = None;
    for l in porcelain.lines() {
        if let Some(content) = l.strip_prefix('\t') {
            let _ = content;
            let uncommitted = sha.chars().all(|c| c == '0');
            map.insert(
                cur_line,
                BlameLine {
                    author: if uncommitted {
                        "(uncommitted)".into()
                    } else {
                        author.clone()
                    },
                    time: if uncommitted { None } else { time },
                    commit: if uncommitted {
                        None
                    } else {
                        Some(sha[..sha.len().min(8)].to_string())
                    },
                },
            );
            continue;
        }
        let mut parts = l.split(' ');
        let first = parts.next().unwrap_or("");
        if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            sha = first.to_string();
            let _orig = parts.next();
            cur_line = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        } else if let Some(a) = l.strip_prefix("author ") {
            author = a.to_string();
        } else if let Some(t) = l.strip_prefix("author-time ") {
            time = t.trim().parse().ok();
        }
    }
    map
}

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Scan every tracked file in the repository (or under `paths`), then blame the matching lines.
pub fn scan_repo(
    repo: &Path,
    tags: &[String],
    paths: &[String],
    now: i64,
) -> Result<Vec<Item>, String> {
    let mut args = vec!["ls-files", "-z", "--"];
    let p: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    args.extend(p);
    let files = git(repo, &args)?;
    let re = marker_regex(tags);
    let mut items = Vec::new();
    for file in files.split('\0').filter(|f| !f.is_empty()) {
        let full = repo.join(file);
        let Ok(bytes) = std::fs::read(&full) else {
            continue;
        };
        if bytes.len() > 2_000_000 || bytes.iter().take(8000).any(|&b| b == 0) {
            continue; // large or binary
        }
        let text = String::from_utf8_lossy(&bytes);
        let hits = scan_text(&text, &re);
        if hits.is_empty() {
            continue;
        }
        let blame = git(repo, &["blame", "--line-porcelain", "-w", "--", file])
            .map(|s| parse_blame(&s))
            .unwrap_or_default();
        for (line, tag, body) in hits {
            let b = blame.get(&line);
            let ts = b.and_then(|b| b.time);
            items.push(Item {
                file: file.to_string(),
                line,
                tag,
                text: body,
                author: b
                    .map(|b| b.author.clone())
                    .unwrap_or_else(|| "(unknown)".into()),
                timestamp: ts,
                age_days: ts.map(|t| ((now - t).max(0)) / 86_400),
                commit: b.and_then(|b| b.commit.clone()),
            });
        }
    }
    Ok(items)
}

/// Human age: "today", "12d", "5mo", "2y 3mo".
pub fn human_age(days: Option<i64>) -> String {
    match days {
        None => "new".into(),
        Some(0) => "today".into(),
        Some(d) if d < 60 => format!("{d}d"),
        Some(d) if d < 365 => format!("{}mo", d / 30),
        Some(d) => {
            let y = d / 365;
            let m = (d % 365) / 30;
            if m == 0 {
                format!("{y}y")
            } else {
                format!("{y}y {m}mo")
            }
        }
    }
}

/// Parse durations like "90d", "6m"/"6mo", "2y", "3w" or a bare number of days.
pub fn parse_days(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: i64 = num.parse().ok()?;
    let mult = match unit {
        "" | "d" => 1,
        "w" => 7,
        "m" | "mo" => 30,
        "y" => 365,
        _ => return None,
    };
    Some(n * mult)
}
