//! Git, read-only (H4.2d, threats T1, T3, T6, T7): the arguments Ferny
//! passes to the `git` program and parsers for its answers. Every call
//! switches off what a repository's configuration could run on a read
//! (fsmonitor, hooks, external diff, textconv); the caller runs git only in
//! trusted folders, without a shell.

use crate::browse::display_name;

/// Most status entries kept.
pub const MAX_STATUS: usize = 20_000;
/// Most blame lines kept.
pub const MAX_BLAME_LINES: usize = 100_000;

/// Options in front of every git command.
fn safe(command: &[&str]) -> Vec<String> {
    let mut args: Vec<String> = [
        "--no-optional-locks",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.hooksPath=",
        "-c",
        "diff.external=",
        "-c",
        "core.pager=cat",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    args.extend(command.iter().map(|s| (*s).to_owned()));
    args
}

/// `rev-parse --show-toplevel`.
pub fn toplevel_args() -> Vec<String> {
    safe(&["rev-parse", "--show-toplevel"])
}

/// `status --porcelain=v1 -z` of the whole working tree.
pub fn status_args() -> Vec<String> {
    safe(&["status", "--porcelain=v1", "-z", "--untracked-files=normal"])
}

/// `blame --porcelain` of `file` (a name in the working folder; after
/// `--`, so it is never an option).
pub fn blame_args(file: &str) -> Vec<String> {
    safe(&["blame", "--porcelain", "--no-textconv", "--", file])
}

/// One status entry: the path relative to the repository root and a
/// letter: `M` modified, `A` added, `D` deleted, `R` renamed, `U`
/// conflict, `?` untracked.
pub type StatusEntry = (String, char);

/// Parses `status --porcelain=v1 -z`.
pub fn parse_status(output: &[u8]) -> Vec<StatusEntry> {
    let mut out = Vec::new();
    let mut fields = output.split(|b| *b == 0);
    while let Some(field) = fields.next() {
        let code = |c: u8| b" MTADRCU?!".contains(&c);
        if field.len() < 4 || field[2] != b' ' || !code(field[0]) || !code(field[1]) {
            continue;
        }
        if out.len() >= MAX_STATUS {
            break;
        }
        let (x, y) = (field[0], field[1]);
        let path = String::from_utf8_lossy(&field[3..]).into_owned();
        // Renames and copies carry the old path as the next field.
        if matches!(x, b'R' | b'C') {
            fields.next();
        }
        let letter = match (x, y) {
            (b'?', b'?') => '?',
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => 'U',
            (b'R', _) => 'R',
            (b'A', _) => 'A',
            (b'D', _) | (_, b'D') => 'D',
            _ => 'M',
        };
        out.push((path, letter));
    }
    out
}

/// One line of a blame: final line number (1-based), short commit,
/// author, date (`YYYY-MM-DD`, UTC) and summary, neutralised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    /// 1-based line in the current file.
    pub line: u32,
    /// First 8 characters of the commit.
    pub commit: String,
    /// Author name.
    pub author: String,
    /// Commit date of the author, `YYYY-MM-DD`.
    pub date: String,
    /// First line of the commit message.
    pub summary: String,
}

/// Parses `blame --porcelain`: a header per line, commit details the first
/// time a commit appears, then the line's text (ignored).
pub fn parse_blame(output: &[u8]) -> Vec<BlameLine> {
    #[derive(Clone, Default)]
    struct Commit {
        author: String,
        date: String,
        summary: String,
    }
    let text = String::from_utf8_lossy(output);
    let mut commits: std::collections::HashMap<String, Commit> = std::collections::HashMap::new();
    let mut out = Vec::new();
    let mut current: Option<(String, u32)> = None;
    for line in text.lines() {
        if out.len() >= MAX_BLAME_LINES {
            break;
        }
        if line.starts_with('\t') {
            if let Some((sha, number)) = current.take() {
                let commit = commits.get(&sha).cloned().unwrap_or_default();
                out.push(BlameLine {
                    line: number,
                    commit: sha.chars().take(8).collect(),
                    author: commit.author,
                    date: commit.date,
                    summary: commit.summary,
                });
            }
            continue;
        }
        let mut words = line.splitn(2, ' ');
        let key = words.next().unwrap_or("");
        let rest = words.next().unwrap_or("");
        if key.len() == 40 && key.chars().all(|c| c.is_ascii_hexdigit()) {
            let number = rest
                .split(' ')
                .nth(1)
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            current = Some((key.to_owned(), number));
            commits.entry(key.to_owned()).or_default();
            continue;
        }
        let Some((sha, _)) = &current else {
            continue;
        };
        let commit = commits.entry(sha.clone()).or_default();
        let shown = |s: &str| display_name(&s.chars().take(200).collect::<String>());
        match key {
            "author" => commit.author = shown(rest),
            "summary" => commit.summary = shown(rest),
            "author-time" => {
                commit.date = rest
                    .parse::<u64>()
                    .map(|t| crate::log::timestamp(t)[..10].to_owned())
                    .unwrap_or_default();
            }
            _ => {}
        }
    }
    out
}
