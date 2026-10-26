//! User filetypes and colour schemes (H1.5b), loaded on top of the built-in
//! catalog from Ferny's configuration directory:
//!
//! - `filetypes/<id>.toml`: [`UserFiletype`] — overrides a built-in
//!   filetype, or defines a new one based on a built-in (`base`), which
//!   provides the lexer and its styles. No new lexers, nothing executable.
//! - `colorschemes/<id>.toml`: same format as the built-in schemes.
//!
//! Both directories are untrusted input: regular files only (symlinks are
//! not followed), at most [`MAX_FILES`] files of [`MAX_FILE_BYTES`] each,
//! every value validated; invalid files are skipped with a warning.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Catalog, ColorScheme, Filetype};

/// Largest user file read, in bytes.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Most files read per directory.
pub const MAX_FILES: usize = 500;
/// Longest keyword list, in bytes.
pub const MAX_WORDS_BYTES: usize = 64 * 1024;
/// Longest style value or setting, in bytes.
pub const MAX_VALUE_BYTES: usize = 256;
/// Most file-name patterns per filetype.
pub const MAX_PATTERNS: usize = 64;

/// Settings a user filetype may set (no commands, no paths).
pub const USER_SETTINGS: &[&str] = &[
    "extension",
    "mime_type",
    "comment_single",
    "comment_open",
    "comment_close",
    "comment_use_indent",
    "wordchars",
];

/// A keyword list given by name (`primary`, `secondary`, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedWords {
    /// Geany's name for the set.
    pub key: String,
    /// Space-separated words.
    pub words: String,
}

/// A style value given by name (`comment`, `word`, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedValue {
    /// Geany's name for the style.
    pub key: String,
    /// Geany style value.
    pub value: String,
}

/// A user filetype file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserFiletype {
    /// Id (`[a-z0-9_]`); a built-in id overrides that filetype.
    pub id: String,
    /// Built-in filetype providing the lexer, for new filetypes.
    #[serde(default)]
    pub base: Option<String>,
    /// Display name (new filetypes).
    #[serde(default)]
    pub name: Option<String>,
    /// File-name patterns; replace the base's when present.
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    /// Allowlisted settings.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// Keyword lists to replace.
    #[serde(default)]
    pub keywords: Vec<NamedWords>,
    /// Style values to replace.
    #[serde(default)]
    pub styles: Vec<NamedValue>,
}

/// Whether `id` is a valid filetype or scheme id.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn plain(text: &str, max: usize) -> bool {
    text.len() <= max && !text.chars().any(|c| c.is_control() && c != '\t')
}

impl UserFiletype {
    /// Checks every field; `false` means the file is skipped.
    pub fn is_valid(&self) -> bool {
        valid_id(&self.id)
            && self.base.as_deref().is_none_or(valid_id)
            && self
                .name
                .as_deref()
                .is_none_or(|n| !n.is_empty() && plain(n, 64))
            && self.extensions.as_ref().is_none_or(|e| {
                e.len() <= MAX_PATTERNS && e.iter().all(|p| !p.is_empty() && plain(p, 128))
            })
            && self
                .settings
                .iter()
                .all(|(k, v)| USER_SETTINGS.contains(&k.as_str()) && plain(v, MAX_VALUE_BYTES))
            && self
                .keywords
                .iter()
                .all(|k| plain(&k.key, 64) && plain(&k.words, MAX_WORDS_BYTES))
            && self
                .styles
                .iter()
                .all(|s| plain(&s.key, 64) && plain(&s.value, MAX_VALUE_BYTES))
    }

    /// Applies this file on top of `base` (a copy of a built-in filetype).
    fn apply(&self, mut ft: Filetype) -> Filetype {
        ft.id.clone_from(&self.id);
        if let Some(name) = &self.name {
            ft.name.clone_from(name);
        }
        if let Some(extensions) = &self.extensions {
            ft.extensions.clone_from(extensions);
        }
        for (key, value) in &self.settings {
            ft.settings.insert(key.clone(), value.clone());
        }
        for named in &self.keywords {
            if let Some(set) = ft.keywords.iter_mut().find(|k| k.key == named.key) {
                set.words.clone_from(&named.words);
            }
        }
        for named in &self.styles {
            for style in ft.styles.iter_mut().filter(|s| s.key == named.key) {
                style.value.clone_from(&named.value);
            }
        }
        ft
    }
}

/// Reads up to [`MAX_FILES`] `*.toml` regular files of a directory.
fn read_dir_toml(dir: &Path, warnings: &mut Vec<String>) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    if paths.len() > MAX_FILES {
        warnings.push(format!("too many user files; only {MAX_FILES} read"));
        paths.truncate(MAX_FILES);
    }
    paths
        .iter()
        .filter_map(
            |path| match ferny_core::fsio::read_limited(path, MAX_FILE_BYTES) {
                Ok(file) => String::from_utf8(file.bytes).ok(),
                Err(_) => {
                    warnings
                        .push("a user file is unreadable or too large and was skipped".to_owned());
                    None
                }
            },
        )
        .collect()
}

impl Catalog {
    /// The built-in catalog plus the user files under `config_dir`
    /// (`filetypes/`, `colorschemes/`), and warnings for skipped files.
    pub fn with_user_dir(config_dir: &Path) -> (Catalog, Vec<String>) {
        let mut catalog = Catalog::builtin().clone();
        let mut warnings = Vec::new();
        for text in read_dir_toml(&config_dir.join("filetypes"), &mut warnings) {
            match toml::from_str::<UserFiletype>(&text) {
                Ok(user) if user.is_valid() => {
                    if !catalog.add_user_filetype(&user) {
                        warnings.push(
                            "a user filetype has no built-in base and was skipped".to_owned(),
                        );
                    }
                }
                _ => warnings.push("a user filetype file is invalid and was skipped".to_owned()),
            }
        }
        for text in read_dir_toml(&config_dir.join("colorschemes"), &mut warnings) {
            match toml::from_str::<ColorScheme>(&text) {
                Ok(scheme) if valid_scheme(&scheme) => {
                    catalog.schemes.retain(|s| s.id != scheme.id);
                    catalog.schemes.push(scheme);
                }
                _ => warnings.push("a user colour scheme is invalid and was skipped".to_owned()),
            }
        }
        catalog.schemes.sort_by(|a, b| a.id.cmp(&b.id));
        (catalog, warnings)
    }

    fn add_user_filetype(&mut self, user: &UserFiletype) -> bool {
        let base_id = if self.filetype(&user.id).is_some() {
            user.id.as_str()
        } else {
            match &user.base {
                Some(base) => base.as_str(),
                None => return false,
            }
        };
        let Some(base) = self.filetype(base_id).cloned() else {
            return false;
        };
        let merged = user.apply(base);
        self.filetypes.retain(|ft| ft.id != merged.id);
        self.filetypes.push(merged);
        self.filetypes.sort_by(|a, b| a.id.cmp(&b.id));
        true
    }
}

fn valid_scheme(scheme: &ColorScheme) -> bool {
    valid_id(&scheme.id)
        && plain(&scheme.name, 128)
        && plain(&scheme.description, 1024)
        && plain(&scheme.author, 256)
        && plain(&scheme.license, 256)
        && scheme.colors.len() <= 1024
        && scheme
            .colors
            .iter()
            .all(|(k, v)| plain(k, 64) && plain(v, 64))
        && scheme.styles.len() <= 1024
        && scheme
            .styles
            .iter()
            .all(|s| plain(&s.name, 64) && plain(&s.value, MAX_VALUE_BYTES))
}
