//! `.editorconfig` support with an allowlist (threat T13).
//!
//! Only seven keys are honoured (maintainer decision, H1.3c):
//! `indent_style`, `indent_size`, `tab_width`, `end_of_line`, `charset`,
//! `trim_trailing_whitespace`, `insert_final_newline`. Files are untrusted
//! input: each is read with a size limit, globs are compiled for the
//! linear-time `regex` engine with a size limit, and invalid values are
//! ignored. See <https://spec.editorconfig.org/>.

use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};

use crate::fsio;
use crate::text::Eol;

/// Largest `.editorconfig` file read, in bytes.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;
/// Most directories searched upwards.
pub const MAX_DIRECTORIES: usize = 64;
/// Longest glob pattern compiled.
pub const MAX_PATTERN_LEN: usize = 4096;
/// Largest indentation or tab width accepted.
pub const MAX_WIDTH: u8 = 32;

/// Indentation character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    /// Spaces.
    Space,
    /// Tabs.
    Tab,
}

/// Charset declared for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    /// ISO-8859-1 (decoded as windows-1252, its superset).
    Latin1,
    /// UTF-8 without a BOM requirement.
    Utf8,
    /// UTF-8 with a BOM.
    Utf8Bom,
    /// UTF-16 big endian.
    Utf16Be,
    /// UTF-16 little endian.
    Utf16Le,
}

impl Charset {
    /// Encoding label for [`crate::text::encoding_for_label`].
    pub fn label(self) -> &'static str {
        match self {
            Self::Latin1 => "windows-1252",
            Self::Utf8 | Self::Utf8Bom => "UTF-8",
            Self::Utf16Be => "UTF-16BE",
            Self::Utf16Le => "UTF-16LE",
        }
    }
}

/// The properties that apply to one file; `None` = not specified.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorConfig {
    /// Spaces or tabs.
    pub indent_style: Option<IndentStyle>,
    /// Columns per indentation level.
    pub indent_size: Option<u8>,
    /// Columns per tab.
    pub tab_width: Option<u8>,
    /// Line ending.
    pub end_of_line: Option<Eol>,
    /// Charset.
    pub charset: Option<Charset>,
    /// Remove spaces and tabs at the end of lines on save.
    pub trim_trailing_whitespace: Option<bool>,
    /// End the file with a line ending on save.
    pub insert_final_newline: Option<bool>,
}

/// Raw property values gathered for a file (`indent_size = tab` needs the
/// final `tab_width`, so values are interpreted at the end).
#[derive(Debug, Default)]
struct Raw {
    values: Vec<(String, String)>,
}

impl Raw {
    fn set(&mut self, key: &str, value: &str) {
        self.values.retain(|(k, _)| k != key);
        self.values.push((key.to_owned(), value.to_owned()));
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .filter(|v| *v != "unset")
    }
}

const ALLOWED: &[&str] = &[
    "indent_style",
    "indent_size",
    "tab_width",
    "end_of_line",
    "charset",
    "trim_trailing_whitespace",
    "insert_final_newline",
];

/// The properties for `file` (an absolute path) from the `.editorconfig`
/// files of its directory and its parents.
pub fn for_file(file: &Path) -> EditorConfig {
    let Some(start) = file.parent() else {
        return EditorConfig::default();
    };
    // Directories from the file's upwards, stopping at `root = true`.
    let mut configs = Vec::new();
    for dir in start.ancestors().take(MAX_DIRECTORIES) {
        let path = dir.join(".editorconfig");
        let Ok(read) = fsio::read_limited(&path, MAX_FILE_BYTES) else {
            continue;
        };
        let Ok(text) = String::from_utf8(read.bytes) else {
            continue;
        };
        let parsed = parse(&text);
        let root = parsed.root;
        configs.push((dir.to_path_buf(), parsed));
        if root {
            break;
        }
    }
    let mut raw = Raw::default();
    // Farthest first, so nearer files override.
    for (dir, config) in configs.iter().rev() {
        let Some(relative) = relative_path(file, dir) else {
            continue;
        };
        for section in &config.sections {
            if section.matches(&relative) {
                for (key, value) in &section.properties {
                    raw.set(key, value);
                }
            }
        }
    }
    interpret(&raw)
}

fn relative_path(file: &Path, dir: &Path) -> Option<String> {
    let relative = file.strip_prefix(dir).ok()?;
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(parts.join("/"))
}

fn interpret(raw: &Raw) -> EditorConfig {
    let width = |key: &str| {
        raw.get(key)
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|w| (1..=MAX_WIDTH).contains(w))
    };
    let flag = |key: &str| match raw.get(key) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    };
    let indent_style = match raw.get("indent_style") {
        Some("space") => Some(IndentStyle::Space),
        Some("tab") => Some(IndentStyle::Tab),
        _ => None,
    };
    let mut tab_width = width("tab_width");
    let mut indent_size = match raw.get("indent_size") {
        Some("tab") => tab_width,
        _ => width("indent_size"),
    };
    // Spec defaults between the two.
    if indent_size.is_none() && indent_style == Some(IndentStyle::Tab) {
        indent_size = tab_width;
    }
    if tab_width.is_none() && raw.get("indent_size") != Some("tab") {
        tab_width = indent_size;
    }
    EditorConfig {
        indent_style,
        indent_size,
        tab_width,
        end_of_line: match raw.get("end_of_line") {
            Some("lf") => Some(Eol::Lf),
            Some("crlf") => Some(Eol::CrLf),
            Some("cr") => Some(Eol::Cr),
            _ => None,
        },
        charset: match raw.get("charset") {
            Some("latin1") => Some(Charset::Latin1),
            Some("utf-8") => Some(Charset::Utf8),
            Some("utf-8-bom") => Some(Charset::Utf8Bom),
            Some("utf-16be") => Some(Charset::Utf16Be),
            Some("utf-16le") => Some(Charset::Utf16Le),
            _ => None,
        },
        trim_trailing_whitespace: flag("trim_trailing_whitespace"),
        insert_final_newline: flag("insert_final_newline"),
    }
}

/// One `[glob]` section.
#[derive(Debug)]
struct Section {
    matcher: Option<Matcher>,
    properties: Vec<(String, String)>,
}

impl Section {
    fn matches(&self, relative: &str) -> bool {
        self.matcher.as_ref().is_some_and(|m| m.matches(relative))
    }
}

#[derive(Debug)]
struct Parsed {
    root: bool,
    sections: Vec<Section>,
}

/// Parses an `.editorconfig` file. Unknown keys and malformed lines are
/// skipped; keys and values are lower-cased as the spec requires for the
/// supported properties.
fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed {
        root: false,
        sections: Vec::new(),
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(glob) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            parsed.sections.push(Section {
                matcher: Matcher::new(glob),
                properties: Vec::new(),
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        match parsed.sections.last_mut() {
            None if key == "root" => parsed.root = value == "true",
            Some(section) if ALLOWED.contains(&key.as_str()) => {
                section.properties.push((key, value));
            }
            _ => {}
        }
    }
    parsed
}

/// A compiled glob: a regex plus the numeric ranges of `{a..b}` groups.
#[derive(Debug)]
struct Matcher {
    regex: Regex,
    ranges: Vec<(i64, i64)>,
}

impl Matcher {
    fn new(glob: &str) -> Option<Self> {
        if glob.is_empty() || glob.len() > MAX_PATTERN_LEN {
            return None;
        }
        let mut ranges = Vec::new();
        let body = translate(glob, &mut ranges)?;
        // Without a slash the glob matches the file name in any directory;
        // with one (a leading one included) it is relative to the file's
        // own directory.
        let anchored = if glob.contains('/') {
            format!("^{body}$")
        } else {
            format!("^(?:.*/)?{body}$")
        };
        let regex = RegexBuilder::new(&anchored)
            .size_limit(1 << 20)
            .dfa_size_limit(1 << 20)
            .build()
            .ok()?;
        Some(Self { regex, ranges })
    }

    fn matches(&self, path: &str) -> bool {
        let Some(captures) = self.regex.captures(path) else {
            return false;
        };
        self.ranges.iter().enumerate().all(|(i, (low, high))| {
            captures
                .get(i + 1)
                .and_then(|m| m.as_str().parse::<i64>().ok())
                .is_some_and(|n| (*low..=*high).contains(&n))
        })
    }
}

/// EditorConfig glob → regex body. Numeric ranges become capture groups
/// (the only capturing groups), checked after matching.
fn translate(glob: &str, ranges: &mut Vec<(i64, i64)>) -> Option<String> {
    let chars: Vec<char> = glob.trim_start_matches('/').chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut depth = 0usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() => {
                out.push_str(&regex::escape(&chars[i + 1].to_string()));
                i += 1;
            }
            '*' if chars.get(i + 1) == Some(&'*') => {
                out.push_str(".*");
                i += 1;
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            '[' => match chars[i..].iter().position(|&c| c == ']') {
                Some(end) if end > 1 => {
                    let class: String = chars[i + 1..i + end].iter().collect();
                    let (negate, class) = match class.strip_prefix('!') {
                        Some(rest) => (true, rest.to_owned()),
                        None => (false, class),
                    };
                    let escaped: String = class
                        .chars()
                        .map(|c| match c {
                            '\\' | '[' | ']' | '^' | '&' | '~' => format!("\\{c}"),
                            _ => c.to_string(),
                        })
                        .collect();
                    out.push_str(&format!("[{}{escaped}]", if negate { "^" } else { "" }));
                    i += end;
                }
                _ => out.push_str("\\["),
            },
            '{' => {
                let close = matching_brace(&chars, i);
                match close {
                    Some(end) => {
                        let inner: String = chars[i + 1..end].iter().collect();
                        if let Some((low, high)) = numeric_range(&inner) {
                            ranges.push((low, high));
                            out.push_str("([+-]?[0-9]+)");
                            i = end;
                        } else if inner.contains(',') {
                            out.push_str("(?:");
                            depth += 1;
                        } else {
                            // `{single}` is literal, as in the spec.
                            out.push_str(&regex::escape(&format!("{{{inner}}}")));
                            i = end;
                        }
                    }
                    None => out.push_str("\\{"),
                }
            }
            ',' if depth > 0 => out.push('|'),
            '}' if depth > 0 => {
                out.push(')');
                depth -= 1;
            }
            _ => out.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    (depth == 0).then_some(out)
}

fn matching_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0;
    for (offset, &c) in chars[open..].iter().enumerate() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn numeric_range(inner: &str) -> Option<(i64, i64)> {
    let (low, high) = inner.split_once("..")?;
    let low: i64 = low.parse().ok()?;
    let high: i64 = high.parse().ok()?;
    Some((low.min(high), low.max(high)))
}

/// Applies the save-time properties to `text` (UTF-8, line endings as in
/// the editor). Returns `None` when nothing changes.
pub fn apply_on_save(text: &str, config: &EditorConfig, eol: Eol) -> Option<String> {
    let mut result = text.to_owned();
    if config.trim_trailing_whitespace == Some(true) {
        result = result
            .split('\n')
            .map(|line| {
                let (body, cr) = match line.strip_suffix('\r') {
                    Some(body) => (body, "\r"),
                    None => (line, ""),
                };
                format!("{}{cr}", body.trim_end_matches([' ', '\t']))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    if config.insert_final_newline == Some(true)
        && !result.is_empty()
        && !result.ends_with('\n')
        && !result.ends_with('\r')
    {
        result.push_str(match eol {
            Eol::Lf => "\n",
            Eol::CrLf => "\r\n",
            Eol::Cr => "\r",
        });
    }
    (result != text).then_some(result)
}

/// Absolute form of `path` for matching (relative paths are joined to the
/// current directory; symlinks are not resolved, as editors do).
pub fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}
