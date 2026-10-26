//! Search and replace in a document, and find in files (threat T6).
//!
//! Every search goes through the `regex` crate, whose engine runs in linear
//! time (no backtracking), with size limits; plain-text queries are escaped
//! into a regex. Find in files walks one directory tree without following
//! symlinks, reads only regular files through `fsio::read_limited`, skips
//! large and binary files and stops at a result limit or when cancelled.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use regex::{Regex, RegexBuilder};

use crate::fsio::{self, LARGE_DOCUMENT_BYTES};
use crate::glob::glob_match;
use crate::text;

/// Longest pattern accepted, in bytes.
pub const MAX_PATTERN_BYTES: usize = 4096;
/// Most find-in-files results (maintainer decision, H1.4).
pub const MAX_RESULTS: usize = 10_000;
/// Deepest directory level searched.
pub const MAX_DEPTH: usize = 64;
/// Longest line excerpt kept per result, in characters.
pub const MAX_EXCERPT_CHARS: usize = 500;

/// What to look for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The text or regular expression.
    pub pattern: String,
    /// Treat `pattern` as a regular expression.
    pub regex: bool,
    /// Distinguish upper and lower case.
    pub match_case: bool,
    /// Only whole words.
    pub whole_word: bool,
}

/// Why a search could not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    /// Empty or longer than [`MAX_PATTERN_BYTES`].
    #[error("empty or too long pattern")]
    BadLength,
    /// Not a valid regular expression, or too large once compiled.
    #[error("invalid pattern")]
    InvalidPattern,
}

impl Query {
    /// Compiles the query for the linear-time engine.
    pub fn compile(&self) -> Result<Regex, SearchError> {
        if self.pattern.is_empty() || self.pattern.len() > MAX_PATTERN_BYTES {
            return Err(SearchError::BadLength);
        }
        let body = if self.regex {
            self.pattern.clone()
        } else {
            regex::escape(&self.pattern)
        };
        let body = if self.whole_word {
            format!(r"\b(?:{body})\b")
        } else {
            body
        };
        RegexBuilder::new(&body)
            .case_insensitive(!self.match_case)
            .multi_line(true)
            .size_limit(10 << 20)
            .dfa_size_limit(10 << 20)
            .build()
            .map_err(|_| SearchError::InvalidPattern)
    }
}

/// Finds the next match after byte offset `from` (forward) or the last
/// one ending at or before `from` (backward), wrapping around once.
/// Returns a byte range. Empty matches are skipped.
pub fn find(
    text: &str,
    query: &Query,
    from: usize,
    forward: bool,
) -> Result<Option<(usize, usize)>, SearchError> {
    let regex = query.compile()?;
    let from = from.min(text.len());
    let mut matches = regex
        .find_iter(text)
        .filter(|m| !m.is_empty())
        .map(|m| (m.start(), m.end()));
    if forward {
        let mut first = None;
        for (start, end) in matches.by_ref() {
            if first.is_none() {
                first = Some((start, end));
            }
            if start >= from {
                return Ok(Some((start, end)));
            }
        }
        Ok(first)
    } else {
        let mut last_before = None;
        let mut last = None;
        for (start, end) in matches {
            if end <= from {
                last_before = Some((start, end));
            }
            last = Some((start, end));
        }
        Ok(last_before.or(last))
    }
}

/// The replacement for the match at `start..end` (regex queries expand
/// `$1`, `${name}`; plain queries insert `replacement` literally). `None`
/// if there is no match exactly there.
pub fn replacement_for(
    text: &str,
    query: &Query,
    start: usize,
    end: usize,
    replacement: &str,
) -> Result<Option<String>, SearchError> {
    let regex = query.compile()?;
    let Some(captures) = text
        .get(start..)
        .and_then(|_| regex.captures_at(text, start))
    else {
        return Ok(None);
    };
    let whole = captures.get(0).map(|m| (m.start(), m.end()));
    if whole != Some((start, end)) {
        return Ok(None);
    }
    if !query.regex {
        return Ok(Some(replacement.to_owned()));
    }
    let mut out = String::new();
    captures.expand(replacement, &mut out);
    Ok(Some(out))
}

/// Replaces every match; returns the new text and the count.
pub fn replace_all(
    text: &str,
    query: &Query,
    replacement: &str,
) -> Result<(String, usize), SearchError> {
    let regex = query.compile()?;
    let count = regex.find_iter(text).filter(|m| !m.is_empty()).count();
    if count == 0 {
        return Ok((text.to_owned(), 0));
    }
    let replaced = if query.regex {
        regex.replace_all(text, replacement).into_owned()
    } else {
        regex
            .replace_all(text, regex::NoExpand(replacement))
            .into_owned()
    };
    Ok((replaced, count))
}

/// One find-in-files result (1-based line and column, in characters).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The file.
    pub path: PathBuf,
    /// Line number.
    pub line: u32,
    /// Column of the match start.
    pub column: u32,
    /// The line, truncated to [`MAX_EXCERPT_CHARS`].
    pub excerpt: String,
}

/// Outcome of a find in files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSearch {
    /// Results, in walk order (directories sorted by name).
    pub hits: Vec<Hit>,
    /// Stopped at [`MAX_RESULTS`].
    pub truncated: bool,
    /// Stopped because it was cancelled.
    pub cancelled: bool,
    /// Files read and searched.
    pub files_searched: usize,
    /// Files skipped: too large, binary, unreadable, symlinks, special.
    pub files_skipped: usize,
}

/// Names of version-control directories never searched.
const SKIPPED_DIRS: &[&str] = &[".git", ".hg", ".svn", ".bzr", "CVS"];

/// Searches the files under `root` whose names match one of `patterns`
/// (Geany-style `*`/`?` globs; empty = all files).
pub fn find_in_files(
    root: &Path,
    query: &Query,
    patterns: &[String],
    cancel: &AtomicBool,
) -> Result<FileSearch, SearchError> {
    let regex = query.compile()?;
    let mut search = FileSearch::default();
    walk(root, 0, &regex, patterns, cancel, &mut search);
    Ok(search)
}

fn walk(
    dir: &Path,
    depth: usize,
    regex: &Regex,
    patterns: &[String],
    cancel: &AtomicBool,
    search: &mut FileSearch,
) {
    if depth > MAX_DEPTH || search.truncated || search.cancelled {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            search.cancelled = true;
            return;
        }
        if search.truncated {
            return;
        }
        // Symlinks are never followed (loops, escaping the folder).
        let Ok(kind) = entry.file_type() else {
            search.files_skipped += 1;
            continue;
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                walk(&path, depth + 1, regex, patterns, cancel, search);
            }
            continue;
        }
        if !kind.is_file() {
            search.files_skipped += 1;
            continue;
        }
        if !patterns.is_empty() && !patterns.iter().any(|p| glob_match(p, &name)) {
            continue;
        }
        search_file(&path, regex, search);
    }
}

fn search_file(path: &Path, regex: &Regex, search: &mut FileSearch) {
    let Ok(file) = fsio::read_limited(path, LARGE_DOCUMENT_BYTES) else {
        search.files_skipped += 1;
        return;
    };
    let Ok(decoded) = text::decode(&file.bytes, None) else {
        search.files_skipped += 1; // binary
        return;
    };
    search.files_searched += 1;
    for (index, line) in decoded.text.lines().enumerate() {
        if let Some(m) = regex.find_iter(line).find(|m| !m.is_empty()) {
            if search.hits.len() == MAX_RESULTS {
                search.truncated = true;
                return;
            }
            let column = line[..m.start()].chars().count() + 1;
            search.hits.push(Hit {
                path: path.to_path_buf(),
                line: u32::try_from(index + 1).unwrap_or(u32::MAX),
                column: u32::try_from(column).unwrap_or(u32::MAX),
                excerpt: line.chars().take(MAX_EXCERPT_CHARS).collect(),
            });
        }
    }
}
