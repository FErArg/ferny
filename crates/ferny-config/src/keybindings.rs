//! User keybindings (`keybindings.toml`): command id → key sequences,
//! overriding the defaults of the command registry.
//!
//! ```toml
//! [keys]
//! "file.save" = ["Ctrl+S"]
//! "search.replace" = []          # no shortcut
//! ```
//!
//! Unknown commands and malformed sequences are dropped with a warning.

use std::collections::BTreeMap;
use std::path::Path;

use ferny_core::command::{Registry, is_valid_key_sequence};
use ferny_core::fsio;

/// Largest keybindings file read, in bytes.
pub const MAX_KEYBINDINGS_BYTES: u64 = 256 * 1024;
/// Most key sequences per command.
pub const MAX_KEYS: usize = 4;

/// Overrides read from or written to `keybindings.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keybindings {
    overrides: BTreeMap<String, Vec<String>>,
    warnings: Vec<String>,
}

/// Whether `id` names a plugin command: `plugin.<plugin id>.<command>`
/// (plugin ids `[a-z0-9_-]` ≤ 64, command ids `[a-z0-9_]` ≤ 32; H3.3c).
pub fn is_plugin_command(id: &str) -> bool {
    let mut parts = id.split('.');
    let (Some("plugin"), Some(plugin), Some(command), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let ok = |s: &str, max: usize, extra: &[char]| {
        !s.is_empty()
            && s.len() <= max
            && s.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || extra.contains(&c)
            })
    };
    ok(plugin, 64, &['-']) && ok(command, 32, &[])
}

impl Keybindings {
    /// Reads `path`, keeping only valid entries for commands in `registry`.
    pub fn load(path: &Path, registry: &Registry) -> Self {
        let mut bindings = Self::default();
        let file = match fsio::read_limited(path, MAX_KEYBINDINGS_BYTES) {
            Ok(file) => file,
            Err(fsio::ReadError::NotFound) => return bindings,
            Err(_) => {
                bindings
                    .warnings
                    .push("keybindings: unreadable or too large".to_owned());
                return bindings;
            }
        };
        let Some(table) = std::str::from_utf8(&file.bytes)
            .ok()
            .and_then(|t| t.parse::<toml::Table>().ok())
        else {
            bindings
                .warnings
                .push("keybindings: not valid TOML, ignored".to_owned());
            return bindings;
        };
        let Some(keys) = table.get("keys").and_then(toml::Value::as_table) else {
            return bindings;
        };
        for (id, value) in keys {
            let sequences: Option<Vec<String>> = value.as_array().and_then(|items| {
                items
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .filter(|s| is_valid_key_sequence(s))
                            .map(str::to_owned)
                    })
                    .collect()
            });
            match sequences {
                Some(seqs)
                    if (registry.get(id).is_some() || is_plugin_command(id))
                        && seqs.len() <= MAX_KEYS =>
                {
                    bindings.overrides.insert(id.clone(), seqs);
                }
                _ => bindings
                    .warnings
                    .push("keybindings: an entry was invalid and ignored".to_owned()),
            }
        }
        bindings
    }

    /// Sets the keys of a command (validated); `None` restores the default.
    pub fn set(&mut self, registry: &Registry, id: &str, keys: Option<Vec<String>>) -> bool {
        if registry.get(id).is_none() {
            return false;
        }
        match keys {
            None => {
                self.overrides.remove(id);
                true
            }
            Some(keys) => {
                if keys.len() > MAX_KEYS || !keys.iter().all(|k| is_valid_key_sequence(k)) {
                    return false;
                }
                self.overrides.insert(id.to_owned(), keys);
                true
            }
        }
    }

    /// The keys of `id`: the override, or the registry default.
    pub fn keys<'a>(&'a self, registry: &'a Registry, id: &str) -> &'a [String] {
        match self.overrides.get(id) {
            Some(keys) => keys,
            None => registry.get(id).map(|c| c.default_keys()).unwrap_or(&[]),
        }
    }

    /// The user's keys for plugin command `id` (H3.3c), without those a
    /// built-in command uses: a plugin never takes over a built-in key.
    pub fn plugin_keys(&self, registry: &Registry, id: &str) -> Vec<String> {
        self.overrides
            .get(id)
            .filter(|_| is_plugin_command(id))
            .map(|keys| {
                keys.iter()
                    .filter(|k| self.conflicts(registry, id, k).is_empty())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Commands other than `id` that use `key` (for conflict warnings).
    pub fn conflicts(&self, registry: &Registry, id: &str, key: &str) -> Vec<String> {
        registry
            .commands()
            .map(|c| c.id().as_str())
            .filter(|other| *other != id)
            .filter(|other| self.keys(registry, other).iter().any(|k| k == key))
            .map(str::to_owned)
            .collect()
    }

    /// Problems found while loading (no paths, no values).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The file contents.
    pub fn to_toml(&self) -> String {
        let mut keys = toml::Table::new();
        for (id, seqs) in &self.overrides {
            keys.insert(
                id.clone(),
                toml::Value::Array(seqs.iter().cloned().map(toml::Value::String).collect()),
            );
        }
        let mut root = toml::Table::new();
        root.insert("keys".to_owned(), toml::Value::Table(keys));
        toml::to_string(&root).unwrap_or_default()
    }

    /// Writes the overrides atomically.
    pub fn save(&self, path: &Path) -> Result<(), fsio::WriteError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(fsio::WriteError::from)?;
        }
        fsio::write_atomic(path, self.to_toml().as_bytes(), None).map(|_| ())
    }
}
