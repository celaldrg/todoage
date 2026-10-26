use std::path::Path;
use std::process::Command;
use todoage::*;

fn tags() -> Vec<String> {
    DEFAULT_TAGS.iter().map(|s| s.to_string()).collect()
}

#[test]
fn finds_markers_in_many_comment_styles() {
    let re = marker_regex(&tags());
    let src = "// TODO: rust style\n# FIXME python style\n/* HACK(alice): c block */\n<!-- XXX html -->\n-- TODO sql\nlet s = \"TODO\";\nlet todo_list = 1;\nTODOS are not markers\n// XXX\n";
    let hits = scan_text(src, &re);
    let got: Vec<(usize, &str, &str)> = hits
        .iter()
        .map(|(l, t, x)| (*l, t.as_str(), x.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (1, "TODO", "rust style"),
            (2, "FIXME", "python style"),
            (3, "HACK", "(alice) c block"),
            (4, "XXX", "html"),
            (5, "TODO", "sql"),
            (9, "XXX", ""),
        ]
    );
}

#[test]
fn custom_tags() {
    let re = marker_regex(&["NOTE".to_string()]);
    assert_eq!(scan_text("// NOTE: x\n// TODO: y", &re).len(), 1);
}

#[test]
fn ages_and_durations() {
    assert_eq!(human_age(None), "new");
    assert_eq!(human_age(Some(0)), "today");
    assert_eq!(human_age(Some(45)), "45d");
    assert_eq!(human_age(Some(200)), "6mo");
    assert_eq!(human_age(Some(365)), "1y");
    assert_eq!(human_age(Some(800)), "2y 2mo");
    assert_eq!(parse_days("90d"), Some(90));
    assert_eq!(parse_days("90"), Some(90));
    assert_eq!(parse_days("3w"), Some(21));
    assert_eq!(parse_days("6mo"), Some(180));
    assert_eq!(parse_days("2y"), Some(730));
    assert_eq!(parse_days("soon"), None);
}

#[test]
fn parses_line_porcelain() {
    let sha = "a".repeat(40);
    let zero = "0".repeat(40);
    let p = format!(
        "{sha} 1 1 1\nauthor Ada\nauthor-time 1000\nsummary x\nfilename f\n\tline one\n{zero} 2 2 1\nauthor Not Committed Yet\nauthor-time 2000\nfilename f\n\tline two\n"
    );
    let m = parse_blame(&p);
    assert_eq!(m[&1].author, "Ada");
    assert_eq!(m[&1].time, Some(1000));
    assert_eq!(m[&1].commit.as_deref(), Some("aaaaaaaa"));
    assert_eq!(m[&2].author, "(uncommitted)");
    assert_eq!(m[&2].time, None);
}

// ---- integration: a real throwaway git repo with back-dated commits ----

const NOW: i64 = 1_790_000_000; // fixed clock for stable ages

fn git(dir: &Path, args: &[&str], author: &str, when: i64) {
    // isolate from the developer's/CI runner's git config (portable: an empty file, not /dev/null)
    let empty_cfg = std::env::temp_dir().join("todoage-empty-gitconfig");
    std::fs::write(&empty_cfg, "").unwrap();
    let st = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", author)
        .env(
            "GIT_AUTHOR_EMAIL",
            format!("{}@example.com", author.to_lowercase()),
        )
        .env("GIT_COMMITTER_NAME", author)
        .env("GIT_COMMITTER_EMAIL", "c@example.com")
        .env("GIT_AUTHOR_DATE", format!("@{when} +0000"))
        .env("GIT_COMMITTER_DATE", format!("@{when} +0000"))
        .env("GIT_CONFIG_GLOBAL", &empty_cfg)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git");
    assert!(
        st.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&st.stderr)
    );
}

fn fixture() -> tempdir::Dir {
    let d = tempdir::Dir::new();
    let p = d.path();
    git(p, &["init", "-q", "-b", "main"], "x", NOW);
    std::fs::write(
        p.join("a.rs"),
        "fn main() {\n    // TODO: handle errors\n}\n",
    )
    .unwrap();
    git(p, &["add", "."], "x", NOW);
    git(p, &["commit", "-q", "-m", "one"], "Ada", NOW - 400 * 86_400);
    std::fs::write(p.join("b.py"), "# FIXME: slow\nx = 1\n").unwrap();
    std::fs::write(p.join("ignored.txt"), "TODO: not tracked\n").unwrap();
    git(p, &["add", "b.py"], "x", NOW);
    git(
        p,
        &["commit", "-q", "-m", "two"],
        "Linus",
        NOW - 10 * 86_400,
    );
    // uncommitted edit
    std::fs::write(
        p.join("a.rs"),
        "fn main() {\n    // TODO: handle errors\n    // HACK: temporary\n}\n",
    )
    .unwrap();
    d
}

#[test]
fn scans_repo_with_blame() {
    let d = fixture();
    let mut items = scan_repo(d.path(), &tags(), &[], NOW).unwrap();
    items.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    assert_eq!(items.len(), 3, "{items:?}");
    assert_eq!(
        (items[0].author.as_str(), items[0].age_days),
        ("Ada", Some(400))
    );
    assert_eq!(
        (
            items[1].tag.as_str(),
            items[1].author.as_str(),
            items[1].age_days
        ),
        ("HACK", "(uncommitted)", None)
    );
    assert_eq!(
        (
            items[2].file.as_str(),
            items[2].author.as_str(),
            items[2].age_days
        ),
        ("b.py", "Linus", Some(10))
    );
}

#[test]
fn cli_filters_json_and_fail_gate() {
    let d = fixture();
    let bin = env!("CARGO_BIN_EXE_todoage");
    let run = |args: &[&str]| {
        Command::new(bin)
            .args(["-C", d.path().to_str().unwrap()])
            .args(args)
            .env("TODOAGE_NOW", NOW.to_string())
            .output()
            .unwrap()
    };

    let out = run(&[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1y 1mo"), "{text}");
    assert!(text.contains("3 items"), "{text}");
    // oldest first
    assert!(text.find("a.rs:2").unwrap() < text.find("b.py:1").unwrap());

    let out = run(&["--json", "--older-than", "30d"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["author"], "Ada");

    let out = run(&["--fail-older-than", "1y", "--summary"]);
    assert_eq!(out.status.code(), Some(1));
    let out = run(&["--fail-older-than", "2y"]);
    assert_eq!(out.status.code(), Some(0));
    let out = run(&["--bogus"]);
    assert_eq!(out.status.code(), Some(2));
}

mod tempdir {
    use std::path::{Path, PathBuf};
    pub struct Dir(PathBuf);
    impl Dir {
        pub fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static N: AtomicUsize = AtomicUsize::new(0);
            let p = std::env::temp_dir().join(format!(
                "todoage-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
