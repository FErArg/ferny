//! Bounded reads, file stamps and atomic saves (threats T6 and T9).
//!
//! Saving writes a temporary file in the target's own directory (created
//! exclusively, so it never follows a planted file or symlink), copies the
//! original permissions, syncs it and renames it over the target, then
//! syncs the directory. Files with other hard links, or owned by another
//! user, are written in place instead, because a rename would split the
//! links or change the owner; that path is not atomic.

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Hard upper bound for a document read into memory, in bytes
/// (maintainer decision, H1.1).
pub const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;

/// Documents larger than this are "large": heavy features such as syntax
/// highlighting are turned off (maintainer decision, H1.1).
pub const LARGE_DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;

/// Attempts at finding an unused temporary file name.
const TEMP_ATTEMPTS: u32 = 16;

/// Identity and version of a file on disk, used to notice changes made by
/// other programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

impl FileStamp {
    /// The stamp of `metadata`.
    pub fn from_metadata(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            dev: metadata.dev(),
            #[cfg(unix)]
            ino: metadata.ino(),
        }
    }

    /// Size in bytes when the stamp was taken.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the file was empty when the stamp was taken.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// State of a file compared with a stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalState {
    /// Same size, modification time and identity.
    Unchanged,
    /// Changed or replaced by another program.
    Modified,
    /// No longer exists.
    Deleted,
}

/// Compares the file at `path` with `stamp`.
pub fn external_state(path: &Path, stamp: &FileStamp) -> ExternalState {
    match fs::metadata(path) {
        Ok(metadata) if FileStamp::from_metadata(&metadata) == *stamp => ExternalState::Unchanged,
        Ok(_) => ExternalState::Modified,
        Err(err) if err.kind() == io::ErrorKind::NotFound => ExternalState::Deleted,
        Err(_) => ExternalState::Modified,
    }
}

/// Why a file could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// The path does not exist.
    #[error("file not found")]
    NotFound,
    /// The operating system refused access.
    #[error("permission denied")]
    PermissionDenied,
    /// Directory, device, FIFO, socket or other non-regular file.
    #[error("not a regular file")]
    NotAFile,
    /// Larger than the limit.
    #[error("file exceeds the size limit")]
    TooLarge,
    /// Any other I/O failure.
    #[error("I/O error ({0:?})")]
    Io(io::ErrorKind),
}

impl From<io::Error> for ReadError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            kind => Self::Io(kind),
        }
    }
}

/// Bytes of a file plus what is known about it.
#[derive(Debug)]
pub struct ReadFile {
    /// The contents.
    pub bytes: Vec<u8>,
    /// Stamp taken on the open handle.
    pub stamp: FileStamp,
    /// Whether the file has no write permission at all.
    pub read_only: bool,
}

/// Reads a regular file of at most `max_bytes` bytes.
///
/// The type is checked before opening, so a FIFO or device never blocks
/// the caller, and again on the open handle. The read itself is bounded,
/// so a file that grows after the check cannot exceed the limit.
pub fn read_limited(path: &Path, max_bytes: u64) -> Result<ReadFile, ReadError> {
    // Follows symlinks on purpose: the target must be a regular file.
    if !fs::metadata(path)?.is_file() {
        return Err(ReadError::NotAFile);
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(ReadError::NotAFile);
    }
    if metadata.len() > max_bytes {
        return Err(ReadError::TooLarge);
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| ReadError::TooLarge)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(ReadError::TooLarge);
    }
    Ok(ReadFile {
        bytes,
        stamp: FileStamp::from_metadata(&metadata),
        read_only: metadata.permissions().readonly(),
    })
}

/// Why a file could not be saved. Nothing was changed on disk unless the
/// variant says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    /// The target (or the directory that should contain it) is missing,
    /// e.g. a symlink to a file that no longer exists.
    #[error("target not found")]
    NotFound,
    /// The target or its directory is not writable.
    #[error("permission denied")]
    PermissionDenied,
    /// The target is a directory or a special file.
    #[error("not a regular file")]
    NotAFile,
    /// The file changed on disk since it was read or last saved.
    #[error("modified by another program")]
    ModifiedExternally,
    /// Any other I/O failure.
    #[error("I/O error ({0:?})")]
    Io(io::ErrorKind),
}

impl From<io::Error> for WriteError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            kind => Self::Io(kind),
        }
    }
}

/// Permissions of a saved file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    /// Keep the permissions of an existing file; new files get the
    /// platform default (umask on Unix).
    Preserve,
    /// Readable and writable by the owner only (0600 on Unix), for files
    /// with private data such as the session. On Windows the file keeps
    /// the ACL inherited from the user's profile directory.
    OwnerOnly,
}

/// Writes `bytes` to `path` safely and returns the new stamp.
///
/// A symlink is followed and its target written (maintainer decision,
/// H1.1); the link is kept. With `expected`, the save is refused if the
/// file no longer matches that stamp (pass `None` to overwrite anyway, or
/// for a new file).
pub fn write_atomic(
    path: &Path,
    bytes: &[u8],
    expected: Option<&FileStamp>,
) -> Result<FileStamp, WriteError> {
    write_atomic_with_mode(path, bytes, expected, FileMode::Preserve)
}

/// [`write_atomic`] with explicit permissions for the result.
pub fn write_atomic_with_mode(
    path: &Path,
    bytes: &[u8],
    expected: Option<&FileStamp>,
    mode: FileMode,
) -> Result<FileStamp, WriteError> {
    let target = resolve_target(path)?;
    let existing = match fs::metadata(&target) {
        Ok(metadata) => Some(metadata),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    if let Some(metadata) = &existing {
        if !metadata.is_file() {
            return Err(WriteError::NotAFile);
        }
        // A rename would replace a read-only file without asking.
        if metadata.permissions().readonly() {
            return Err(WriteError::PermissionDenied);
        }
    }
    if let Some(expected) = expected {
        match &existing {
            Some(metadata) if FileStamp::from_metadata(metadata) == *expected => {}
            _ => return Err(WriteError::ModifiedExternally),
        }
    }

    let directory = match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let (temp_path, mut temp) = create_temp(&directory, &target)?;
    let result = (|| {
        if mode == FileMode::OwnerOnly {
            set_owner_only(&temp)?;
        } else if let Some(metadata) = &existing {
            temp.set_permissions(metadata.permissions())?;
        }
        if let Some(metadata) = &existing
            && must_write_in_place(metadata, &temp.metadata()?)
        {
            return Ok(false);
        }
        temp.write_all(bytes)?;
        temp.sync_all()?;
        Ok::<bool, io::Error>(true)
    })();
    drop(temp);
    match result {
        Ok(true) => {
            if let Err(err) = fs::rename(&temp_path, &target) {
                let _ = fs::remove_file(&temp_path);
                return Err(err.into());
            }
            sync_directory(&directory);
        }
        Ok(false) => {
            let _ = fs::remove_file(&temp_path);
            write_in_place(&target, bytes)?;
        }
        Err(err) => {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
        }
    }
    Ok(FileStamp::from_metadata(&fs::metadata(&target)?))
}

/// The file a save to `path` must write: `path` itself, or the final
/// target of a symlink.
fn resolve_target(path: &Path) -> Result<PathBuf, WriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Ok(fs::canonicalize(path)?),
        Ok(_) => Ok(path.to_path_buf()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(err) => Err(err.into()),
    }
}

/// Creates an unused temporary file next to `target`. `create_new` fails
/// if anything (including a symlink) already has the name.
fn create_temp(directory: &Path, target: &Path) -> Result<(PathBuf, File), WriteError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Keep the name short; it only needs to be unique and recognisable.
    let stem: String = name.chars().take(32).collect();
    for _ in 0..TEMP_ATTEMPTS {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or_default();
        let unique = format!(
            ".{stem}.ferny-{}-{nanos:08x}-{}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let temp_path = directory.join(unique);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    Err(WriteError::Io(io::ErrorKind::AlreadyExists))
}

#[cfg(unix)]
fn set_owner_only(file: &File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_owner_only(_file: &File) -> io::Result<()> {
    Ok(())
}

/// Whether a rename would lose hard links or change the owner.
#[cfg(unix)]
fn must_write_in_place(original: &Metadata, temp: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    original.nlink() > 1 || original.uid() != temp.uid()
}

#[cfg(not(unix))]
fn must_write_in_place(_original: &Metadata, _temp: &Metadata) -> bool {
    false
}

/// Fallback for files that cannot be replaced by a rename. Not atomic: a
/// failure after truncation leaves a partial file.
fn write_in_place(target: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    let mut file = OpenOptions::new().write(true).open(target)?;
    file.set_len(0)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Makes the rename durable. Best effort: not supported on every platform
/// or filesystem.
fn sync_directory(directory: &Path) {
    #[cfg(unix)]
    if let Ok(dir) = File::open(directory) {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = directory;
}

/// Why a private directory could not be used. Carries no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateDirError {
    /// Not a real directory (a link or a file), or not owned by this user.
    Unsafe,
    /// Any other I/O failure.
    Io,
}

impl From<io::Error> for PrivateDirError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}

/// Creates `dir` if needed (0700) and checks that it is a real directory
/// owned by this user and closed to others. A wider mode is corrected to
/// 0700. Links are refused, never followed. On non-Unix platforms only the
/// directory is created (no owner or mode check).
#[cfg(unix)]
pub fn private_directory(dir: &Path) -> Result<(), PrivateDirError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match fs::symlink_metadata(dir) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        Err(err) => return Err(err.into()),
        Ok(_) => {}
    }
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.file_type().is_dir() || metadata.uid() != current_uid(dir)? {
        return Err(PrivateDirError::Unsafe);
    }
    if metadata.mode() & 0o077 != 0 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// See the Unix version.
#[cfg(not(unix))]
pub fn private_directory(dir: &Path) -> Result<(), PrivateDirError> {
    fs::create_dir_all(dir)?;
    Ok(())
}

/// The user id of this process, read from a probe file it creates in `dir`
/// (the standard library has no `getuid`).
#[cfg(unix)]
pub(crate) fn current_uid(dir: &Path) -> io::Result<u32> {
    use std::os::unix::fs::MetadataExt;
    let probe = dir.join(format!(".ferny-probe-{}", std::process::id()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe);
    let result = file.and_then(|f| f.metadata()).map(|m| m.uid());
    let _ = fs::remove_file(&probe);
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::{PrivateDirError, private_directory};
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};

    fn scratch(name: &str) -> PathBuf {
        // Under the git-ignored workspace target directory, not the shared temp dir.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ferny-fsio-tests");
        let dir = root.join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn creates_missing_directory_owner_only() {
        let dir = scratch("create").join("private");
        private_directory(&dir).expect("created");
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn corrects_a_wider_mode() {
        let dir = scratch("widen");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        private_directory(&dir).expect("accepted");
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn refuses_a_link_and_a_file() {
        let base = scratch("links");
        let real = base.join("real");
        std::fs::create_dir_all(&real).expect("mkdir");
        let link = base.join("link");
        symlink(&real, &link).expect("symlink");
        assert_eq!(private_directory(&link), Err(PrivateDirError::Unsafe));
        let file = base.join("file");
        std::fs::write(&file, b"x").expect("write");
        assert_eq!(private_directory(&file), Err(PrivateDirError::Unsafe));
    }
}
