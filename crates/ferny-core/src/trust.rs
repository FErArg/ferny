//! Workspace trust (H2.1, threats T1–T3, decision R1).
//!
//! Folders are untrusted until the user trusts them or a parent folder.
//! Everything that would run project content asks [`TrustStore::is_trusted`]
//! first. The store is `trust.toml` in the user config directory: written
//! only by Ferny (owner-only, atomic) and read strictly; any doubt means
//! "not trusted".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fsio::{self, FileMode};
use crate::session::create_private_dir;

/// Largest `trust.toml` read, in bytes.
pub const MAX_TRUST_BYTES: u64 = 64 * 1024;
/// Most trusted folders kept.
pub const MAX_TRUSTED: usize = 256;

const VERSION: u32 = 1;

/// Why a folder could not be trusted or the store saved. No paths inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TrustError {
    /// Missing, not a folder, or not resolvable.
    #[error("not a folder")]
    NotAFolder,
    /// The file-system root can never be trusted.
    #[error("the root folder cannot be trusted")]
    Root,
    /// `MAX_TRUSTED` folders are already trusted.
    #[error("too many trusted folders")]
    Full,
    /// The store could not be written.
    #[error("I/O error")]
    Io,
}

/// Trusted folders, canonical, in the order they were added.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustStore {
    folders: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustFile {
    version: u32,
    #[serde(default)]
    folders: Vec<String>,
}

impl TrustStore {
    /// Reads `path`. A missing file is an empty store; an invalid one too
    /// (fail closed), reported as `false`.
    pub fn load(path: &Path) -> (Self, bool) {
        let Ok(file) = fsio::read_limited(path, MAX_TRUST_BYTES) else {
            return (Self::default(), !path.exists());
        };
        match Self::parse(&file.bytes) {
            Some(store) => (store, true),
            None => (Self::default(), false),
        }
    }

    /// Renames an invalid store at `path` to `<name>.invalid-<seconds>`
    /// (or `-1` … `-9` if taken), so a new store can be written without
    /// losing the old bytes (S2). Never deletes; returns the new path.
    pub fn set_invalid_aside(path: &Path, unix_seconds: u64) -> Result<PathBuf, TrustError> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(TrustError::Io)?;
        let dir = path.parent().ok_or(TrustError::Io)?;
        for attempt in 0..10u32 {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!("-{attempt}")
            };
            let target = dir.join(format!("{name}.invalid-{unix_seconds}{suffix}"));
            if std::fs::symlink_metadata(&target).is_ok() {
                continue;
            }
            std::fs::rename(path, &target).map_err(|_| TrustError::Io)?;
            return Ok(target);
        }
        Err(TrustError::Io)
    }

    /// Parses a store; `None` if anything is wrong with it.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        let parsed: TrustFile = toml::from_str(text).ok()?;
        if parsed.version != VERSION || parsed.folders.len() > MAX_TRUSTED {
            return None;
        }
        let valid = |p: &String| !p.chars().any(char::is_control) && Path::new(p).is_absolute();
        if !parsed.folders.iter().all(valid) {
            return None;
        }
        Some(Self {
            folders: parsed.folders.into_iter().map(PathBuf::from).collect(),
        })
    }

    /// Writes the store to `path` (owner-only, atomic).
    pub fn save(&self, path: &Path) -> Result<(), TrustError> {
        let file = TrustFile {
            version: VERSION,
            folders: self
                .folders
                .iter()
                .filter_map(|p| p.to_str().map(str::to_owned))
                .collect(),
        };
        let text = toml::to_string(&file).map_err(|_| TrustError::Io)?;
        if let Some(parent) = path.parent() {
            create_private_dir(parent).map_err(|_| TrustError::Io)?;
        }
        fsio::write_atomic_with_mode(path, text.as_bytes(), None, FileMode::OwnerOnly)
            .map_err(|_| TrustError::Io)?;
        Ok(())
    }

    /// Trusted folders, as stored.
    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    /// Whether `path` (a file or folder) is inside a trusted folder. The
    /// path is resolved first, so links count where they point; anything
    /// unresolvable is untrusted.
    pub fn is_trusted(&self, path: &Path) -> bool {
        let Ok(real) = std::fs::canonicalize(path) else {
            return false;
        };
        // Path::starts_with compares whole components: /src2 is not in /src.
        self.folders.iter().any(|folder| real.starts_with(folder))
    }

    /// Trusts `folder` (canonicalised) and everything below it.
    pub fn trust(&mut self, folder: &Path) -> Result<PathBuf, TrustError> {
        let real = std::fs::canonicalize(folder).map_err(|_| TrustError::NotAFolder)?;
        if !real.is_dir() {
            return Err(TrustError::NotAFolder);
        }
        if real.parent().is_none() {
            return Err(TrustError::Root);
        }
        if !self.folders.contains(&real) {
            if self.folders.len() >= MAX_TRUSTED {
                return Err(TrustError::Full);
            }
            self.folders.push(real.clone());
        }
        Ok(real)
    }

    /// Stops trusting `folder` (as listed). True if it was there.
    pub fn revoke(&mut self, folder: &Path) -> bool {
        let before = self.folders.len();
        self.folders.retain(|f| f != folder);
        self.folders.len() != before
    }
}
