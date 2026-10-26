//! Lua 5.4 plugin host (H3.2, threats T4, T5, T17, decision R2).
//!
//! Plugins live only in the user's configuration directory
//! (`plugins/<id>/plugin.toml` + `main.lua`). Each runs in its own Lua state
//! with `table`, `string`, `math` and `utf8` only (no `io`, `os`, `package`,
//! `debug`, `coroutine`, `load`, `require`, `string.dump`), text chunks
//! only, 64 MB of memory and 50 ms per call. Permissions come from the
//! manifest and are granted by the user (`plugin-grants.toml`).
#![forbid(unsafe_code)]

pub mod pattern;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ferny_core::fsio::{self, FileMode};
use mlua::chunk::ChunkMode;
use mlua::{HookTriggers, Lua, LuaOptions, StdLib, VmState};
use serde::{Deserialize, Serialize};

/// The permissions a plugin may ask for (R2).
pub const PERMISSIONS: [&str; 7] = [
    "fs.read.project",
    "fs.read.any",
    "fs.write.project",
    "process.spawn",
    "net",
    "clipboard",
    "config",
];
/// Permissions that also ask at their first use (R2).
pub const SENSITIVE: [&str; 3] = ["process.spawn", "net", "fs.read.any"];
/// Memory per plugin (R2).
pub const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// Time per synchronous call (R2).
pub const CALL_BUDGET: Duration = Duration::from_millis(50);
/// Most plugins loaded.
pub const MAX_PLUGINS: usize = 64;
/// Largest grants file or `main.lua`, in bytes.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Largest manifest, in bytes.
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024;

/// Events a plugin can handle (H3.3a; `document.before_save`, H3.3c,
/// runs through [`Plugin::before_save`]).
pub const EVENTS: [&str; 6] = [
    "app.startup_complete",
    "document.opened",
    "document.activated",
    "document.before_save",
    "document.saved",
    "document.closed",
];
/// Extension points (H3.3c).
pub const PROVIDERS: [&str; 3] = ["completion", "hover", "format"];
/// Most providers per plugin.
pub const MAX_PROVIDERS: usize = 16;
/// Most completion items kept from one answer.
pub const MAX_COMPLETIONS: usize = 500;
/// Longest completion item, in bytes.
pub const MAX_COMPLETION_BYTES: usize = 256;
/// Longest hover text, in bytes.
pub const MAX_HOVER_BYTES: usize = 4096;
/// Longest text a formatter or `before_save` returns, in bytes.
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
/// Most actions per call.
pub const MAX_ACTIONS: usize = 64;
/// Longest status text, in bytes.
pub const MAX_STATUS_BYTES: usize = 256;
/// Longest message, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 4096;
/// Most text a command inserts per call, in bytes.
pub const MAX_EDIT_BYTES: usize = 1024 * 1024;
/// Most commands per plugin.
pub const MAX_COMMANDS: usize = 32;
/// Most event handlers per plugin.
pub const MAX_HANDLERS: usize = 64;
/// Largest locale file, in bytes.
pub const MAX_LOCALE_BYTES: u64 = 64 * 1024;
/// Largest file `ferny.fs.read` returns or `ferny.fs.write` writes (H3.3b).
pub const MAX_FS_BYTES: usize = 4 * 1024 * 1024;
/// Largest private configuration value, in bytes.
pub const MAX_CONFIG_BYTES: usize = 256 * 1024;
/// Most entries `ferny.fs.list` returns.
pub const MAX_LIST_ENTRIES: usize = 1000;
/// Longest a plugin's process runs.
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
/// Output kept per stream of a plugin's process, in bytes.
pub const MAX_STREAM_BYTES: usize = 512 * 1024;
/// Processes a plugin may have running at once.
pub const MAX_PROCESSES: usize = 4;
/// Most arguments of a process, and the longest one (or program), in bytes.
pub const MAX_ARGS: usize = 64;
/// Longest program name or argument, in bytes.
pub const MAX_ARG_BYTES: usize = 4096;

pub(crate) const TIME_LIMIT_MESSAGE: &str = "ferny: plugin time limit";

/// Why a plugin cannot be loaded or run. No contents inside.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    /// Bad or missing manifest.
    #[error("invalid manifest")]
    Manifest,
    /// `main.lua` missing, too large or not UTF-8.
    #[error("missing or invalid main.lua")]
    Code,
    /// The plugin's code failed (first line, bounded).
    #[error("plugin error: {0}")]
    Runtime(String),
}

/// A plugin's manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Id: the folder name (`[a-z0-9_-]`, ≤ 64).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Version text.
    pub version: String,
    /// Permissions asked for, sorted, from `PERMISSIONS`.
    pub permissions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    api: u32,
    #[serde(default)]
    permissions: Vec<String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
}

fn valid_text(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= 128 && !text.chars().any(char::is_control)
}

/// Parses a manifest whose folder is named `folder`.
pub fn parse_manifest(text: &str, folder: &str) -> Result<Manifest, PluginError> {
    let file: ManifestFile = toml::from_str(text).map_err(|_| PluginError::Manifest)?;
    let mut permissions = file.permissions;
    permissions.sort();
    permissions.dedup();
    let valid = file.id == folder
        && valid_id(&file.id)
        && valid_text(&file.name)
        && valid_text(&file.version)
        && file.api == 0
        && permissions
            .iter()
            .all(|p| PERMISSIONS.contains(&p.as_str()));
    if !valid {
        return Err(PluginError::Manifest);
    }
    Ok(Manifest {
        id: file.id,
        name: file.name,
        version: file.version,
        permissions,
    })
}

fn read_text(path: &Path, limit: u64) -> Option<String> {
    // Regular files only; links are not followed.
    if !std::fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let file = fsio::read_limited(path, limit).ok()?;
    String::from_utf8(file.bytes).ok()
}

/// Id prefix of the plugins bundled with Ferny; user plugins may not use it.
pub const BUNDLED_PREFIX: &str = "ferny-";

/// A plugin compiled into Ferny (H3.4): never read from disk at run time.
#[derive(Debug, PartialEq, Eq)]
pub struct BundledPlugin {
    folder: &'static str,
    manifest: &'static str,
    code: &'static str,
    locales: &'static [(&'static str, &'static str)],
}

static BUNDLED: [BundledPlugin; 6] = [
    BundledPlugin {
        folder: "ferny-classbuilder",
        manifest: include_str!("../../../plugins/ferny-classbuilder/plugin.toml"),
        code: include_str!("../../../plugins/ferny-classbuilder/main.lua"),
        locales: &[(
            "es",
            include_str!("../../../plugins/ferny-classbuilder/locale/es.toml"),
        )],
    },
    BundledPlugin {
        folder: "ferny-htmlchars",
        manifest: include_str!("../../../plugins/ferny-htmlchars/plugin.toml"),
        code: include_str!("../../../plugins/ferny-htmlchars/main.lua"),
        locales: &[(
            "es",
            include_str!("../../../plugins/ferny-htmlchars/locale/es.toml"),
        )],
    },
    BundledPlugin {
        folder: "addon-addons",
        manifest: include_str!("../../../plugins/addon-addons/plugin.toml"),
        code: include_str!("../../../plugins/addon-addons/main.lua"),
        locales: &[],
    },
    BundledPlugin {
        folder: "addon-geniuspaste",
        manifest: include_str!("../../../plugins/addon-geniuspaste/plugin.toml"),
        code: include_str!("../../../plugins/addon-geniuspaste/main.lua"),
        locales: &[],
    },
    BundledPlugin {
        folder: "addon-lipsum",
        manifest: include_str!("../../../plugins/addon-lipsum/plugin.toml"),
        code: include_str!("../../../plugins/addon-lipsum/main.lua"),
        locales: &[],
    },
    BundledPlugin {
        folder: "addon-macros",
        manifest: include_str!("../../../plugins/addon-macros/plugin.toml"),
        code: include_str!("../../../plugins/addon-macros/main.lua"),
        locales: &[],
    },
];

/// Where a plugin's code comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A folder in the user's `plugins/` directory.
    Folder(PathBuf),
    /// Compiled into Ferny.
    Bundled(&'static BundledPlugin),
}

/// The bundled plugins with their manifests, sorted by id.
pub fn bundled() -> Vec<(Source, Manifest)> {
    BUNDLED
        .iter()
        .filter_map(|b| {
            parse_manifest(b.manifest, b.folder)
                .ok()
                .map(|m| (Source::Bundled(b), m))
        })
        .collect()
}

/// The plugins under `plugins_dir`: folders (not links) with a valid
/// manifest, sorted by id; problems are reported, not fatal.
pub fn discover(plugins_dir: &Path) -> (Vec<(PathBuf, Manifest)>, Vec<String>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return (found, warnings);
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let folder = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        if folder.starts_with(BUNDLED_PREFIX) {
            warnings.push(format!("plugin {folder}: reserved id, not loaded"));
            continue;
        }
        if found.len() >= MAX_PLUGINS {
            warnings.push(format!("plugin {folder}: too many plugins, not loaded"));
            continue;
        }
        let manifest = read_text(&dir.join("plugin.toml"), MAX_MANIFEST_BYTES)
            .ok_or(PluginError::Manifest)
            .and_then(|text| parse_manifest(&text, &folder));
        match manifest {
            Ok(manifest) => found.push((dir, manifest)),
            Err(_) => warnings.push(format!("plugin {folder}: invalid manifest, not loaded")),
        }
    }
    (found, warnings)
}

/// The user's decision about one plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    /// Whether the plugin runs at all.
    pub enabled: bool,
    /// The manifest's permissions when the user decided.
    pub requested: Vec<String>,
    /// The permissions granted (a subset of `requested`).
    pub granted: Vec<String>,
    /// Sensitive permissions confirmed at first use (H3.3b).
    #[serde(default)]
    pub confirmed: Vec<String>,
    /// Sensitive permissions refused at first use (H3.3b).
    #[serde(default)]
    pub refused: Vec<String>,
}

/// `plugin-grants.toml`: decisions by plugin id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    by_id: BTreeMap<String, Grant>,
}

impl Grants {
    /// Reads `path`; missing or invalid means nothing granted (fail closed).
    pub fn load(path: &Path) -> Self {
        let parsed = read_text(path, MAX_FILE_BYTES)
            .and_then(|text| toml::from_str::<BTreeMap<String, Grant>>(&text).ok());
        let mut grants = Self::default();
        for (id, grant) in parsed.unwrap_or_default() {
            let consistent = valid_id(&id)
                && grant.granted.iter().all(|p| grant.requested.contains(p))
                && grant
                    .confirmed
                    .iter()
                    .chain(&grant.refused)
                    .all(|p| grant.granted.contains(p) && SENSITIVE.contains(&p.as_str()))
                && grant
                    .requested
                    .iter()
                    .all(|p| PERMISSIONS.contains(&p.as_str()));
            if consistent {
                grants.by_id.insert(id, grant);
            }
        }
        grants
    }

    /// Writes `path` (owner-only, atomic).
    pub fn save(&self, path: &Path) -> bool {
        let Ok(text) = toml::to_string(&self.by_id) else {
            return false;
        };
        fsio::write_atomic_with_mode(path, text.as_bytes(), None, FileMode::OwnerOnly).is_ok()
    }

    /// The decision for `manifest`, if it still matches its permissions
    /// (a changed manifest needs a new decision).
    pub fn decision(&self, manifest: &Manifest) -> Option<&Grant> {
        self.by_id
            .get(&manifest.id)
            .filter(|g| g.requested == manifest.permissions)
    }

    /// Records a decision; `granted` is reduced to the manifest's
    /// permissions.
    pub fn decide(&mut self, manifest: &Manifest, enabled: bool, granted: &[String]) {
        let mut granted: Vec<String> = granted
            .iter()
            .filter(|p| manifest.permissions.contains(p))
            .cloned()
            .collect();
        granted.sort();
        granted.dedup();
        let granted = if enabled { granted } else { Vec::new() };
        // First-use answers survive a new decision (H3.4) while the
        // manifest is the same and the permission is still granted.
        let kept = |pick: fn(&Grant) -> &Vec<String>| -> Vec<String> {
            self.decision(manifest)
                .map(|g| {
                    pick(g)
                        .iter()
                        .filter(|p| granted.contains(p))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
        let confirmed = kept(|g| &g.confirmed);
        let refused = kept(|g| &g.refused);
        self.by_id.insert(
            manifest.id.clone(),
            Grant {
                enabled,
                requested: manifest.permissions.clone(),
                granted,
                confirmed,
                refused,
            },
        );
    }

    /// Forgets the first-use answers of `manifest`'s plugin, so its
    /// sensitive permissions ask again (H3.4).
    pub fn reset_confirmations(&mut self, manifest: &Manifest) -> bool {
        match self
            .by_id
            .get_mut(&manifest.id)
            .filter(|g| g.requested == manifest.permissions)
        {
            Some(grant) => {
                grant.confirmed.clear();
                grant.refused.clear();
                true
            }
            None => false,
        }
    }

    /// Records the first-use answer for a sensitive permission granted to
    /// `manifest`'s plugin; false if there is nothing to answer.
    pub fn confirm(&mut self, manifest: &Manifest, permission: &str, allow: bool) -> bool {
        let Some(grant) = self
            .by_id
            .get_mut(&manifest.id)
            .filter(|g| g.requested == manifest.permissions)
        else {
            return false;
        };
        if !SENSITIVE.contains(&permission) || !grant.granted.iter().any(|p| p == permission) {
            return false;
        }
        grant.confirmed.retain(|p| p != permission);
        grant.refused.retain(|p| p != permission);
        if allow {
            grant.confirmed.push(permission.to_owned());
        } else {
            grant.refused.push(permission.to_owned());
        }
        true
    }
}

/// The current document, as the UI sees it when a plugin is called.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocInfo {
    /// Path (empty for an unsaved document).
    pub path: String,
    /// Filetype id (empty for plain text).
    pub filetype: String,
    /// Cursor line, 1-based.
    pub line: u32,
    /// Cursor column, 1-based.
    pub column: u32,
    /// Whole text: given to commands only.
    pub text: String,
    /// Selected text: given to commands only.
    pub selection: String,
    /// Clipboard text: given to commands only (H3.3b).
    pub clipboard: String,
    /// The word at the position (completion: its part before the cursor;
    /// H3.3c).
    pub word: String,
}

/// A provider's answer (H3.3c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Completion items (valid ones only, at most [`MAX_COMPLETIONS`]).
    Items(Vec<String>),
    /// Hover text (controls other than newlines escaped) or a formatted
    /// document.
    Text(String),
}

/// What a plugin asks the UI to do after a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Show a status-bar text (neutralised).
    Status(String),
    /// Add a line to Messages (neutralised).
    Message(String),
    /// Insert text at the cursor of the document that was current.
    Insert(String),
    /// Replace the selection of the document that was current.
    ReplaceSelection(String),
    /// Put text on the clipboard (H3.3b).
    ClipboardSet(String),
    /// Ask the user to confirm a sensitive permission at first use.
    Confirm(String),
}

/// A process a plugin asked for (H3.3b); the host resolves and runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnRequest {
    /// Token for [`Plugin::deliver`].
    pub token: u64,
    /// Program as the plugin gave it (resolved by the host).
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
}

/// How a plugin's process ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessOutcome {
    /// Exit code; `None` if killed or never started.
    pub code: Option<i32>,
    /// Killed after [`PROCESS_TIMEOUT`].
    pub timed_out: bool,
    /// Standard output, cut at [`MAX_STREAM_BYTES`].
    pub stdout: Vec<u8>,
    /// Standard error, cut at [`MAX_STREAM_BYTES`].
    pub stderr: Vec<u8>,
    /// Why it did not start (generic), if it did not.
    pub error: Option<String>,
}

/// Reads `stream` keeping at most `MAX_STREAM_BYTES`; the rest is read
/// and dropped so the process never blocks on a full pipe.
fn drain(mut stream: impl std::io::Read) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buf = [0u8; 8192];
    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 {
            break;
        }
        let room = MAX_STREAM_BYTES.saturating_sub(kept.len());
        kept.extend_from_slice(&buf[..n.min(room)]);
    }
    kept
}

/// Process groups of the plugin processes still running (audit F2).
static RUNNING: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Kills process group `pgid` (the plugin's process and its children);
/// `/bin/kill` with an argument vector, so no `unsafe` is needed.
fn kill_group(pgid: u32) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{pgid}")])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(unix))]
    let _ = pgid; // shortcut: Windows kills the process only, children survive
}

/// Kills every plugin process and its children: Ferny is quitting.
pub fn kill_all_processes() {
    let groups = RUNNING
        .lock()
        .map(|mut r| std::mem::take(&mut *r))
        .unwrap_or_default();
    for pgid in groups {
        kill_group(pgid);
    }
}

/// Runs `program` (already resolved) with `args` in `cwd` with exactly
/// `env`: no shell, stdin closed, in its own process group, which is
/// killed after `timeout` or by [`kill_all_processes`]. Blocks; call it off
/// the UI thread.
pub fn run_process(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(std::ffi::OsString, std::ffi::OsString)],
    timeout: Duration,
) -> ProcessOutcome {
    use std::process::{Command, Stdio};
    let mut command = Command::new(program);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    // CREATE_NO_WINDOW (winbase.h): a console tool gets no window of its own.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
    let child = command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return ProcessOutcome {
            error: Some("could not start".to_owned()),
            ..ProcessOutcome::default()
        };
    };
    let pgid = child.id();
    if let Ok(mut r) = RUNNING.lock() {
        r.push(pgid);
    }
    let out = child
        .stdout
        .take()
        .map(|s| std::thread::spawn(move || drain(s)));
    let err = child
        .stderr
        .take()
        .map(|s| std::thread::spawn(move || drain(s)));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() > timeout => {
                timed_out = true;
                kill_group(pgid);
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break None,
        }
    };
    // Children left behind by the program hold the pipes open: end them.
    // The group id is not reused while a member lives; with none left the
    // kill finds no group (only a new session leader taking that very id
    // in between could be hit).
    let ours = RUNNING.lock().map(|mut r| {
        let had = r.contains(&pgid);
        r.retain(|g| *g != pgid);
        had
    });
    if ours.unwrap_or(false) {
        kill_group(pgid);
    }
    let join = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    ProcessOutcome {
        code: if timed_out {
            None
        } else {
            status.and_then(|s| s.code())
        },
        timed_out,
        stdout: join(out),
        stderr: join(err),
        error: None,
    }
}

/// A command a plugin added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommand {
    /// `plugin.<plugin id>.<id>`.
    pub id: String,
    /// Menu title (validated, plain text).
    pub title: String,
}

#[derive(Default)]
struct CallState {
    loading: bool,
    granted: Vec<String>,
    confirmed: Vec<String>,
    refused: Vec<String>,
    logged: Vec<String>,
    /// Canonical root of the open project, only while it is trusted.
    project: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    spawns: Vec<SpawnRequest>,
    callbacks: Vec<(u64, Option<mlua::RegistryKey>)>,
    next_token: u64,
    doc: Option<DocInfo>,
    in_command: bool,
    actions: Vec<Action>,
    edit_bytes: usize,
    handlers: Vec<(String, mlua::RegistryKey)>,
    providers: Vec<(String, Vec<String>, mlua::RegistryKey)>,
    commands: Vec<(PluginCommand, mlua::RegistryKey)>,
}

type Shared = Arc<Mutex<CallState>>;

fn lock(state: &Shared) -> mlua::Result<std::sync::MutexGuard<'_, CallState>> {
    state
        .lock()
        .map_err(|_| mlua::Error::runtime("ferny: internal error"))
}

/// A running plugin.
pub struct Plugin {
    /// Its manifest.
    pub manifest: Manifest,
    lua: Lua,
    deadline: Arc<Mutex<Option<Instant>>>,
    state: Shared,
}

impl PluginError {
    /// Whether the plugin ran out of time (it is then disabled).
    pub fn is_time_limit(&self) -> bool {
        matches!(self, Self::Runtime(m) if m.contains(TIME_LIMIT_MESSAGE))
    }
}

/// Sandbox prelude, run before any plugin code: error-catching functions
/// rethrow the time limit, so a plugin cannot outlive its budget by
/// catching the error.
const PRELUDE: &str = r#"
local raw_pcall, raw_xpcall, pack, unpack = pcall, xpcall, table.pack, table.unpack
local tostring, find, error = tostring, string.find, error
local function check(r)
    if not r[1] and find(tostring(r[2]), "ferny: plugin time limit", 1, true) then
        error(r[2], 0)
    end
    return unpack(r, 1, r.n)
end
pcall = function(...) return check(pack(raw_pcall(...))) end
xpcall = function(...) return check(pack(raw_xpcall(...))) end
"#;

fn bounded_error(err: &mlua::Error) -> String {
    let text = err.to_string();
    let first = text.lines().next().unwrap_or("").to_owned();
    let mut end = first.len().min(256);
    while !first.is_char_boundary(end) {
        end -= 1;
    }
    first[..end].to_owned()
}

/// Valid command id: `[a-z0-9_]`, at most 32.
fn valid_command_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The plugin's translations for `language` (`locale/<language>.toml`).
fn load_locale(dir: &Path, language: &str) -> BTreeMap<String, String> {
    if language.is_empty() || !language.chars().all(|c| c.is_ascii_lowercase()) {
        return BTreeMap::new();
    }
    read_text(
        &dir.join("locale").join(format!("{language}.toml")),
        MAX_LOCALE_BYTES,
    )
    .and_then(|text| toml::from_str(&text).ok())
    .unwrap_or_default()
}

/// Checks a provider's return value (H3.3c).
fn answer(kind: &str, value: mlua::Value) -> mlua::Result<Option<Answer>> {
    match (kind, value) {
        (_, mlua::Value::Nil) => Ok(None),
        ("completion", mlua::Value::Table(t)) => {
            let mut items = Vec::new();
            for item in t.sequence_values::<mlua::Value>() {
                if let mlua::Value::String(s) = item? {
                    let s = s.to_str()?.to_owned();
                    let ok = !s.is_empty()
                        && s.len() <= MAX_COMPLETION_BYTES
                        && ferny_core::browse::display_name(&s) == s;
                    if ok && !items.contains(&s) {
                        items.push(s);
                    }
                }
                if items.len() >= MAX_COMPLETIONS {
                    break;
                }
            }
            Ok(Some(Answer::Items(items)))
        }
        ("hover", mlua::Value::String(s)) => {
            let s = s.to_str()?;
            if s.len() > MAX_HOVER_BYTES {
                return Err(too_long());
            }
            let shown: Vec<String> = s.lines().map(ferny_core::browse::display_name).collect();
            Ok(Some(Answer::Text(shown.join("\n"))))
        }
        ("format", mlua::Value::String(s)) => {
            if s.as_bytes().len() > MAX_DOCUMENT_BYTES {
                return Err(too_long());
            }
            Ok(Some(Answer::Text(s.to_str()?.to_owned())))
        }
        _ => Err(mlua::Error::runtime("ferny: invalid provider answer")),
    }
}

fn too_long() -> mlua::Error {
    mlua::Error::runtime("ferny: text too long")
}

/// Records an action, within the per-call bounds.
fn push(state: &Shared, action: Action) -> mlua::Result<()> {
    let mut s = lock(state)?;
    if s.actions.len() >= MAX_ACTIONS {
        return Err(mlua::Error::runtime("ferny: too many actions"));
    }
    s.actions.push(action);
    Ok(())
}

/// Text a plugin shows: bounded, then escaped like file names (T7).
fn shown(text: &str, limit: usize) -> mlua::Result<String> {
    if text.len() > limit {
        return Err(too_long());
    }
    Ok(ferny_core::browse::display_name(text))
}

type Log = std::rc::Rc<dyn Fn(&str)>;

/// Whether the call may use `permission`: granted and, if sensitive,
/// confirmed. An unanswered sensitive permission asks the user once after
/// the call (H3.3b). The first use of each permission is logged (T17).
fn allowed(state: &Shared, permission: &str, log: &Log) -> Result<(), String> {
    let mut s = lock(state).map_err(|e| e.to_string())?;
    if !s.granted.iter().any(|p| p == permission) || s.refused.iter().any(|p| p == permission) {
        return Err("permission denied".to_owned());
    }
    if SENSITIVE.contains(&permission) && !s.confirmed.iter().any(|p| p == permission) {
        let action = Action::Confirm(permission.to_owned());
        if !s.actions.contains(&action) && s.actions.len() < MAX_ACTIONS {
            s.actions.push(action);
        }
        return Err("confirmation pending".to_owned());
    }
    if !s.logged.iter().any(|p| p == permission) {
        s.logged.push(permission.to_owned());
        log(&format!("permission {permission} used"));
    }
    Ok(())
}

/// The project root of this call (canonical, trusted), if any.
fn project(state: &Shared) -> Option<PathBuf> {
    state.lock().ok().and_then(|s| s.project.clone())
}

/// A plugin path: relative ones are taken from the project root.
fn plugin_path(state: &Shared, path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    project(state)
        .map(|root| root.join(path))
        .ok_or_else(|| "no trusted project".to_owned())
}

/// Checks a read of `path` (file or folder): inside the trusted project
/// with `fs.read.project`, anywhere else with `fs.read.any`. The last
/// component must not be a link.
fn readable(state: &Shared, path: &str, log: &Log) -> Result<PathBuf, String> {
    let path = plugin_path(state, path)?;
    let meta = std::fs::symlink_metadata(&path).map_err(|_| "not found".to_owned())?;
    if meta.file_type().is_symlink() {
        return Err("links are not followed".to_owned());
    }
    let real = std::fs::canonicalize(&path).map_err(|_| "not found".to_owned())?;
    let inside = project(state).is_some_and(|root| real.starts_with(root));
    if !(inside && allowed(state, "fs.read.project", log).is_ok()) {
        allowed(state, "fs.read.any", log)?;
    }
    Ok(real)
}

/// Checks a write of `path`: inside the trusted project only, no `..`,
/// an existing target must be a regular file (not a link).
fn writable(state: &Shared, path: &str, log: &Log) -> Result<PathBuf, String> {
    let root = project(state).ok_or_else(|| "no trusted project".to_owned())?;
    let path = plugin_path(state, path)?;
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("outside the project".to_owned());
    }
    let name = path.file_name().ok_or_else(|| "invalid path".to_owned())?;
    let parent = path.parent().ok_or_else(|| "invalid path".to_owned())?;
    let parent = std::fs::canonicalize(parent).map_err(|_| "not found".to_owned())?;
    if !parent.starts_with(&root) {
        return Err("outside the project".to_owned());
    }
    let target = parent.join(name);
    if std::fs::symlink_metadata(&target).is_ok_and(|meta| !meta.file_type().is_file()) {
        return Err("not a regular file".to_owned());
    }
    allowed(state, "fs.write.project", log)?;
    Ok(target)
}

/// Private per-plugin configuration path. Plugin ids are validated manifest
/// ids; callers cannot provide a path or select another plugin's namespace.
fn config_path(state: &Shared, plugin_id: &str) -> Result<PathBuf, String> {
    if !valid_id(plugin_id) {
        return Err("invalid plugin id".to_owned());
    }
    let root = lock(state)
        .map_err(|_| "internal error".to_owned())?
        .config_dir
        .clone()
        .ok_or_else(|| "configuration unavailable".to_owned())?;
    let root = std::fs::canonicalize(root).map_err(|_| "configuration unavailable".to_owned())?;
    let path = root.join("plugin-config").join(plugin_id).join("data");
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("invalid configuration path".to_owned());
    }
    if let Ok(meta) = std::fs::symlink_metadata(&path)
        && (meta.file_type().is_symlink() || !meta.file_type().is_file())
    {
        return Err("invalid configuration file".to_owned());
    }
    Ok(path)
}

#[allow(clippy::too_many_lines)]
fn install_api(
    lua: &Lua,
    state: &Shared,
    plugin_id: &str,
    locale: BTreeMap<String, String>,
    log: Log,
) -> mlua::Result<()> {
    let api = lua.create_table()?;
    api.raw_set("api_version", 0)?;
    let plugin_log = log.clone();
    api.raw_set(
        "log",
        lua.create_function(move |_, message: String| {
            plugin_log(&message);
            Ok(())
        })?,
    )?;
    let st = Arc::clone(state);
    api.raw_set(
        "has_permission",
        lua.create_function(move |_, name: String| Ok(lock(&st)?.granted.contains(&name)))?,
    )?;
    api.raw_set(
        "tr",
        lua.create_function(move |_, text: String| Ok(locale.get(&text).cloned().unwrap_or(text)))?,
    )?;

    // Registration: only while main.lua runs, so the menus are complete.
    let st = Arc::clone(state);
    api.raw_set(
        "on",
        lua.create_function(move |lua, (event, f): (String, mlua::Function)| {
            let mut s = lock(&st)?;
            if !s.loading {
                return Err(mlua::Error::runtime("ferny.on: only while loading"));
            }
            if !EVENTS.contains(&event.as_str()) {
                return Err(mlua::Error::runtime("ferny.on: unknown event"));
            }
            if s.handlers.len() >= MAX_HANDLERS {
                return Err(mlua::Error::runtime("ferny.on: too many handlers"));
            }
            let key = lua.create_registry_value(f)?;
            s.handlers.push((event, key));
            Ok(())
        })?,
    )?;
    let st = Arc::clone(state);
    let prefix = format!("plugin.{plugin_id}.");
    api.raw_set(
        "command",
        lua.create_function(move |lua, spec: mlua::Table| {
            let id: String = spec.get("id")?;
            let title: String = spec.get("title")?;
            let run: mlua::Function = spec.get("run")?;
            let mut s = lock(&st)?;
            if !s.loading {
                return Err(mlua::Error::runtime("ferny.command: only while loading"));
            }
            let full = format!("{prefix}{id}");
            let title_ok = valid_text(&title) && ferny_core::browse::display_name(&title) == title;
            if !valid_command_id(&id) || !title_ok || s.commands.iter().any(|(c, _)| c.id == full) {
                return Err(mlua::Error::runtime("ferny.command: invalid command"));
            }
            if s.commands.len() >= MAX_COMMANDS {
                return Err(mlua::Error::runtime("ferny.command: too many commands"));
            }
            let key = lua.create_registry_value(run)?;
            s.commands.push((PluginCommand { id: full, title }, key));
            Ok(())
        })?,
    )?;

    let st = Arc::clone(state);
    api.raw_set(
        "provide",
        lua.create_function(move |lua, (kind, spec): (String, mlua::Table)| {
            let filetypes: Vec<String> = spec.get("filetypes")?;
            let run: mlua::Function = spec.get("run")?;
            let mut s = lock(&st)?;
            if !s.loading {
                return Err(mlua::Error::runtime("ferny.provide: only while loading"));
            }
            let valid = PROVIDERS.contains(&kind.as_str())
                && !filetypes.is_empty()
                && filetypes.len() <= 32
                && filetypes.iter().all(|f| {
                    f == "*"
                        || (!f.is_empty()
                            && f.len() <= 64
                            && f.chars()
                                .all(|c| c.is_ascii_alphanumeric() || "_-+".contains(c)))
                });
            if !valid {
                return Err(mlua::Error::runtime("ferny.provide: invalid provider"));
            }
            if s.providers.len() >= MAX_PROVIDERS {
                return Err(mlua::Error::runtime("ferny.provide: too many providers"));
            }
            let key = lua.create_registry_value(run)?;
            s.providers.push((kind, filetypes, key));
            Ok(())
        })?,
    )?;

    let documents = lua.create_table()?;
    let st = Arc::clone(state);
    documents.raw_set(
        "current",
        lua.create_function(move |lua, ()| {
            let s = lock(&st)?;
            let Some(doc) = s.doc.as_ref() else {
                return Ok(mlua::Value::Nil);
            };
            let t = lua.create_table()?;
            t.raw_set("path", doc.path.as_str())?;
            t.raw_set("filetype", doc.filetype.as_str())?;
            t.raw_set("line", doc.line)?;
            t.raw_set("column", doc.column)?;
            Ok(mlua::Value::Table(t))
        })?,
    )?;
    api.raw_set("documents", documents)?;

    // Editor access: inside commands the user runs only (T5, T9).
    let editor = lua.create_table()?;
    for (name, selection) in [("text", false), ("selection", true)] {
        let st = Arc::clone(state);
        editor.raw_set(
            name,
            lua.create_function(move |_, ()| {
                let s = lock(&st)?;
                match s.doc.as_ref().filter(|_| s.in_command) {
                    Some(doc) if selection => Ok(doc.selection.clone()),
                    Some(doc) => Ok(doc.text.clone()),
                    None => Err(mlua::Error::runtime("ferny.editor: only in commands")),
                }
            })?,
        )?;
    }
    for (name, replace) in [("insert", false), ("replace_selection", true)] {
        let st = Arc::clone(state);
        editor.raw_set(
            name,
            lua.create_function(move |_, text: String| {
                {
                    let mut s = lock(&st)?;
                    if !s.in_command || s.doc.is_none() {
                        return Err(mlua::Error::runtime("ferny.editor: only in commands"));
                    }
                    if s.edit_bytes + text.len() > MAX_EDIT_BYTES {
                        return Err(too_long());
                    }
                    s.edit_bytes += text.len();
                }
                push(
                    &st,
                    if replace {
                        Action::ReplaceSelection(text)
                    } else {
                        Action::Insert(text)
                    },
                )
            })?,
        )?;
    }
    api.raw_set("editor", editor)?;

    let ui = lua.create_table()?;
    let st = Arc::clone(state);
    ui.raw_set(
        "status",
        lua.create_function(move |_, text: String| {
            push(&st, Action::Status(shown(&text, MAX_STATUS_BYTES)?))
        })?,
    )?;
    let st = Arc::clone(state);
    ui.raw_set(
        "message",
        lua.create_function(move |_, text: String| {
            push(&st, Action::Message(shown(&text, MAX_MESSAGE_BYTES)?))
        })?,
    )?;
    api.raw_set("ui", ui)?;

    // Files (H3.3b): bounded, links not followed, writes atomic and only
    // inside the trusted project.
    let fs_table = lua.create_table()?;
    let (st, lg) = (Arc::clone(state), log.clone());
    fs_table.raw_set(
        "read",
        lua.create_function(move |lua, path: String| {
            let read = readable(&st, &path, &lg).and_then(|real| {
                let meta = std::fs::metadata(&real).map_err(|_| "not found".to_owned())?;
                if !meta.is_file() {
                    return Err("not a file".to_owned());
                }
                fsio::read_limited(&real, MAX_FS_BYTES as u64)
                    .map(|f| f.bytes)
                    .map_err(|_| "cannot read (missing or too large)".to_owned())
            });
            Ok(match read {
                Ok(bytes) => (Some(lua.create_string(bytes)?), None),
                Err(e) => (None, Some(e)),
            })
        })?,
    )?;
    let (st, lg) = (Arc::clone(state), log.clone());
    fs_table.raw_set(
        "list",
        lua.create_function(move |lua, path: String| {
            let listed = readable(&st, &path, &lg).and_then(|real| {
                let entries = std::fs::read_dir(real).map_err(|_| "not a folder".to_owned())?;
                Ok(entries
                    .flatten()
                    .take(MAX_LIST_ENTRIES)
                    .map(|e| {
                        let kind = match e.file_type() {
                            Ok(t) if t.is_symlink() => "link",
                            Ok(t) if t.is_dir() => "dir",
                            Ok(t) if t.is_file() => "file",
                            _ => "other",
                        };
                        (e.file_name().to_string_lossy().into_owned(), kind)
                    })
                    .collect::<Vec<_>>())
            });
            Ok(match listed {
                Ok(entries) => {
                    let list = lua.create_table()?;
                    for (name, kind) in entries {
                        let entry = lua.create_table()?;
                        entry.raw_set("name", name)?;
                        entry.raw_set("kind", kind)?;
                        list.raw_push(entry)?;
                    }
                    (Some(list), None)
                }
                Err(e) => (None, Some(e)),
            })
        })?,
    )?;
    let (st, lg) = (Arc::clone(state), log.clone());
    fs_table.raw_set(
        "write",
        lua.create_function(move |_, (path, text): (String, mlua::LuaString)| {
            let bytes = text.as_bytes().to_vec();
            if bytes.len() > MAX_FS_BYTES {
                return Ok((None, Some("too large".to_owned())));
            }
            let written = writable(&st, &path, &lg).and_then(|target| {
                fsio::write_atomic(&target, &bytes, None)
                    .map(|_| true)
                    .map_err(|_| "cannot write".to_owned())
            });
            Ok(match written {
                Ok(done) => (Some(done), None),
                Err(e) => (None, Some(e)),
            })
        })?,
    )?;
    api.raw_set("fs", fs_table)?;

    // Private plugin configuration: one bounded opaque UTF-8 value per
    // plugin, outside project files. The capability is explicit and granted
    // through the normal plugin decision flow.
    let config = lua.create_table()?;
    let config_id = plugin_id.to_owned();
    let read_id = config_id.clone();
    let (st, lg) = (Arc::clone(state), log.clone());
    config.raw_set(
        "read",
        lua.create_function(move |lua, ()| {
            allowed(&st, "config", &lg).map_err(mlua::Error::runtime)?;
            let path = config_path(&st, &read_id).map_err(mlua::Error::runtime)?;
            let value = match fsio::read_limited(&path, MAX_CONFIG_BYTES as u64) {
                Ok(file) => Some(lua.create_string(file.bytes)?),
                Err(_) => None,
            };
            Ok(value)
        })?,
    )?;
    let (st, lg) = (Arc::clone(state), log.clone());
    config.raw_set(
        "write",
        lua.create_function(move |_, value: mlua::LuaString| {
            allowed(&st, "config", &lg).map_err(mlua::Error::runtime)?;
            let bytes = value.as_bytes();
            if bytes.len() > MAX_CONFIG_BYTES {
                return Err(mlua::Error::runtime("ferny.config: value too large"));
            }
            let path = config_path(&st, &config_id).map_err(mlua::Error::runtime)?;
            let parent = path
                .parent()
                .ok_or_else(|| mlua::Error::runtime("ferny.config: invalid path"))?;
            std::fs::create_dir_all(parent)
                .map_err(|_| mlua::Error::runtime("ferny.config: cannot create storage"))?;
            fsio::write_atomic_with_mode(&path, &bytes, None, FileMode::OwnerOnly)
                .map_err(|_| mlua::Error::runtime("ferny.config: cannot write"))?;
            Ok(true)
        })?,
    )?;
    api.raw_set("config", config)?;

    // Processes (H3.3b): the host resolves and runs them; the result comes
    // back to `on_exit` in a later call.
    let (st, lg) = (Arc::clone(state), log.clone());
    api.raw_set(
        "spawn",
        lua.create_function(move |lua, spec: mlua::Table| {
            let program: String = spec.get("program")?;
            let args: Option<Vec<String>> = spec.get("args")?;
            let on_exit: Option<mlua::Function> = spec.get("on_exit")?;
            let args = args.unwrap_or_default();
            let bounded = !program.is_empty()
                && program.len() <= MAX_ARG_BYTES
                && args.len() <= MAX_ARGS
                && args.iter().all(|a| a.len() <= MAX_ARG_BYTES)
                && !program.contains('\0')
                && !args.iter().any(|a| a.contains('\0'));
            if !bounded {
                return Ok((None, Some("invalid program or arguments".to_owned())));
            }
            if let Err(e) = allowed(&st, "process.spawn", &lg) {
                return Ok((None, Some(e)));
            }
            let key = on_exit.map(|f| lua.create_registry_value(f)).transpose()?;
            let mut s = lock(&st)?;
            if s.callbacks.len() >= MAX_PROCESSES {
                return Ok((None, Some("too many processes".to_owned())));
            }
            s.next_token += 1;
            let token = s.next_token;
            s.callbacks.push((token, key));
            s.spawns.push(SpawnRequest {
                token,
                program,
                args,
            });
            Ok((Some(true), None))
        })?,
    )?;

    // Clipboard (H3.3b): write any time, read in commands only.
    let clipboard = lua.create_table()?;
    let (st, lg) = (Arc::clone(state), log.clone());
    clipboard.raw_set(
        "set",
        lua.create_function(move |_, text: String| {
            if text.len() > MAX_EDIT_BYTES {
                return Err(too_long());
            }
            allowed(&st, "clipboard", &lg).map_err(mlua::Error::runtime)?;
            push(&st, Action::ClipboardSet(text))
        })?,
    )?;
    let (st, lg) = (Arc::clone(state), log);
    clipboard.raw_set(
        "get",
        lua.create_function(move |_, ()| {
            allowed(&st, "clipboard", &lg).map_err(mlua::Error::runtime)?;
            let s = lock(&st)?;
            match s.doc.as_ref().filter(|_| s.in_command) {
                Some(doc) => Ok(doc.clipboard.clone()),
                None => Err(mlua::Error::runtime(
                    "ferny.clipboard.get: only in commands",
                )),
            }
        })?,
    )?;
    api.raw_set("clipboard", clipboard)?;

    lua.globals().raw_set("ferny", api)
}

impl Plugin {
    /// Creates the sandbox and runs the plugin's `main.lua` within one call
    /// budget. `language` picks `locale/<language>.toml` for `ferny.tr`;
    /// `log` receives `ferny.log` messages.
    pub fn start(
        dir: &Path,
        manifest: Manifest,
        granted: Vec<String>,
        language: &str,
        log: impl Fn(&str) + Send + 'static,
    ) -> Result<Self, PluginError> {
        Self::start_from(
            &Source::Folder(dir.to_path_buf()),
            manifest,
            granted,
            language,
            log,
        )
    }

    /// [`Plugin::start`] from a folder or from a bundled plugin.
    pub fn start_from(
        source: &Source,
        manifest: Manifest,
        granted: Vec<String>,
        language: &str,
        log: impl Fn(&str) + Send + 'static,
    ) -> Result<Self, PluginError> {
        Self::start_from_with_config(source, manifest, granted, language, None, log)
    }

    /// Starts a plugin with its private configuration root.
    pub fn start_from_with_config(
        source: &Source,
        manifest: Manifest,
        granted: Vec<String>,
        language: &str,
        config_dir: Option<&Path>,
        log: impl Fn(&str) + Send + 'static,
    ) -> Result<Self, PluginError> {
        let (code, locale) = match source {
            Source::Folder(dir) => (
                read_text(&dir.join("main.lua"), MAX_FILE_BYTES).ok_or(PluginError::Code)?,
                load_locale(dir, language),
            ),
            Source::Bundled(b) => (
                b.code.to_owned(),
                b.locales
                    .iter()
                    .find(|(l, _)| *l == language)
                    .and_then(|(_, text)| toml::from_str(text).ok())
                    .unwrap_or_default(),
            ),
        };
        let runtime = |e: mlua::Error| PluginError::Runtime(bounded_error(&e));
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
            LuaOptions::default(),
        )
        .map_err(runtime)?;
        lua.set_memory_limit(MEMORY_LIMIT).map_err(runtime)?;
        let globals = lua.globals();
        for name in [
            "load",
            "loadfile",
            "dofile",
            "require",
            "collectgarbage",
            "print",
        ] {
            globals.raw_set(name, mlua::Value::Nil).map_err(runtime)?;
        }
        let string: mlua::Table = globals.raw_get("string").map_err(runtime)?;
        string.raw_set("dump", mlua::Value::Nil).map_err(runtime)?;

        let deadline: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        // Pattern functions run in C, out of the hook's reach (audit F1).
        pattern::install(&lua, Arc::clone(&deadline)).map_err(runtime)?;
        let hook_deadline = Arc::clone(&deadline);
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(1000),
            move |_, _| {
                let expired = hook_deadline
                    .lock()
                    .ok()
                    .and_then(|d| *d)
                    .is_some_and(|d| Instant::now() > d);
                if expired {
                    Err(mlua::Error::runtime(TIME_LIMIT_MESSAGE))
                } else {
                    Ok(VmState::Continue)
                }
            },
        )
        .map_err(runtime)?;
        lua.load(PRELUDE)
            .set_mode(ChunkMode::Text)
            .exec()
            .map_err(runtime)?;

        let state: Shared = Arc::new(Mutex::new(CallState {
            loading: true,
            granted,
            config_dir: config_dir.map(Path::to_path_buf),
            ..CallState::default()
        }));
        install_api(&lua, &state, &manifest.id, locale, std::rc::Rc::new(log)).map_err(runtime)?;

        let plugin = Self {
            manifest,
            lua,
            deadline,
            state,
        };
        plugin.with_budget(|lua| {
            lua.load(&code)
                .set_name("main.lua")
                .set_mode(ChunkMode::Text)
                .exec()
        })?;
        if let Ok(mut s) = plugin.state.lock() {
            s.loading = false;
            // Actions from loading (a status at start) are dropped.
            s.actions.clear();
        }
        Ok(plugin)
    }

    /// Runs `f` within one call budget.
    fn with_budget<T>(&self, f: impl FnOnce(&Lua) -> mlua::Result<T>) -> Result<T, PluginError> {
        if let Ok(mut d) = self.deadline.lock() {
            *d = Some(Instant::now() + CALL_BUDGET);
        }
        let result = f(&self.lua);
        if let Ok(mut d) = self.deadline.lock() {
            *d = None;
        }
        result.map_err(|e| PluginError::Runtime(bounded_error(&e)))
    }

    /// Calls the global function `name` (no arguments) within one budget;
    /// `Ok(false)` if the plugin does not define it.
    pub fn call(&self, name: &str) -> Result<bool, PluginError> {
        let function: Option<mlua::Function> = self.lua.globals().get(name).ok();
        match function {
            Some(f) => self.with_budget(|_| f.call::<()>(())).map(|()| true),
            None => Ok(false),
        }
    }

    /// Sets the sensitive permissions confirmed and refused at first use.
    pub fn set_confirmations(&self, confirmed: &[String], refused: &[String]) {
        if let Ok(mut s) = self.state.lock() {
            s.confirmed = confirmed.to_vec();
            s.refused = refused.to_vec();
        }
    }

    /// Sets the project root for the next calls: pass it only while the
    /// project is trusted (H3.3b).
    pub fn set_project(&self, root: Option<&Path>) {
        if let Ok(mut s) = self.state.lock() {
            s.project = root.and_then(|r| std::fs::canonicalize(r).ok());
        }
    }

    /// Processes asked for since the last call to this.
    pub fn take_spawns(&self) -> Vec<SpawnRequest> {
        self.state
            .lock()
            .map(|mut s| std::mem::take(&mut s.spawns))
            .unwrap_or_default()
    }

    /// Gives process `token`'s outcome to its `on_exit` within one budget.
    pub fn deliver(
        &self,
        token: u64,
        outcome: &ProcessOutcome,
    ) -> Result<Vec<Action>, PluginError> {
        let key = self.state.lock().ok().and_then(|mut s| {
            let index = s.callbacks.iter().position(|(t, _)| *t == token)?;
            Some(s.callbacks.remove(index).1)
        });
        let Some(Some(key)) = key else {
            return Ok(Vec::new());
        };
        let callback: Option<mlua::Function> = self.lua.registry_value(&key).ok();
        let _ = self.lua.remove_registry_value(key);
        let Some(callback) = callback else {
            return Ok(Vec::new());
        };
        self.begin(None, false);
        let result = self.with_budget(|lua| {
            let t = lua.create_table()?;
            t.raw_set("code", outcome.code)?;
            t.raw_set("timed_out", outcome.timed_out)?;
            t.raw_set("stdout", lua.create_string(&outcome.stdout)?)?;
            t.raw_set("stderr", lua.create_string(&outcome.stderr)?)?;
            t.raw_set("error", outcome.error.clone())?;
            callback.call::<()>(t)
        });
        let actions = self.finish();
        result.map(|()| actions)
    }

    /// Whether the plugin provides `kind` for `filetype`.
    pub fn has_provider(&self, kind: &str, filetype: &str) -> bool {
        self.state.lock().is_ok_and(|s| {
            s.providers
                .iter()
                .any(|(k, fts, _)| k == kind && fts.iter().any(|f| f == "*" || f == filetype))
        })
    }

    /// Whether the plugin handles `document.before_save`.
    pub fn has_before_save(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|s| s.handlers.iter().any(|(e, _)| e == "document.before_save"))
    }

    /// Asks the plugin's first `kind` provider for `doc.filetype` within one
    /// budget; `None` = no answer (built-in providers take over).
    pub fn provide(
        &self,
        kind: &str,
        doc: &DocInfo,
    ) -> Result<(Option<Answer>, Vec<Action>), PluginError> {
        let run: Option<mlua::Function> = self.state.lock().ok().and_then(|s| {
            s.providers
                .iter()
                .find(|(k, fts, _)| k == kind && fts.iter().any(|f| f == "*" || *f == doc.filetype))
                .and_then(|(_, _, key)| self.lua.registry_value(key).ok())
        });
        let Some(run) = run else {
            return Ok((None, Vec::new()));
        };
        self.begin(None, false);
        let result = self.with_budget(|lua| {
            let ctx = lua.create_table()?;
            ctx.raw_set("path", doc.path.as_str())?;
            ctx.raw_set("filetype", doc.filetype.as_str())?;
            ctx.raw_set("line", doc.line)?;
            ctx.raw_set("column", doc.column)?;
            ctx.raw_set("text", doc.text.as_str())?;
            ctx.raw_set("word", doc.word.as_str())?;
            let value: mlua::Value = run.call(ctx)?;
            answer(kind, value)
        });
        let actions = self.finish();
        result.map(|a| (a, actions))
    }

    /// Runs the `document.before_save` handlers in order within one
    /// budget; each may return a new text for the next. `None` = unchanged.
    pub fn before_save(&self, doc: &DocInfo) -> Result<(Option<String>, Vec<Action>), PluginError> {
        let handlers: Vec<mlua::Function> = self
            .state
            .lock()
            .map(|s| {
                s.handlers
                    .iter()
                    .filter(|(e, _)| e == "document.before_save")
                    .filter_map(|(_, key)| self.lua.registry_value(key).ok())
                    .collect()
            })
            .unwrap_or_default();
        if handlers.is_empty() {
            return Ok((None, Vec::new()));
        }
        self.begin(None, false);
        let result = self.with_budget(|lua| {
            let mut text: Option<String> = None;
            for handler in &handlers {
                let arg = lua.create_table()?;
                arg.raw_set("path", doc.path.as_str())?;
                arg.raw_set("filetype", doc.filetype.as_str())?;
                arg.raw_set("text", text.as_deref().unwrap_or(&doc.text))?;
                let value: mlua::Value = handler.call(arg)?;
                if let Some(Answer::Text(new)) = answer("format", value)? {
                    text = Some(new);
                }
            }
            Ok(text)
        });
        let actions = self.finish();
        result.map(|t| (t, actions))
    }

    /// The commands the plugin added while loading.
    pub fn commands(&self) -> Vec<PluginCommand> {
        self.state
            .lock()
            .map(|s| s.commands.iter().map(|(c, _)| c.clone()).collect())
            .unwrap_or_default()
    }

    /// Prepares a call: the document snapshot, no actions yet.
    fn begin(&self, doc: Option<DocInfo>, in_command: bool) {
        if let Ok(mut s) = self.state.lock() {
            s.doc = doc;
            s.in_command = in_command;
            s.actions.clear();
            s.edit_bytes = 0;
        }
    }

    /// Ends a call: forgets the document, returns the actions.
    fn finish(&self) -> Vec<Action> {
        self.state
            .lock()
            .map(|mut s| {
                s.doc = None;
                s.in_command = false;
                std::mem::take(&mut s.actions)
            })
            .unwrap_or_default()
    }

    /// Runs the handlers of `event` within one budget. Handlers see the
    /// document's path, filetype and cursor, never its text.
    pub fn emit(&self, event: &str, doc: Option<&DocInfo>) -> Result<Vec<Action>, PluginError> {
        let handlers: Vec<mlua::Function> = self
            .state
            .lock()
            .map(|s| {
                s.handlers
                    .iter()
                    .filter(|(e, _)| e == event)
                    .filter_map(|(_, key)| self.lua.registry_value(key).ok())
                    .collect()
            })
            .unwrap_or_default();
        if handlers.is_empty() {
            return Ok(Vec::new());
        }
        let doc = doc.map(|d| DocInfo {
            text: String::new(),
            selection: String::new(),
            ..d.clone()
        });
        self.begin(doc.clone(), false);
        let result = self.with_budget(|lua| {
            for handler in &handlers {
                let arg = match &doc {
                    Some(d) => {
                        let t = lua.create_table()?;
                        t.raw_set("path", d.path.as_str())?;
                        t.raw_set("filetype", d.filetype.as_str())?;
                        mlua::Value::Table(t)
                    }
                    None => mlua::Value::Nil,
                };
                handler.call::<()>(arg)?;
            }
            Ok(())
        });
        let actions = self.finish();
        result.map(|()| actions)
    }

    /// Runs command `id` (a full id from [`Plugin::commands`]) within one
    /// budget; it may read and edit `doc`.
    pub fn run_command(&self, id: &str, doc: Option<DocInfo>) -> Result<Vec<Action>, PluginError> {
        let run: Option<mlua::Function> = self.state.lock().ok().and_then(|s| {
            s.commands
                .iter()
                .find(|(c, _)| c.id == id)
                .and_then(|(_, key)| self.lua.registry_value(key).ok())
        });
        let Some(run) = run else {
            return Err(PluginError::Runtime("unknown command".to_owned()));
        };
        self.begin(doc, true);
        let result = self.with_budget(|_| run.call::<()>(()));
        let actions = self.finish();
        result.map(|()| actions)
    }
}
