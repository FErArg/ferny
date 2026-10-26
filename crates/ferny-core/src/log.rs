//! Local log (H3.2, threats T10, T17): `ferny.log` in the user's
//! configuration directory, owner-only, rotated at `MAX_LOG_BYTES` into one
//! older file. It records events (a plugin loaded, a permission used, a
//! program started, an internal error), never file or clipboard contents,
//! environment variables or tokens: callers pass short event texts, and
//! every message is escaped and cut at `MAX_MESSAGE_BYTES`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Size at which the log is rotated, in bytes.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// Longest message kept, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 512;

static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Starts logging to `path` (an empty path stops logging).
pub fn init(path: &Path) {
    if let Ok(mut log) = LOG.lock() {
        *log = (!path.as_os_str().is_empty()).then(|| path.to_path_buf());
    }
}

/// One line: escaped, bounded, single-line.
pub fn line(component: &str, message: &str, unix_seconds: u64) -> String {
    let mut text = String::new();
    for c in message.chars() {
        if c.is_control() || crate::browse::display_name(&c.to_string()) != c.to_string() {
            text.push_str(&format!("\\u{{{:04X}}}", u32::from(c)));
        } else {
            text.push(c);
        }
        if text.len() >= MAX_MESSAGE_BYTES {
            let mut end = MAX_MESSAGE_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            break;
        }
    }
    let component: String = component
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
        .take(64)
        .collect();
    format!("{} {component}: {text}\n", timestamp(unix_seconds))
}

/// `YYYY-MM-DDTHH:MM:SSZ` of a Unix time (UTC).
pub(crate) fn timestamp(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Appends an event; failures are ignored (logging never breaks Ferny).
pub fn write(component: &str, message: &str) {
    let Ok(log) = LOG.lock() else {
        return;
    };
    let Some(path) = log.as_ref() else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let _ = append(path, &line(component, message, now));
}

fn append(path: &Path, line: &str) -> std::io::Result<()> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let mut older = path.as_os_str().to_owned();
        older.push(".1");
        std::fs::rename(path, older)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Owner-only; never follow a link planted at the log's path.
        options.mode(0o600).custom_flags(libc_nofollow());
    }
    let mut file = options.open(path)?;
    file.write_all(line.as_bytes())
}

#[cfg(unix)]
fn libc_nofollow() -> i32 {
    // O_NOFOLLOW: 0x100 on macOS and the BSDs, 0o400000 on Linux.
    if cfg!(target_os = "linux") {
        0o400_000
    } else {
        0x100
    }
}
