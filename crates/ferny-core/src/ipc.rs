//! Single-instance requests (threat T8, decision R3).
//!
//! A second invocation forwards the files it was given to the running
//! instance. The only request is "open these files"; it is validated as a
//! whole before anything happens, and opening then goes through the normal
//! checks. The transport (`QLocalServer`) lives in the UI; the endpoint
//! directory and socket are checked here.
//!
//! Wire format (UTF-8, `\n`-terminated lines):
//!
//! ```text
//! ferny-open 1
//! <line>\t<column>\t<absolute path>
//! ...
//! ```
//!
//! `0` means "no position". Decimal fields accept leading zeros and are
//! normalized when re-encoded. Framing (a 4-byte length prefix) is done by
//! the transport, which also enforces [`MAX_REQUEST_BYTES`].

#[cfg(unix)]
use std::fs;
use std::io;
use std::path::Path;

use crate::fsio::{self, PrivateDirError};
use crate::location::Location;

/// Largest request accepted, in bytes.
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Most files in one request.
pub const MAX_REQUEST_FILES: usize = 64;

const HEADER: &str = "ferny-open 1";

/// Why a request was refused. Carries no path, so it is safe to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IpcError {
    /// Larger than [`MAX_REQUEST_BYTES`].
    #[error("request too large")]
    TooLarge,
    /// More than [`MAX_REQUEST_FILES`] files.
    #[error("too many files")]
    TooManyFiles,
    /// Not a version-1 open request.
    #[error("unknown request")]
    UnknownRequest,
    /// A line is not `<line>\t<column>\t<path>`.
    #[error("malformed request")]
    Malformed,
    /// A path is relative, empty or contains control characters.
    #[error("invalid path")]
    InvalidPath,
    /// The endpoint directory is not private to this user.
    #[error("unsafe endpoint directory")]
    UnsafeDirectory,
    /// Any other I/O failure.
    #[error("I/O error")]
    Io,
}

impl From<io::Error> for IpcError {
    fn from(_: io::Error) -> Self {
        Self::Io
    }
}

/// Encodes an open request. Paths must be absolute and printable.
pub fn encode_request(locations: &[Location]) -> Result<Vec<u8>, IpcError> {
    if locations.len() > MAX_REQUEST_FILES {
        return Err(IpcError::TooManyFiles);
    }
    let mut out = String::from(HEADER);
    out.push('\n');
    for location in locations {
        let path = location.path.to_str().ok_or(IpcError::InvalidPath)?;
        if !valid_path(path) {
            return Err(IpcError::InvalidPath);
        }
        out.push_str(&format!(
            "{}\t{}\t{}\n",
            location.line.unwrap_or(0),
            location.column.unwrap_or(0),
            path
        ));
    }
    if out.len() > MAX_REQUEST_BYTES {
        return Err(IpcError::TooLarge);
    }
    Ok(out.into_bytes())
}

/// Decodes and validates an open request; all or nothing.
pub fn decode_request(bytes: &[u8]) -> Result<Vec<Location>, IpcError> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(IpcError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| IpcError::Malformed)?;
    let body = text.strip_suffix('\n').ok_or(IpcError::Malformed)?;
    let mut lines = body.split('\n');
    if lines.next() != Some(HEADER) {
        return Err(IpcError::UnknownRequest);
    }
    let mut locations = Vec::new();
    for line in lines {
        if locations.len() == MAX_REQUEST_FILES {
            return Err(IpcError::TooManyFiles);
        }
        let mut fields = line.splitn(3, '\t');
        let (Some(row), Some(column), Some(path)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(IpcError::Malformed);
        };
        let row = number(row)?;
        let column = number(column)?;
        if !valid_path(path) {
            return Err(IpcError::InvalidPath);
        }
        locations.push(Location {
            path: path.into(),
            line: (row > 0).then_some(row),
            column: (column > 0).then_some(column),
        });
    }
    Ok(locations)
}

fn number(text: &str) -> Result<u32, IpcError> {
    if text.is_empty() || text.len() > 10 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(IpcError::Malformed);
    }
    text.parse().map_err(|_| IpcError::Malformed)
}

fn valid_path(path: &str) -> bool {
    !path.is_empty() && !path.chars().any(char::is_control) && Path::new(path).is_absolute()
}

/// Longest Unix socket path used, in bytes (the system limit is 104 on
/// macOS and 108 on Linux, including the terminating NUL).
pub const MAX_SOCKET_PATH_BYTES: usize = 100;

/// Whether `socket` is short enough to be a Unix socket path. When it is
/// not, the instance runs without single-instance support.
pub fn socket_path_fits(socket: &Path) -> bool {
    socket.as_os_str().len() <= MAX_SOCKET_PATH_BYTES
}

/// State of the endpoint (Unix socket) before connecting or listening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointState {
    /// Nothing there: listen.
    Absent,
    /// A socket owned by this user: connect, or remove it if stale.
    Ours,
    /// Anything else (another user's socket, a symlink, a regular file):
    /// neither connect nor listen; run a separate instance (R3).
    Foreign,
}

/// Creates the private endpoint directory if needed and checks that it is
/// a real directory owned by this user and closed to others (0700). On
/// non-Unix platforms there is nothing to check (named pipes).
pub fn prepare_directory(dir: &Path) -> Result<(), IpcError> {
    fsio::private_directory(dir).map_err(|err| match err {
        PrivateDirError::Unsafe => IpcError::UnsafeDirectory,
        PrivateDirError::Io => IpcError::Io,
    })
}

/// What is at `socket` (inside a directory checked by
/// [`prepare_directory`]).
pub fn endpoint_state(socket: &Path) -> EndpointState {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let metadata = match fs::symlink_metadata(socket) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return EndpointState::Absent,
            Err(_) => return EndpointState::Foreign,
        };
        let ours = socket
            .parent()
            .and_then(|dir| fsio::current_uid(dir).ok())
            .is_some_and(|uid| uid == metadata.uid());
        if metadata.file_type().is_socket() && ours {
            EndpointState::Ours
        } else {
            EndpointState::Foreign
        }
    }
    #[cfg(not(unix))]
    {
        let _ = socket;
        EndpointState::Absent
    }
}
