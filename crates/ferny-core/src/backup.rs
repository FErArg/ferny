//! Backup copies before saving (H4.2a, Geany's Save Actions): the version
//! on disk is copied into the user's data folder, never next to the file,
//! owner-only, and the newest `MAX_COPIES` per file are kept.

use std::path::{Path, PathBuf};

use crate::fsio::{self, FileMode};

/// Copies kept per file.
pub const MAX_COPIES: usize = 5;
/// Largest file copied, in bytes (larger ones are not backed up).
pub const MAX_BACKUP_BYTES: u64 = 64 * 1024 * 1024;

/// Why no copy was made. No paths or contents inside.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackupError {
    /// Nothing on disk yet, or not a regular file.
    #[error("nothing to back up")]
    Nothing,
    /// Too large, unreadable, or the backup folder is unusable.
    #[error("backup failed")]
    Failed,
}

/// FNV-1a of the path: a stable folder name per file.
fn key(path: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Owner-only folder for copies; a link or a foreign owner is refused.
fn private_dir(dir: &Path) -> Result<(), BackupError> {
    fsio::private_directory(dir).map_err(|_| BackupError::Failed)
}

/// Copies `file` as it is on disk into `dir/<key>/`, named after the time
/// and the file name, then keeps the newest `MAX_COPIES`. Returns the copy.
pub fn backup(dir: &Path, file: &Path, unix_seconds: u64) -> Result<PathBuf, BackupError> {
    let meta = std::fs::metadata(file).map_err(|_| BackupError::Nothing)?;
    if !meta.is_file() {
        return Err(BackupError::Nothing);
    }
    let read = fsio::read_limited(file, MAX_BACKUP_BYTES).map_err(|_| BackupError::Failed)?;
    let canonical = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let folder = dir.join(key(&canonical));
    private_dir(dir)?;
    private_dir(&folder)?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = crate::log::timestamp(unix_seconds).replace(':', "");
    // A counter for copies in the same second, padded so names still sort.
    let mut n = 1;
    let mut copy = folder.join(format!("{stamp}-{n:02}-{name}"));
    while std::fs::symlink_metadata(&copy).is_ok() && n < 99 {
        n += 1;
        copy = folder.join(format!("{stamp}-{n:02}-{name}"));
    }
    fsio::write_atomic_with_mode(&copy, &read.bytes, None, FileMode::OwnerOnly)
        .map_err(|_| BackupError::Failed)?;
    // The names start with the time, so they sort oldest first.
    let mut copies: Vec<PathBuf> = std::fs::read_dir(&folder)
        .map_err(|_| BackupError::Failed)?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    copies.sort();
    let excess = copies.len().saturating_sub(MAX_COPIES);
    for old in &copies[..excess] {
        let _ = std::fs::remove_file(old);
    }
    Ok(copy)
}
