# todoage

[![CI](https://github.com/celaldrg/todoage/actions/workflows/ci.yml/badge.svg)](https://github.com/celaldrg/todoage/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**How old are your TODOs?** `todoage` finds `TODO`, `FIXME`, `HACK` and `XXX` comments in a git
repository and uses `git blame` to show **who wrote each one and how long ago**, oldest first.

`grep TODO` tells you that technical debt exists. `todoage` tells you which items have been "temporary" for eight years,
who to ask about them, and lets CI fail when new ones are left to rot.

## Example

Real output on [ripgrep](https://github.com/BurntSushi/ripgrep) (commit `6892bc2`, 26 October 2026):

```text
$ todoage
AGE      AUTHOR         LOCATION                                 TAG   TEXT
8y 6mo   Andrew Gallant crates/searcher/src/sink.rs:607          TODO  In theory, it should be possible to amortize
8y 2mo   Andrew Gallant tests/regression.rs:179                  TODO  (burntsushi) Darwin doesn't like this test for some reason. Probably
8y 2mo   Andrew Gallant tests/regression.rs:192                  TODO  (burntsushi) Figure out how to make this test work on Windows. Right
7y 9mo   David Torosyan crates/ignore/src/gitignore.rs:556       TODO  This should not return a `Result`. Fix this in the next semver
7y 9mo   David Torosyan crates/ignore/src/overrides.rs:157       TODO  This should not return a `Result`. Fix this in the next semver
3y 4mo   Andrew Gallant crates/regex/src/matcher.rs:637          FIXME Re-enable this test once inner literal extraction works.
3y 4mo   Andrew Gallant crates/regex/src/non_matching.rs:31      FIXME This is wrong, but not doing this leads to incorrect
3y 4mo   Andrew Gallant crates/regex/src/non_matching.rs:150     FIXME The first four tests below should correspond to a full set
3y       Andrew Gallant crates/globset/src/lib.rs:956            TODO  In the next semver breaking release, we should drop this pool and
2y 11mo  Andrew Gallant crates/searcher/src/searcher/core.rs:683 FIXME This works around a bug in grep-regex where it does

10 items · FIXME 4, TODO 6
top authors: Andrew Gallant 8, David Torosyan 2
oldest: 8y 6mo · median age: 7y 9mo
```

The whole scan, including blame, took about 0.3 s.

Filter by author and path:

```text
$ todoage --author torosyan crates/ignore
AGE      AUTHOR         LOCATION                           TAG   TEXT
7y 9mo   David Torosyan crates/ignore/src/gitignore.rs:556 TODO  This should not return a `Result`. Fix this in the next semver
7y 9mo   David Torosyan crates/ignore/src/overrides.rs:157 TODO  This should not return a `Result`. Fix this in the next semver

2 items · TODO 2
top authors: David Torosyan 2
oldest: 7y 9mo · median age: 7y 9mo
```

Use it as a CI gate:

```text
$ todoage --fail-older-than 5y --summary
10 items · FIXME 4, TODO 6
top authors: Andrew Gallant 8, David Torosyan 2
oldest: 8y 6mo · median age: 7y 9mo
todoage: 5 item(s) older than the --fail-older-than limit
$ echo $?
1
```

Or get JSON for dashboards and scripts:

```text
$ todoage --sort file --json --limit 1
[
  {
    "file": "crates/globset/src/lib.rs",
    "line": 956,
    "tag": "TODO",
    "text": "In the next semver breaking release, we should drop this pool and",
    "author": "Andrew Gallant",
    "timestamp": 1695921597,
    "age_days": 1123,
    "commit": "be9e3089"
  }
]
```

## Install

```sh
cargo install --git https://github.com/celaldrg/todoage
```

Prebuilt binaries for Linux, macOS (Intel and Apple Silicon) and Windows are attached to each
[release](https://github.com/celaldrg/todoage/releases). The only runtime requirement is `git` on your `PATH`.

## Usage

```text
todoage [options] [path...]

  -C <dir>                 repository to scan (default: current directory)
  --tags <A,B>             markers to look for (default: TODO,FIXME,HACK,XXX)
  --older-than <age>       only show items older than this (e.g. 90d, 3w, 6mo, 1y)
  --author <text>          only items whose author contains this text (case-insensitive)
  --sort <key>             age (default, oldest first), file, author
  --limit <n>              show at most n rows (the summary still covers everything)
  --json                   JSON output
  --summary                only print the summary
  --fail-older-than <age>  exit 1 if any item is older than this (for CI)
```

Exit codes: `0` success, `1` the `--fail-older-than` gate was hit, `2` usage error or not a git repository.

### What counts as a marker

The tag must be a whole, upper-case word followed by `:`, whitespace, an owner in parentheses, or the end of the line.
That catches the usual comment styles in every language (`// TODO: x`, `# FIXME x`, `/* HACK(alice): x */`,
`<!-- XXX -->`, `-- TODO x`) and skips things like `"TODO"` string literals, `TODOS` or `todo_list`.
Use `--tags NOTE,OPTIMIZE` to look for your own markers.

### How it works

1. `git ls-files` lists tracked files, so `.gitignore`d, vendored-but-ignored and build output are skipped automatically.
   Binary files and files over 2 MB are skipped.
2. Each file is scanned for markers. Only files with at least one match are blamed, with
   `git blame --line-porcelain -w` (whitespace-only changes do not reset a TODO's age).
3. Lines you have not committed yet show up with author `(uncommitted)` and age `new`.

## GitHub Actions

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0   # blame needs history
- run: cargo install --git https://github.com/celaldrg/todoage --locked
- run: todoage --fail-older-than 1y
```

## Development

```sh
cargo test     # unit tests plus integration tests that build throwaway git repos with back-dated commits
cargo clippy --all-targets -- -D warnings
```

## License

[MIT](LICENSE) © Celalettin Durğaç
