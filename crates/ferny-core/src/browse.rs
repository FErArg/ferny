//! Directory listings for the Files sidebar (H1.6a).
//!
//! Names come from whatever folder the user browses, so they are untrusted
//! (threats T6, T7): listings are bounded, symbolic links are reported but
//! never entered, and names are shown through [`display_name`], which makes
//! control and bidirectional formatting characters visible.

use std::path::{Path, PathBuf};

use crate::editing::{BIDI_CONTROLS, INVISIBLE_CHARACTERS};

/// Most entries listed per directory.
pub const MAX_ENTRIES: usize = 5000;

/// Kind of a listed entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A directory (not a link).
    Directory,
    /// A regular file, or anything else that is not a directory or a link.
    File,
    /// A symbolic link to a directory: listed, never entered.
    DirectoryLink,
    /// A symbolic link to a file or a broken link.
    FileLink,
}

/// One entry of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Full path (the parent joined with the real name).
    pub path: PathBuf,
    /// Name safe to show (see [`display_name`]).
    pub display: String,
    /// What the entry is.
    pub kind: EntryKind,
}

/// A bounded listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// Directories first, then files; each group by case-insensitive name.
    pub entries: Vec<Entry>,
    /// Entries not shown: past `MAX_ENTRIES`, non-UTF-8 names or unreadable.
    /// Hidden files left out on purpose are not counted.
    pub omitted: usize,
}

/// Why a directory could not be listed. No paths inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BrowseError {
    /// Missing, not a directory, or not readable.
    #[error("cannot list the folder")]
    Unreadable,
}

/// Whether `name` is hidden (a dot file, as Geany and Unix consider it).
pub fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// `name` with every control, bidirectional or invisible formatting
/// character replaced by a visible `\u{XXXX}` escape (threat T7).
pub fn display_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let deceptive = c.is_control()
            || BIDI_CONTROLS.iter().any(|(b, _)| *b == c)
            || INVISIBLE_CHARACTERS.iter().any(|(i, _)| *i == c);
        if deceptive {
            out.push_str(&format!("\\u{{{:04X}}}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// Lists `dir` (not recursively). Symbolic links are classified by their
/// target but reported as links; `dir` itself must be a real directory.
pub fn list(dir: &Path, show_hidden: bool) -> Result<Listing, BrowseError> {
    let meta = std::fs::symlink_metadata(dir).map_err(|_| BrowseError::Unreadable)?;
    if !meta.is_dir() {
        return Err(BrowseError::Unreadable);
    }
    let reader = std::fs::read_dir(dir).map_err(|_| BrowseError::Unreadable)?;
    let mut listing = Listing::default();
    for item in reader {
        let Ok(item) = item else {
            listing.omitted += 1;
            continue;
        };
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            listing.omitted += 1;
            continue;
        };
        if !show_hidden && is_hidden(&name) {
            continue;
        }
        if listing.entries.len() >= MAX_ENTRIES {
            listing.omitted += 1;
            continue;
        }
        let path = item.path();
        let Ok(file_type) = item.file_type() else {
            listing.omitted += 1;
            continue;
        };
        let kind = if file_type.is_symlink() {
            // Only the target's kind is read; the link is never entered.
            if std::fs::metadata(&path).is_ok_and(|m| m.is_dir()) {
                EntryKind::DirectoryLink
            } else {
                EntryKind::FileLink
            }
        } else if file_type.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        };
        listing.entries.push(Entry {
            display: display_name(&name),
            path,
            kind,
        });
    }
    listing.entries.sort_by(|a, b| {
        let dir = |e: &Entry| !matches!(e.kind, EntryKind::Directory | EntryKind::DirectoryLink);
        dir(a)
            .cmp(&dir(b))
            .then_with(|| a.display.to_lowercase().cmp(&b.display.to_lowercase()))
            .then_with(|| a.display.cmp(&b.display))
    });
    Ok(listing)
}
