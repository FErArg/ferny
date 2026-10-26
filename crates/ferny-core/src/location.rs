//! Files to open with an optional cursor position, as given on the command
//! line (`file`, `file:line`, `file:line:col`, `+line file`), forwarded by
//! another instance or restored from the session.

use std::path::{Path, PathBuf};

/// A file to open and where to put the cursor (1-based; `None` keeps the
/// default position).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The file. Absolute after [`parse_arguments`].
    pub path: PathBuf,
    /// Line, starting at 1.
    pub line: Option<u32>,
    /// Column, starting at 1.
    pub column: Option<u32>,
}

impl Location {
    /// A location without a cursor position.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            line: None,
            column: None,
        }
    }
}

/// Interprets command-line arguments relative to `cwd`.
///
/// `+N` sets the line of the next file (Geany syntax). A trailing `:N` or
/// `:N:M` is a position, unless a file with the literal name exists. Line
/// and column numbers must be positive and fit in 32 bits; otherwise the
/// argument is taken literally.
pub fn parse_arguments(args: &[String], cwd: &Path) -> Vec<Location> {
    let mut locations = Vec::new();
    let mut next_line: Option<u32> = None;
    for arg in args {
        if let Some(number) = arg.strip_prefix('+')
            && let Some(line) = positive(number)
        {
            next_line = Some(line);
            continue;
        }
        let mut location = split_position(arg, cwd);
        if location.line.is_none() {
            location.line = next_line.take();
        } else {
            next_line = None;
        }
        locations.push(location);
    }
    locations
}

fn split_position(arg: &str, cwd: &Path) -> Location {
    let literal = cwd.join(arg);
    if literal.exists() {
        return Location::new(literal);
    }
    // file:line:col, then file:line.
    let mut parts = arg.rsplitn(3, ':');
    let last = parts.next();
    let middle = parts.next();
    let rest = parts.next();
    if let (Some(column), Some(line), Some(path)) = (last, middle, rest)
        && let (Some(line), Some(column)) = (positive(line), positive(column))
        && !path.is_empty()
    {
        return Location {
            path: cwd.join(path),
            line: Some(line),
            column: Some(column),
        };
    }
    if let Some((path, line)) = arg.rsplit_once(':')
        && let Some(line) = positive(line)
        && !path.is_empty()
    {
        return Location {
            path: cwd.join(path),
            line: Some(line),
            column: None,
        };
    }
    Location::new(literal)
}

fn positive(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok().filter(|&n| n > 0)
}
