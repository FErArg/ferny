//! Which server serves which filetype: a built-in list of well-known
//! servers, changed or disabled by the user's `lsp.toml` (H3.1a).
//!
//! ```toml
//! [servers.clangd]
//! command = ["clangd", "--background-index"]
//! filetypes = ["c", "cpp"]
//! env = ["CPATH"]        # more environment variables for this server
//! enabled = true
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// Largest `lsp.toml` read, in bytes.
pub const MAX_CONFIG_BYTES: u64 = 64 * 1024;
/// Most servers kept.
pub const MAX_SERVERS: usize = 32;

/// One server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// Name (`clangd`).
    pub name: String,
    /// Program and arguments (no shell).
    pub command: Vec<String>,
    /// Filetype ids it serves.
    pub filetypes: Vec<String>,
    /// Extra environment variables passed through.
    pub env: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerFile {
    #[serde(default)]
    command: Option<Vec<String>>,
    #[serde(default)]
    filetypes: Option<Vec<String>>,
    #[serde(default)]
    env: Option<Vec<String>>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    servers: BTreeMap<String, ServerFile>,
}

fn builtin() -> Vec<ServerConfig> {
    let server = |name: &str, command: &[&str], filetypes: &[&str]| ServerConfig {
        name: name.to_owned(),
        command: command.iter().map(|s| (*s).to_owned()).collect(),
        filetypes: filetypes.iter().map(|s| (*s).to_owned()).collect(),
        env: Vec::new(),
    };
    vec![
        server("clangd", &["clangd"], &["c", "cpp"]),
        server("rust-analyzer", &["rust-analyzer"], &["rust"]),
        server("pylsp", &["pylsp"], &["python"]),
        server(
            "typescript-language-server",
            &["typescript-language-server", "--stdio"],
            &["javascript", "typescript"],
        ),
    ]
}

fn valid_word(text: &str) -> bool {
    !text.is_empty() && text.len() <= 1024 && !text.chars().any(char::is_control)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn valid_env(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The servers: the built-in ones, then `path` (a missing file changes
/// nothing). Invalid entries are dropped and reported.
pub fn load(path: Option<&Path>) -> (Vec<ServerConfig>, Vec<String>) {
    let mut servers = builtin();
    let Some(path) = path.filter(|p| p.exists()) else {
        return (servers, Vec::new());
    };
    let text = std::fs::metadata(path)
        .ok()
        .filter(|m| m.is_file() && m.len() <= MAX_CONFIG_BYTES)
        .and_then(|_| std::fs::read_to_string(path).ok());
    let Some(text) = text else {
        return (
            servers,
            vec!["lsp.toml: unreadable or too large".to_owned()],
        );
    };
    match toml::from_str::<ConfigFile>(&text) {
        Ok(file) => {
            let warnings = apply(&mut servers, file);
            (servers, warnings)
        }
        Err(_) => (servers, vec!["lsp.toml: not valid, ignored".to_owned()]),
    }
}

fn apply(servers: &mut Vec<ServerConfig>, file: ConfigFile) -> Vec<String> {
    let mut warnings = Vec::new();
    for (name, entry) in file.servers {
        let valid = valid_id(&name)
            && entry
                .command
                .as_ref()
                .is_none_or(|c| !c.is_empty() && c.iter().all(|w| valid_word(w)))
            && entry
                .filetypes
                .as_ref()
                .is_none_or(|f| f.iter().all(|id| valid_id(id)))
            && entry
                .env
                .as_ref()
                .is_none_or(|e| e.iter().all(|n| valid_env(n)));
        if !valid {
            warnings.push(format!("lsp.toml: server {name} is not valid, ignored"));
            continue;
        }
        if entry.enabled == Some(false) {
            servers.retain(|s| s.name != name);
            continue;
        }
        match servers.iter_mut().find(|s| s.name == name) {
            Some(server) => {
                if let Some(command) = entry.command {
                    server.command = command;
                }
                if let Some(filetypes) = entry.filetypes {
                    server.filetypes = filetypes;
                }
                if let Some(env) = entry.env {
                    server.env = env;
                }
            }
            None => match (entry.command, entry.filetypes) {
                (Some(command), Some(filetypes)) if servers.len() < MAX_SERVERS => {
                    servers.push(ServerConfig {
                        name,
                        command,
                        filetypes,
                        env: entry.env.unwrap_or_default(),
                    });
                }
                _ => warnings.push(format!(
                    "lsp.toml: server {name} needs a command and filetypes, ignored"
                )),
            },
        }
    }
    warnings
}

/// The server for `filetype`, if any.
pub fn for_filetype<'a>(servers: &'a [ServerConfig], filetype: &str) -> Option<&'a ServerConfig> {
    servers
        .iter()
        .find(|s| s.filetypes.iter().any(|f| f == filetype))
}
