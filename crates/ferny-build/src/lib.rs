//! Build commands, process spawning and compiler error parsing (H2.4,
//! threats T1–T3, T7).
//!
//! A command template (Geany's syntax, e.g. `gcc -Wall -o "%e" "%f"`) is
//! split once into a program and arguments; there is never a shell.
//! Placeholders are substituted inside each argument and never split it.
//! Programs are resolved on `PATH` without the project's folder; output is
//! neutralised before it is shown.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The commands of a filetype, in menu order.
pub const COMMANDS: [&str; 4] = ["compile", "build", "lint", "execute"];
/// Largest `build.toml` read, in bytes.
pub const MAX_STORE_BYTES: u64 = 64 * 1024;
/// Longest template accepted, in bytes.
pub const MAX_TEMPLATE_BYTES: usize = 4096;
/// Output lines longer than this are not parsed for error locations.
pub const MAX_ERROR_LINE_BYTES: usize = 4096;

/// Why a template or a command cannot be used. No paths inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    /// Empty, too long, or with unbalanced quotes.
    #[error("invalid command")]
    Invalid,
    /// Shell syntax (pipes, redirections, `;`, `&&`, `$(`, backquotes)
    /// outside quotes: there is no shell.
    #[error("shell syntax is not supported; use sh -c \"…\" explicitly")]
    ShellSyntax,
    /// Unknown placeholder (`%x`).
    #[error("unknown placeholder")]
    Placeholder,
    /// The program was not found where it may be taken from.
    #[error("program not found")]
    NotFound,
}

/// Splits `template` into words: spaces separate, double and single
/// quotes group, a backslash escapes the next character outside single
/// quotes. Unquoted shell operators are refused.
pub fn split(template: &str) -> Result<Vec<String>, BuildError> {
    if template.trim().is_empty() || template.len() > MAX_TEMPLATE_BYTES {
        return Err(BuildError::Invalid);
    }
    if template.chars().any(|c| c.is_control() && c != '\t') {
        return Err(BuildError::Invalid);
    }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => word.push(chars.next().ok_or(BuildError::Invalid)?),
                        Some(other) => word.push(other),
                        None => return Err(BuildError::Invalid),
                    }
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(other) => word.push(other),
                        None => return Err(BuildError::Invalid),
                    }
                }
            }
            '\\' => {
                in_word = true;
                word.push(chars.next().ok_or(BuildError::Invalid)?);
            }
            '|' | ';' | '&' | '<' | '>' | '`' => return Err(BuildError::ShellSyntax),
            '$' if chars.peek() == Some(&'(') => return Err(BuildError::ShellSyntax),
            other => {
                in_word = true;
                word.push(other);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    if words.is_empty() {
        return Err(BuildError::Invalid);
    }
    Ok(words)
}

/// Values for the placeholders of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholders {
    /// `%f`: the file name (no folder).
    pub file: String,
    /// `%d`: the file's folder.
    pub dir: String,
    /// `%e`: the file name without its last extension.
    pub stem: String,
    /// `%l`: the cursor line.
    pub line: u32,
    /// `%p`: the project's base folder (empty without a project).
    pub project: String,
}

impl Placeholders {
    /// The placeholders of `path` with the cursor at `line`.
    pub fn for_file(path: &Path, line: u32) -> Self {
        let name = |p: Option<&std::ffi::OsStr>| p.map(|s| s.to_string_lossy().into_owned());
        Self {
            file: name(path.file_name()).unwrap_or_default(),
            dir: path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            stem: name(path.file_stem()).unwrap_or_default(),
            line,
            project: String::new(),
        }
    }
}

/// Replaces `%f %d %e %l %p %%` inside each word. Values never split a word,
/// so a file named `a b;rm -rf ~.c` stays one argument.
pub fn expand(words: &[String], values: &Placeholders) -> Result<Vec<String>, BuildError> {
    words
        .iter()
        .map(|word| {
            let mut out = String::with_capacity(word.len());
            let mut chars = word.chars();
            while let Some(c) = chars.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match chars.next() {
                    Some('f') => out.push_str(&values.file),
                    Some('d') => out.push_str(&values.dir),
                    Some('e') => out.push_str(&values.stem),
                    Some('l') => out.push_str(&values.line.to_string()),
                    Some('p') => out.push_str(&values.project),
                    Some('%') => out.push('%'),
                    _ => return Err(BuildError::Placeholder),
                }
            }
            Ok(out)
        })
        .collect()
}

/// Resolves `program` for a run in `work_dir` (the file's folder).
///
/// A bare name is looked up on `path_var`, skipping empty and relative
/// entries and any entry inside `excluded`, the trusted workspace root
/// (threat T2). A name with a separator is taken relative to `work_dir`
/// (the file's folder) only when `allow_local` (Execute running the program
/// just built); absolute names are used as they are. The result must be an
/// existing file.
pub fn resolve(
    program: &str,
    work_dir: &Path,
    excluded: &Path,
    path_var: &std::ffi::OsStr,
    allow_local: bool,
) -> Result<PathBuf, BuildError> {
    let candidate = Path::new(program);
    let found = if candidate.is_absolute() {
        Some(candidate.to_path_buf())
    } else if program.contains('/') || program.contains('\\') {
        allow_local.then(|| work_dir.join(candidate))
    } else {
        let excluded = std::fs::canonicalize(excluded).unwrap_or_else(|_| excluded.to_path_buf());
        std::env::split_paths(path_var)
            .filter(|dir| dir.is_absolute())
            .filter(|dir| {
                let real = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
                !real.starts_with(&excluded)
            })
            .flat_map(|dir| executable_names(program).map(move |n| dir.join(n)))
            .find(|p| p.is_file())
    };
    found
        .filter(|p| p.is_file())
        .and_then(|p| std::fs::canonicalize(p).ok())
        .ok_or(BuildError::NotFound)
}

fn executable_names(program: &str) -> impl Iterator<Item = String> + '_ {
    // Windows looks for the usual executable extensions too.
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".com", ".bat", ".cmd"]
    } else {
        &[""]
    };
    extensions.iter().map(move |e| format!("{program}{e}"))
}

/// Output made safe to show: escape sequences (CSI, OSC, others) removed,
/// other control, bidi and invisible formatting characters escaped,
/// invalid UTF-8 replaced (threat T7). Newlines and tabs are kept.
pub fn neutralise(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                // CSI: parameters and intermediates, then a final byte.
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC, DCS, APC, PM, SOS: up to BEL or ESC \.
                Some(']' | 'P' | '_' | '^' | 'X') => {
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' || (n == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' | '\t' => out.push(c),
            '\r' if chars.peek() == Some(&'\n') => {}
            c if c.is_control() || is_format(c) => {
                out.push_str(&format!("\\u{{{:04X}}}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out
}

fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// A file location found in an output line (`file:line[:col]`, also
/// rustc's `--> file:line:col`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The file as written (relative to the run's folder or absolute).
    pub file: String,
    /// 1-based line.
    pub line: u32,
    /// 1-based column, 0 if not given.
    pub column: u32,
}

/// The location an output line points to, if any.
pub fn error_location(line: &str) -> Option<Location> {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    if line.len() > MAX_ERROR_LINE_BYTES {
        return None;
    }
    let pattern = PATTERN.get_or_init(|| {
        regex::Regex::new(r"^\s*(?:-->\s*)?([^:\s][^:]*):(\d{1,9}):(?:(\d{1,9}):?)?")
            .unwrap_or_else(|err| unreachable!("built-in pattern: {err}"))
    });
    let caps = pattern.captures(line)?;
    let line_no = caps
        .get(2)?
        .as_str()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)?;
    Some(Location {
        file: caps.get(1)?.as_str().to_owned(),
        line: line_no,
        column: caps
            .get(3)
            .and_then(|c| c.as_str().parse().ok())
            .unwrap_or(0),
    })
}

/// The user's build commands, by filetype id and command (`build.toml`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserCommands {
    commands: BTreeMap<String, BTreeMap<String, String>>,
}

impl UserCommands {
    /// Reads `path`: a missing file is empty; every entry is validated and
    /// invalid ones are dropped and reported (as `filetype.command`).
    pub fn load(path: &Path) -> (Self, Vec<String>) {
        let Ok(meta) = std::fs::metadata(path) else {
            return (Self::default(), Vec::new());
        };
        if !meta.is_file() || meta.len() > MAX_STORE_BYTES {
            return (
                Self::default(),
                vec!["build.toml: unreadable or too large".into()],
            );
        }
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Self::parse(&text)
    }

    /// Parses the text of `build.toml`.
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let Ok(raw) = toml::from_str::<BTreeMap<String, BTreeMap<String, String>>>(text) else {
            return (
                Self::default(),
                vec!["build.toml: not valid TOML, ignored".into()],
            );
        };
        let mut store = Self::default();
        let mut warnings = Vec::new();
        for (filetype, commands) in raw {
            for (name, template) in commands {
                if store.set(&filetype, &name, &template).is_err() {
                    warnings.push(format!("build.toml: invalid {filetype}.{name}, ignored"));
                }
            }
        }
        (store, warnings)
    }

    /// Sets (or, with an empty template, removes) a command after
    /// validating it.
    pub fn set(&mut self, filetype: &str, command: &str, template: &str) -> Result<(), BuildError> {
        if !COMMANDS.contains(&command) || !valid_id(filetype) {
            return Err(BuildError::Invalid);
        }
        if template.trim().is_empty() {
            if let Some(map) = self.commands.get_mut(filetype) {
                map.remove(command);
                if map.is_empty() {
                    self.commands.remove(filetype);
                }
            }
            return Ok(());
        }
        expand(
            &split(template)?,
            &Placeholders::for_file(Path::new("/x/y.c"), 1),
        )?;
        self.commands
            .entry(filetype.to_owned())
            .or_default()
            .insert(command.to_owned(), template.to_owned());
        Ok(())
    }

    /// The user's template for `filetype` and `command`, if any.
    pub fn get(&self, filetype: &str, command: &str) -> Option<&str> {
        self.commands
            .get(filetype)?
            .get(command)
            .map(String::as_str)
    }

    /// The store as TOML.
    pub fn to_toml(&self) -> String {
        toml::to_string(&self.commands).unwrap_or_default()
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
