use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};
use todoage::{human_age, parse_days, scan_repo, Item, DEFAULT_TAGS};

const HELP: &str =
    "todoage - find TODO/FIXME/HACK comments and see who wrote them and how old they are

Usage: todoage [options] [path...]

Options:
  -C <dir>             repository to scan (default: current directory)
  --tags <A,B>         markers to look for (default: TODO,FIXME,HACK,XXX)
  --older-than <age>   only show items older than this (e.g. 90d, 6mo, 1y)
  --author <text>      only items whose author contains this text
  --sort <key>         age (default, oldest first), file, author
  --limit <n>          show at most n items
  --json               JSON output
  --summary            only print the summary
  --fail-older-than <age>  exit 1 if any item is older than this (for CI)
  -h, --help           help
  -V, --version        version";

struct Opts {
    repo: PathBuf,
    tags: Vec<String>,
    older: Option<i64>,
    author: Option<String>,
    sort: String,
    limit: Option<usize>,
    json: bool,
    summary: bool,
    fail_older: Option<i64>,
    paths: Vec<String>,
}

fn parse_args(args: Vec<String>) -> Result<Opts, String> {
    let mut o = Opts {
        repo: PathBuf::from("."),
        tags: DEFAULT_TAGS.iter().map(|s| s.to_string()).collect(),
        older: None,
        author: None,
        sort: "age".into(),
        limit: None,
        json: false,
        summary: false,
        fail_older: None,
        paths: vec![],
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "-V" | "--version" => {
                println!("todoage {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-C" => o.repo = PathBuf::from(val("-C")?),
            "--tags" => {
                o.tags = val("--tags")?
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--older-than" => {
                o.older = Some(
                    parse_days(&val("--older-than")?)
                        .ok_or("bad --older-than (use e.g. 90d, 6mo, 1y)")?,
                )
            }
            "--fail-older-than" => {
                o.fail_older =
                    Some(parse_days(&val("--fail-older-than")?).ok_or("bad --fail-older-than")?)
            }
            "--author" => o.author = Some(val("--author")?.to_lowercase()),
            "--sort" => o.sort = val("--sort")?,
            "--limit" => o.limit = Some(val("--limit")?.parse().map_err(|_| "bad --limit")?),
            "--json" => o.json = true,
            "--summary" => o.summary = true,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => o.paths.push(a),
        }
    }
    if !["age", "file", "author"].contains(&o.sort.as_str()) {
        return Err("--sort must be age, file or author".into());
    }
    Ok(o)
}

fn main() -> ExitCode {
    let o = match parse_args(std::env::args().skip(1).collect()) {
        Ok(o) => o,
        Err(e) if e.is_empty() => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("todoage: {e}\n\n{HELP}");
            return ExitCode::from(2);
        }
    };
    let now = std::env::var("TODOAGE_NOW")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });
    let mut items = match scan_repo(&o.repo, &o.tags, &o.paths, now) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("todoage: {e}");
            return ExitCode::from(2);
        }
    };
    if let Some(min) = o.older {
        items.retain(|i| i.age_days.is_some_and(|d| d > min));
    }
    if let Some(a) = &o.author {
        items.retain(|i| i.author.to_lowercase().contains(a));
    }
    match o.sort.as_str() {
        "file" => items.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line))),
        "author" => items.sort_by(|a, b| (&a.author, a.timestamp).cmp(&(&b.author, b.timestamp))),
        _ => items.sort_by_key(|i| (i.timestamp.unwrap_or(i64::MAX), i.file.clone(), i.line)),
    }
    let failing = o
        .fail_older
        .map(|max| {
            items
                .iter()
                .filter(|i| i.age_days.is_some_and(|d| d > max))
                .count()
        })
        .unwrap_or(0);
    let all = items.clone();
    if let Some(n) = o.limit {
        items.truncate(n);
    }

    if o.json {
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        if !o.summary {
            print_table(&items);
        }
        print_summary(&all, items.len());
    }
    if failing > 0 {
        eprintln!("todoage: {failing} item(s) older than the --fail-older-than limit");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn print_table(items: &[Item]) {
    if items.is_empty() {
        return;
    }
    let aw = items
        .iter()
        .map(|i| i.author.chars().count())
        .max()
        .unwrap_or(6)
        .clamp(6, 24);
    let lw = items
        .iter()
        .map(|i| format!("{}:{}", i.file, i.line).chars().count())
        .max()
        .unwrap_or(8)
        .min(48);
    println!(
        "{:<8} {:<aw$} {:<lw$} {:<5} TEXT",
        "AGE", "AUTHOR", "LOCATION", "TAG"
    );
    for i in items {
        let author: String = i.author.chars().take(aw).collect();
        let loc = format!("{}:{}", i.file, i.line);
        println!(
            "{:<8} {:<aw$} {:<lw$} {:<5} {}",
            human_age(i.age_days),
            author,
            loc,
            i.tag,
            i.text
        );
    }
    println!();
}

/// Summary over every matching item; `shown` is how many rows the table printed.
fn print_summary(items: &[Item], shown: usize) {
    let total = items.len();
    if total == 0 {
        println!("No TODO-style comments found.");
        return;
    }
    let mut by_tag: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_author: BTreeMap<&str, usize> = BTreeMap::new();
    for i in items {
        *by_tag.entry(&i.tag).or_default() += 1;
        *by_author.entry(&i.author).or_default() += 1;
    }
    let tags = by_tag
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut authors: Vec<_> = by_author.into_iter().collect();
    authors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let authors = authors
        .iter()
        .take(5)
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ");
    let ages: Vec<i64> = items.iter().filter_map(|i| i.age_days).collect();
    let shown = if shown < total {
        format!(" (showing {shown})")
    } else {
        String::new()
    };
    println!("{total} items{shown} · {tags}");
    println!("top authors: {authors}");
    if !ages.is_empty() {
        let mut s = ages.clone();
        s.sort();
        println!(
            "oldest: {} · median age: {}",
            human_age(s.last().copied()),
            human_age(Some(s[s.len() / 2]))
        );
    }
}
