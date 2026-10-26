//! File templates and snippets in Geany's formats (H2.5b).
//!
//! Built-in data: Geany 2.1.0's `templates/` and `snippets.conf` (copied to
//! `data/` by `tools/geany-import`). The user's own `templates/files/*` and
//! `snippets.conf` in Ferny's configuration directory add to or replace
//! them, as in Geany. Wildcards are plain text substitutions; Geany's
//! `{command:…}` (which runs a shell command) is not supported and stays as
//! written.

use std::collections::BTreeMap;
use std::path::Path;

use ferny_core::fsio;

use crate::geany::KeyFile;

/// Largest user template or snippets file read, in bytes.
pub const MAX_TEMPLATE_BYTES: u64 = 64 * 1024;
/// Most user file templates read.
pub const MAX_TEMPLATES: usize = 200;

const BUILTIN_FILES: [(&str, &str); 14] = [
    (
        "file.html",
        include_str!("../../../data/templates/files/file.html"),
    ),
    (
        "file.php",
        include_str!("../../../data/templates/files/file.php"),
    ),
    (
        "file.rb",
        include_str!("../../../data/templates/files/file.rb"),
    ),
    (
        "file.tex",
        include_str!("../../../data/templates/files/file.tex"),
    ),
    (
        "file_html5.html",
        include_str!("../../../data/templates/files/file_html5.html"),
    ),
    (
        "main.c",
        include_str!("../../../data/templates/files/main.c"),
    ),
    (
        "main.cxx",
        include_str!("../../../data/templates/files/main.cxx"),
    ),
    (
        "main.d",
        include_str!("../../../data/templates/files/main.d"),
    ),
    (
        "main.java",
        include_str!("../../../data/templates/files/main.java"),
    ),
    (
        "main.md",
        include_str!("../../../data/templates/files/main.md"),
    ),
    (
        "main.py",
        include_str!("../../../data/templates/files/main.py"),
    ),
    (
        "main.vala",
        include_str!("../../../data/templates/files/main.vala"),
    ),
    (
        "module.erl",
        include_str!("../../../data/templates/files/module.erl"),
    ),
    (
        "program.pas",
        include_str!("../../../data/templates/files/program.pas"),
    ),
];
const BUILTIN_PARTS: [(&str, &str); 5] = [
    ("bsd", include_str!("../../../data/templates/bsd")),
    (
        "changelog",
        include_str!("../../../data/templates/changelog"),
    ),
    (
        "fileheader",
        include_str!("../../../data/templates/fileheader"),
    ),
    ("function", include_str!("../../../data/templates/function")),
    ("gpl", include_str!("../../../data/templates/gpl")),
];
const BUILTIN_SNIPPETS: &str = include_str!("../../../data/snippets.conf");

/// Values of the wildcards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    /// `{developer}`.
    pub developer: String,
    /// `{initial}`.
    pub initials: String,
    /// `{mail}`.
    pub mail: String,
    /// `{company}`.
    pub company: String,
    /// `{version}`.
    pub version: String,
    /// `{year}`, `{date}`, `{datetime}` (formatted by the caller).
    pub year: String,
    /// See `year`.
    pub date: String,
    /// See `year`.
    pub datetime: String,
    /// `{filename}` (also `{untitled}`).
    pub filename: String,
    /// `{project}`.
    pub project: String,
}

/// Comment tokens of the target filetype, for `{fileheader}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comment {
    /// Block opener (`/*`); empty if none.
    pub open: String,
    /// Block closer (`*/`).
    pub close: String,
    /// Line comment (`#`); used when there is no block comment.
    pub single: String,
}

fn read_user(path: &Path) -> Option<String> {
    // Regular files only; links are not followed.
    if !std::fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let file = fsio::read_limited(path, MAX_TEMPLATE_BYTES).ok()?;
    String::from_utf8(file.bytes).ok()
}

/// File templates by name: the built-in ones, then the user's
/// `templates/files/*` (same name replaces), sorted by name.
pub fn file_templates(config_dir: Option<&Path>) -> Vec<(String, String)> {
    let mut all: BTreeMap<String, String> = BUILTIN_FILES
        .iter()
        .map(|(n, t)| ((*n).to_owned(), (*t).to_owned()))
        .collect();
    if let Some(dir) = config_dir
        && let Ok(entries) = std::fs::read_dir(dir.join("templates/files"))
    {
        for entry in entries.filter_map(Result::ok).take(MAX_TEMPLATES) {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if let Some(text) = read_user(&entry.path()) {
                all.insert(name, text);
            }
        }
    }
    all.into_iter().collect()
}

fn part(config_dir: Option<&Path>, name: &str) -> String {
    config_dir
        .and_then(|dir| read_user(&dir.join("templates").join(name)))
        .or_else(|| {
            BUILTIN_PARTS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, t)| (*t).to_owned())
        })
        .unwrap_or_default()
}

/// Replaces the simple wildcards (`{developer}`, `{year}`, …). Unknown
/// wildcards and `{command:…}` stay as written.
pub fn expand_wildcards(text: &str, values: &Values) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        let value = match name {
            "developer" => Some(&values.developer),
            "initial" => Some(&values.initials),
            "mail" => Some(&values.mail),
            "company" => Some(&values.company),
            "version" => Some(&values.version),
            "year" => Some(&values.year),
            "date" => Some(&values.date),
            "datetime" => Some(&values.datetime),
            "filename" | "untitled" => Some(&values.filename),
            "project" => Some(&values.project),
            "ob" => {
                out.push('{');
                rest = &after[end + 1..];
                continue;
            }
            "cb" => {
                out.push('}');
                rest = &after[end + 1..];
                continue;
            }
            _ => None,
        };
        match value {
            Some(v) => out.push_str(v),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn commented(text: &str, comment: &Comment) -> String {
    let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
    if !comment.open.is_empty() && !comment.close.is_empty() {
        // As Geany: " * " lines between the block's opener and closer.
        let mut out = format!("{}\n", comment.open);
        for line in lines {
            out.push_str(format!(" * {line}").trim_end());
            out.push('\n');
        }
        out.push_str(&format!(" {}\n", comment.close));
        out
    } else if !comment.single.is_empty() {
        let token = comment.single.trim_end();
        lines
            .iter()
            .map(|line| format!("{token} {line}").trim_end().to_owned() + "\n")
            .collect()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

/// Expands a template: `{fileheader}` (commented for the filetype),
/// `{gpl}`, `{bsd}` and the simple wildcards.
pub fn expand(text: &str, values: &Values, comment: &Comment, config_dir: Option<&Path>) -> String {
    let licence = |name: &str| expand_wildcards(&part(config_dir, name), values);
    let header = || {
        let raw = part(config_dir, "fileheader")
            .replace("{gpl}", &licence("gpl"))
            .replace("{bsd}", &licence("bsd"));
        commented(&expand_wildcards(&raw, values), comment)
    };
    let mut out = text.to_owned();
    if out.contains("{fileheader}") {
        out = out.replace("{fileheader}", header().trim_end_matches('\n'));
    }
    if out.contains("{gpl}") {
        out = out.replace("{gpl}", &commented(&licence("gpl"), comment));
    }
    if out.contains("{bsd}") {
        out = out.replace("{bsd}", &commented(&licence("bsd"), comment));
    }
    expand_wildcards(&out, values)
}

/// Snippets by filetype name (Geany's group names) and keyword.
#[derive(Debug, Default)]
pub struct Snippets {
    file: KeyFile,
}

impl Snippets {
    /// The user's `snippets.conf` if there is one (it replaces Geany's, as
    /// in Geany), else the built-in file.
    pub fn load(config_dir: Option<&Path>) -> Self {
        let text = config_dir
            .and_then(|dir| read_user(&dir.join("snippets.conf")))
            .unwrap_or_else(|| BUILTIN_SNIPPETS.to_owned());
        Self::parse(&text)
    }

    /// Parses the text of a `snippets.conf`.
    pub fn parse(text: &str) -> Self {
        Self {
            file: KeyFile::parse(text),
        }
    }

    /// Expands `keyword` for the filetype named `filetype` (e.g. `C++`):
    /// the text to insert and the character offset of the first
    /// `%cursor%`. Line breaks get `eol` plus the current line's `indent`,
    /// `\t` one indentation step (`unit`).
    pub fn expand(
        &self,
        filetype: &str,
        keyword: &str,
        indent: &str,
        unit: &str,
        eol: &str,
        values: &Values,
    ) -> Option<(String, Option<usize>)> {
        if keyword.is_empty() || filetype == "Special" || filetype == "Keybindings" {
            return None;
        }
        let raw = self
            .file
            .get(filetype, keyword)
            .or_else(|| self.file.get("Default", keyword))?;
        // Special keys (%brace_open% …), not nested, as Geany.
        let mut text = raw.to_owned();
        if let Some((_, _, specials)) = self.file.group("Special") {
            for (key, value) in specials {
                text = text.replace(&format!("%{key}%"), value);
            }
        }
        let newline = format!("{eol}{indent}");
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push_str(&newline),
                Some('t') => out.push_str(unit),
                Some('s') => out.push(' '),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }
        let out = out.replace("%newline%", &newline).replace("%ws%", unit);
        let out = expand_wildcards(&out, values);
        let cursor = out.find("%cursor%").map(|byte| out[..byte].chars().count());
        Some((out.replace("%cursor%", ""), cursor))
    }
}
