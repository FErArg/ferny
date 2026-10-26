//! Session: open files with cursor positions, active tab, window geometry
//! and dock layout, recent files and the Scribble notes, stored in
//! `session.toml` (threat T10).
//!
//! The file is untrusted when read (it could have been copied or edited):
//! size-limited, parsed strictly, sanitised entry by entry. It is written
//! atomically with owner-only permissions.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fsio::{self, FileMode};
use crate::location::Location;

/// Largest session file read, in bytes: room for the files, the recent
/// list and a Scribble whose characters all need escaping (H1.6a).
pub const MAX_SESSION_BYTES: u64 = 8 * 1024 * 1024;
/// Most files restored.
pub const MAX_SESSION_FILES: usize = 256;
/// Recent files remembered (maintainer decision, H1.2).
pub const MAX_RECENT_FILES: usize = 10;
/// Closed tabs that can be reopened (maintainer decision, H1.2).
pub const MAX_CLOSED_TABS: usize = 10;
/// Largest window-geometry (and dock-layout) blob kept, in bytes.
pub const MAX_GEOMETRY_BYTES: usize = 4096;
/// Largest Scribble text kept, in UTF-8 bytes (maintainer decision, H1.6a).
pub const MAX_SCRIBBLE_BYTES: usize = 1024 * 1024;

const VERSION: u32 = 1;

/// Why the session could not be read or written. No paths inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// No session file yet.
    #[error("no session")]
    NotFound,
    /// Too large, not UTF-8, not TOML or of an unknown version.
    #[error("invalid session file")]
    Invalid,
    /// Any other I/O failure.
    #[error("I/O error")]
    Io,
}

/// A bounded most-recent-first list of paths without duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentList<T> {
    items: VecDeque<T>,
    capacity: usize,
}

impl<T: PartialEq> RecentList<T> {
    /// An empty list holding at most `capacity` items.
    pub fn new(capacity: usize) -> Self {
        Self {
            items: VecDeque::new(),
            capacity,
        }
    }

    /// Puts `item` first, removing an equal one and the oldest if full.
    pub fn push(&mut self, item: T) {
        self.items.retain(|existing| *existing != item);
        self.items.push_front(item);
        self.items.truncate(self.capacity);
    }

    /// Removes and returns the most recent item.
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop_front()
    }

    /// Items, most recent first.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// What is saved between runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Open files, in tab order.
    pub files: Vec<Location>,
    /// Index of the active tab in `files`.
    pub active: usize,
    /// Opaque window geometry from the UI.
    pub geometry: Vec<u8>,
    /// Opaque dock layout from the UI.
    pub layout: Vec<u8>,
    /// Recent files.
    pub recent: RecentList<PathBuf>,
    /// Scribble notes (at most `MAX_SCRIBBLE_BYTES`).
    pub scribble: String,
    /// Root of the project open at quit (H2.5a), reopened at start.
    pub project: Option<PathBuf>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            active: 0,
            geometry: Vec::new(),
            layout: Vec::new(),
            recent: RecentList::new(MAX_RECENT_FILES),
            scribble: String::new(),
            project: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SessionFile {
    version: u32,
    #[serde(default)]
    active: usize,
    #[serde(default)]
    geometry: String,
    #[serde(default)]
    layout: String,
    #[serde(default)]
    scribble: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    files: Vec<FileEntry>,
    #[serde(default)]
    recent: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct FileEntry {
    path: String,
    #[serde(default)]
    line: u32,
    #[serde(default)]
    column: u32,
}

/// Reads the session at `path`. Invalid entries are dropped; an invalid
/// file as a whole is an error (the caller starts with an empty session).
pub fn load(path: &Path) -> Result<Session, SessionError> {
    let file = fsio::read_limited(path, MAX_SESSION_BYTES).map_err(|err| match err {
        fsio::ReadError::NotFound => SessionError::NotFound,
        fsio::ReadError::TooLarge | fsio::ReadError::NotAFile => SessionError::Invalid,
        _ => SessionError::Io,
    })?;
    parse(&file.bytes)
}

/// Parses and sanitises the contents of a session file.
pub fn parse(bytes: &[u8]) -> Result<Session, SessionError> {
    if bytes.len() as u64 > MAX_SESSION_BYTES {
        return Err(SessionError::Invalid);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| SessionError::Invalid)?;
    let parsed: SessionFile = toml::from_str(text).map_err(|_| SessionError::Invalid)?;
    if parsed.version != VERSION {
        return Err(SessionError::Invalid);
    }

    let files: Vec<Location> = parsed
        .files
        .into_iter()
        .filter(|entry| valid_path(&entry.path))
        .take(MAX_SESSION_FILES)
        .map(|entry| Location {
            path: PathBuf::from(entry.path),
            line: (entry.line > 0).then_some(entry.line),
            column: (entry.column > 0).then_some(entry.column),
        })
        .collect();
    let mut recent = RecentList::new(MAX_RECENT_FILES);
    let kept: Vec<String> = parsed
        .recent
        .into_iter()
        .filter(|p| valid_path(p))
        .take(MAX_RECENT_FILES)
        .collect();
    // Stored most recent first; pushing oldest first restores the order.
    for path in kept.into_iter().rev() {
        recent.push(PathBuf::from(path));
    }
    let geometry = decode_hex(&parsed.geometry)
        .filter(|bytes| bytes.len() <= MAX_GEOMETRY_BYTES)
        .unwrap_or_default();
    let layout = decode_hex(&parsed.layout)
        .filter(|bytes| bytes.len() <= MAX_GEOMETRY_BYTES)
        .unwrap_or_default();
    let active = if parsed.active < files.len() {
        parsed.active
    } else {
        0
    };
    Ok(Session {
        files,
        active,
        geometry,
        layout,
        recent,
        scribble: truncate_scribble(parsed.scribble),
        project: valid_path(&parsed.project).then(|| PathBuf::from(parsed.project)),
    })
}

/// Cuts `text` to at most `MAX_SCRIBBLE_BYTES`, on a character boundary.
pub fn truncate_scribble(mut text: String) -> String {
    if text.len() > MAX_SCRIBBLE_BYTES {
        let mut end = MAX_SCRIBBLE_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// Writes the session to `path` (owner-only, atomic), creating private
/// parent directories as needed.
pub fn save(path: &Path, session: &Session) -> Result<(), SessionError> {
    let to_string = |p: &Path| p.to_str().filter(|s| valid_path(s)).map(str::to_owned);
    let file = SessionFile {
        version: VERSION,
        active: session.active,
        geometry: encode_hex(&session.geometry[..session.geometry.len().min(MAX_GEOMETRY_BYTES)]),
        layout: encode_hex(&session.layout[..session.layout.len().min(MAX_GEOMETRY_BYTES)]),
        scribble: truncate_scribble(session.scribble.clone()),
        project: session
            .project
            .as_deref()
            .and_then(to_string)
            .unwrap_or_default(),
        files: session
            .files
            .iter()
            .take(MAX_SESSION_FILES)
            .filter_map(|location| {
                Some(FileEntry {
                    path: to_string(&location.path)?,
                    line: location.line.unwrap_or(0),
                    column: location.column.unwrap_or(0),
                })
            })
            .collect(),
        recent: session.recent.iter().filter_map(|p| to_string(p)).collect(),
    };
    let text = toml::to_string(&file).map_err(|_| SessionError::Invalid)?;
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    fsio::write_atomic_with_mode(path, text.as_bytes(), None, FileMode::OwnerOnly)
        .map_err(|_| SessionError::Io)?;
    Ok(())
}

pub(crate) fn create_private_dir(dir: &Path) -> Result<(), SessionError> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    crate::fsio::private_directory(dir).map_err(|_| SessionError::Io)
}

fn valid_path(path: &str) -> bool {
    !path.is_empty() && !path.chars().any(char::is_control) && Path::new(path).is_absolute()
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > MAX_GEOMETRY_BYTES * 2 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
