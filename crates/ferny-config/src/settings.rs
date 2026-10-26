//! User settings (`settings.toml`), declared once in [`DESCRIPTORS`].
//!
//! Layers, later wins: built-in defaults, system file, user file. Each
//! file is untrusted input: read with a size limit, parsed as TOML, and
//! every known key validated on its own (an invalid value is ignored with a
//! warning, never fatal). Unknown keys are kept on save but never
//! interpreted. The preferences dialog is generated from the descriptors.

use std::collections::BTreeMap;
use std::path::Path;

use ferny_core::fsio;
use ferny_core::text::encoding_for_label;

/// Largest settings file read, in bytes.
pub const MAX_SETTINGS_BYTES: u64 = 256 * 1024;

/// Type and range of a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `true` / `false`.
    Bool,
    /// Integer in `min..=max`.
    Int {
        /// Smallest value.
        min: i64,
        /// Largest value.
        max: i64,
    },
    /// One of the listed strings.
    Choice(&'static [&'static str]),
    /// A font family name (empty = system fixed-width font).
    Font,
    /// A text encoding label.
    Encoding,
    /// A colour scheme id.
    Scheme,
    /// Free text on one line (≤ 256 bytes, no control characters).
    Text,
}

/// A setting value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i64),
    /// Text (choices, fonts, encodings, scheme ids).
    Text(String),
}

impl Value {
    /// The value as the UI shows and edits it.
    pub fn to_text(&self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Text(t) => t.clone(),
        }
    }

    fn to_toml(&self) -> toml::Value {
        match self {
            Self::Bool(b) => toml::Value::Boolean(*b),
            Self::Int(i) => toml::Value::Integer(*i),
            Self::Text(t) => toml::Value::String(t.clone()),
        }
    }
}

/// Declaration of one setting.
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    /// `section.name`, also the TOML location (`[section] name = …`).
    pub key: &'static str,
    /// English source title (translated by the UI).
    pub title: &'static str,
    /// Type and range.
    pub kind: Kind,
    /// Built-in default as text.
    pub default: &'static str,
}

/// Every setting (maintainer decision, H1.5a: the basic set).
pub const DESCRIPTORS: &[Descriptor] = &[
    Descriptor {
        key: "editor.font_family",
        title: "Font",
        kind: Kind::Font,
        default: "",
    },
    Descriptor {
        key: "editor.font_size",
        title: "Font size",
        kind: Kind::Int { min: 6, max: 72 },
        default: "12",
    },
    Descriptor {
        key: "editor.indent_width",
        title: "Indentation width",
        kind: Kind::Int { min: 1, max: 16 },
        default: "4",
    },
    Descriptor {
        key: "editor.indent_use_tabs",
        title: "Indent with tabs",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.line_numbers",
        title: "Show line numbers",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.long_line_marker",
        title: "Long line marker column (0 = off)",
        kind: Kind::Int { min: 0, max: 1000 },
        default: "72",
    },
    Descriptor {
        key: "editor.auto_close",
        title: "Auto-close brackets",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.show_invisible",
        title: "Show invisible characters",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.word_wrap",
        title: "Word wrap",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.indent_guides",
        title: "Indentation guides",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.show_eol",
        title: "Show line endings",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.markers_margin",
        title: "Show bookmark margin",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.folding",
        title: "Code folding",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.scroll_past_end",
        title: "Allow scrolling past the last line",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.caret_margin_lines",
        title: "Lines kept visible around the cursor",
        kind: Kind::Int { min: 0, max: 20 },
        default: "0",
    },
    Descriptor {
        key: "editor.long_line_style",
        title: "Long line marker style",
        kind: Kind::Choice(&["line", "background"]),
        default: "line",
    },
    Descriptor {
        key: "editor.virtual_space",
        title: "Virtual space",
        kind: Kind::Choice(&["off", "rectangular", "everywhere"]),
        default: "off",
    },
    Descriptor {
        key: "editor.change_history",
        title: "Change history",
        kind: Kind::Choice(&["off", "margin", "indicator"]),
        default: "off",
    },
    Descriptor {
        key: "editor.auto_indent",
        title: "Auto-indent",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.detect_indent",
        title: "Detect indentation from the file",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.tab_indents",
        title: "Tab key indents selected lines",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.backspace_unindents",
        title: "Backspace unindents",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.smart_home",
        title: "Smart Home key",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "editor.newline_strips_trailing",
        title: "Remove trailing spaces on Enter",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.tab_width",
        title: "Tab width (0 = indentation width)",
        kind: Kind::Int { min: 0, max: 16 },
        default: "0",
    },
    Descriptor {
        key: "editor.auto_close_quotes",
        title: "Auto-close quotes",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "editor.color_scheme",
        title: "Colour scheme",
        kind: Kind::Scheme,
        default: "default",
    },
    Descriptor {
        key: "files.default_encoding",
        title: "Default encoding",
        kind: Kind::Encoding,
        default: "UTF-8",
    },
    Descriptor {
        key: "files.default_eol",
        title: "Default line ending",
        kind: Kind::Choice(&["native", "lf", "crlf", "cr"]),
        default: "native",
    },
    Descriptor {
        key: "files.trim_trailing_whitespace",
        title: "Remove trailing whitespace on save",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "files.ensure_final_newline",
        title: "Ensure a final newline on save",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "files.autosave",
        title: "Save modified files every minute",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "files.backup_copies",
        title: "Keep copies of the previous versions (5 per file)",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "spell.enabled",
        title: "Check spelling",
        kind: Kind::Bool,
        default: "false",
    },
    Descriptor {
        key: "spell.language",
        title: "Spelling dictionary (for example es_ES)",
        kind: Kind::Text,
        default: "en_US",
    },
    Descriptor {
        key: "files.restore_session",
        title: "Restore the last session on start",
        kind: Kind::Bool,
        default: "true",
    },
    Descriptor {
        key: "files.reload_on_external_change",
        title: "When a file changes on disk",
        kind: Kind::Choice(&["ask", "always", "never"]),
        default: "ask",
    },
    Descriptor {
        key: "templates.developer",
        title: "Developer name",
        kind: Kind::Text,
        default: "",
    },
    Descriptor {
        key: "templates.initials",
        title: "Initials",
        kind: Kind::Text,
        default: "",
    },
    Descriptor {
        key: "templates.mail",
        title: "Email address",
        kind: Kind::Text,
        default: "",
    },
    Descriptor {
        key: "templates.company",
        title: "Company",
        kind: Kind::Text,
        default: "",
    },
    Descriptor {
        key: "templates.version",
        title: "Initial version",
        kind: Kind::Text,
        default: "",
    },
    Descriptor {
        key: "ui.language",
        title: "Language (applied on restart)",
        kind: Kind::Choice(&["system", "en", "es"]),
        default: "system",
    },
    Descriptor {
        key: "files.browser_show_hidden",
        title: "Show hidden files in the Files sidebar",
        kind: Kind::Bool,
        default: "false",
    },
];

/// Looks up a descriptor.
pub fn descriptor(key: &str) -> Option<&'static Descriptor> {
    DESCRIPTORS.iter().find(|d| d.key == key)
}

/// Validates `text` for `kind`.
pub fn parse_value(kind: Kind, text: &str) -> Option<Value> {
    match kind {
        Kind::Bool => match text {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        Kind::Int { min, max } => text
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|n| (min..=max).contains(n))
            .map(Value::Int),
        Kind::Choice(choices) => choices
            .contains(&text)
            .then(|| Value::Text(text.to_owned())),
        Kind::Font => (text.len() <= 128 && !text.chars().any(char::is_control))
            .then(|| Value::Text(text.to_owned())),
        Kind::Text => (text.len() <= 256 && !text.chars().any(char::is_control))
            .then(|| Value::Text(text.to_owned())),
        Kind::Encoding => encoding_for_label(text).map(|e| Value::Text(e.name().to_owned())),
        Kind::Scheme => (!text.is_empty()
            && text.len() <= 64
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
        .then(|| Value::Text(text.to_owned())),
    }
}

fn from_toml(kind: Kind, value: &toml::Value) -> Option<Value> {
    match (kind, value) {
        (Kind::Bool, toml::Value::Boolean(b)) => Some(Value::Bool(*b)),
        (Kind::Int { .. }, toml::Value::Integer(i)) => parse_value(kind, &i.to_string()),
        (_, toml::Value::String(s)) if !matches!(kind, Kind::Bool | Kind::Int { .. }) => {
            parse_value(kind, s)
        }
        _ => None,
    }
}

/// Settings after layering.
#[derive(Debug, Clone)]
pub struct Settings {
    values: BTreeMap<&'static str, Value>,
    /// Defaults plus the system layer: what the user layer is compared to.
    base: BTreeMap<&'static str, Value>,
    /// The user file as read, to keep unknown keys on save.
    user_table: toml::Table,
    warnings: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let values: BTreeMap<_, _> = DESCRIPTORS
            .iter()
            .filter_map(|d| parse_value(d.kind, d.default).map(|v| (d.key, v)))
            .collect();
        Self {
            base: values.clone(),
            values,
            user_table: toml::Table::new(),
            warnings: Vec::new(),
        }
    }
}

impl Settings {
    /// Loads the layers; missing files are fine, broken ones are skipped
    /// with a warning.
    pub fn load(system: Option<&Path>, user: Option<&Path>) -> Self {
        let mut settings = Self::default();
        if let Some(table) = system.and_then(|p| settings.read_layer(p, "system")) {
            settings.apply(&table, "system");
        }
        settings.base = settings.values.clone();
        if let Some(table) = user.and_then(|p| settings.read_layer(p, "user")) {
            settings.apply(&table, "user");
            settings.user_table = table;
        }
        settings
    }

    /// Settings from the text of a user file alone (fuzzing, tests).
    pub fn from_user_text(text: &str) -> Self {
        let mut settings = Self::default();
        match text.parse::<toml::Table>() {
            Ok(table) => {
                settings.apply(&table, "user");
                settings.user_table = table;
            }
            Err(_) => settings
                .warnings
                .push("user settings: not valid TOML, ignored".to_owned()),
        }
        settings
    }

    fn read_layer(&mut self, path: &Path, layer: &str) -> Option<toml::Table> {
        let file = match fsio::read_limited(path, MAX_SETTINGS_BYTES) {
            Ok(file) => file,
            Err(fsio::ReadError::NotFound) => return None,
            Err(_) => {
                self.warnings
                    .push(format!("{layer} settings: unreadable or too large"));
                return None;
            }
        };
        let parsed = std::str::from_utf8(&file.bytes)
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok());
        if parsed.is_none() {
            self.warnings
                .push(format!("{layer} settings: not valid TOML, ignored"));
        }
        parsed
    }

    fn apply(&mut self, table: &toml::Table, layer: &str) {
        for descriptor in DESCRIPTORS {
            let Some((section, name)) = descriptor.key.split_once('.') else {
                continue;
            };
            let Some(raw) = table
                .get(section)
                .and_then(toml::Value::as_table)
                .and_then(|t| t.get(name))
            else {
                continue;
            };
            match from_toml(descriptor.kind, raw) {
                Some(value) => {
                    self.values.insert(descriptor.key, value);
                }
                None => self.warnings.push(format!(
                    "{layer} settings: invalid value for {}",
                    descriptor.key
                )),
            }
        }
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// The value of a boolean setting (false if unknown).
    pub fn bool(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Value::Bool(true)))
    }

    /// The value of an integer setting (0 if unknown).
    pub fn int(&self, key: &str) -> i64 {
        match self.get(key) {
            Some(Value::Int(i)) => *i,
            _ => 0,
        }
    }

    /// The value of a text setting (empty if unknown).
    pub fn text(&self, key: &str) -> &str {
        match self.get(key) {
            Some(Value::Text(t)) => t,
            _ => "",
        }
    }

    /// Sets `key` from text; false (and nothing changed) if the key is
    /// unknown or the value invalid.
    pub fn set(&mut self, key: &str, text: &str) -> bool {
        let Some(descriptor) = descriptor(key) else {
            return false;
        };
        match parse_value(descriptor.kind, text) {
            Some(value) => {
                self.values.insert(descriptor.key, value);
                true
            }
            None => false,
        }
    }

    /// Problems found while loading (no paths, no values).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The user layer as TOML: values that differ from defaults and the
    /// system layer, plus any unknown keys that were in the file.
    pub fn user_toml(&self) -> String {
        let mut table = self.user_table.clone();
        for descriptor in DESCRIPTORS {
            let Some((section, name)) = descriptor.key.split_once('.') else {
                continue;
            };
            let value = self.values.get(descriptor.key);
            let differs = value != self.base.get(descriptor.key);
            let entry = table
                .entry(section.to_owned())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let toml::Value::Table(section_table) = entry {
                match value {
                    Some(value) if differs => {
                        section_table.insert(name.to_owned(), value.to_toml());
                    }
                    _ => {
                        section_table.remove(name);
                    }
                }
            }
        }
        table.retain(|_, v| !matches!(v, toml::Value::Table(t) if t.is_empty()));
        toml::to_string(&table).unwrap_or_default()
    }

    /// Writes the user layer atomically.
    pub fn save(&self, path: &Path) -> Result<(), fsio::WriteError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(fsio::WriteError::from)?;
        }
        fsio::write_atomic(path, self.user_toml().as_bytes(), None).map(|_| ())
    }
}
