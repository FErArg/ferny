//! Directory listings for the Files sidebar (threats T6 and T7).

mod common;

use std::fs;

use common::TempDir;
use ferny_core::browse::{self, BrowseError, EntryKind, MAX_ENTRIES, display_name};

fn names(listing: &browse::Listing) -> Vec<&str> {
    listing.entries.iter().map(|e| e.display.as_str()).collect()
}

#[test]
fn hidden_files_and_order() {
    let dir = TempDir::new("browse-order");
    dir.write("b.txt", b"");
    dir.write("A.txt", b"");
    dir.write(".hidden", b"");
    fs::create_dir(dir.0.join("zdir")).expect("mkdir");
    fs::create_dir(dir.0.join(".git")).expect("mkdir");
    let listing = browse::list(&dir.0, false).expect("list");
    assert_eq!(names(&listing), ["zdir", "A.txt", "b.txt"]);
    assert_eq!(listing.omitted, 0);
    assert_eq!(listing.entries[0].kind, EntryKind::Directory);
    assert_eq!(listing.entries[1].path, dir.0.join("A.txt"));

    let all = browse::list(&dir.0, true).expect("list");
    assert_eq!(
        names(&all),
        [".git", "zdir", ".editorconfig", ".hidden", "A.txt", "b.txt"]
    );
}

#[test]
fn display_name_escapes_controls() {
    assert_eq!(display_name("plain ñ.rs"), "plain ñ.rs");
    assert_eq!(display_name("a\nb"), "a\\u{000A}b");
    assert_eq!(display_name("evil\u{202E}txt.exe"), "evil\\u{202E}txt.exe");
    assert_eq!(display_name("zero\u{200B}width"), "zero\\u{200B}width");
    assert_eq!(display_name("\u{1b}[31m"), "\\u{001B}[31m");
}

#[cfg(unix)]
#[test]
fn deceptive_names_keep_their_real_path() {
    let dir = TempDir::new("browse-deceptive");
    let real = "evil\u{202E}txt.sh";
    dir.write(real, b"");
    let listing = browse::list(&dir.0, false).expect("list");
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].display, "evil\\u{202E}txt.sh");
    assert_eq!(listing.entries[0].path, dir.0.join(real));
}

#[test]
fn listing_is_bounded() {
    let dir = TempDir::new("browse-bounded");
    for i in 0..MAX_ENTRIES + 7 {
        dir.write(&format!("f{i:05}"), b"");
    }
    let listing = browse::list(&dir.0, false).expect("list");
    assert_eq!(listing.entries.len(), MAX_ENTRIES);
    assert_eq!(listing.omitted, 7);
}

#[test]
fn errors_carry_no_path() {
    let dir = TempDir::new("browse-errors");
    let file = dir.write("file.txt", b"");
    assert_eq!(browse::list(&file, false), Err(BrowseError::Unreadable));
    assert_eq!(
        browse::list(&dir.0.join("missing"), false),
        Err(BrowseError::Unreadable)
    );
    assert_eq!(
        BrowseError::Unreadable.to_string(),
        "cannot list the folder"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new("browse-links");
    fs::create_dir(dir.0.join("real")).expect("mkdir");
    dir.write("target.txt", b"");
    symlink(dir.0.join("real"), dir.0.join("dirlink")).expect("link");
    symlink(dir.0.join("target.txt"), dir.0.join("filelink")).expect("link");
    symlink(dir.0.join("missing"), dir.0.join("broken")).expect("link");
    // A loop: listing never descends, so it terminates.
    symlink(&dir.0, dir.0.join("real").join("loop")).expect("link");
    let listing = browse::list(&dir.0, false).expect("list");
    let kind = |name: &str| {
        listing
            .entries
            .iter()
            .find(|e| e.display == name)
            .map(|e| e.kind)
    };
    assert_eq!(kind("dirlink"), Some(EntryKind::DirectoryLink));
    assert_eq!(kind("filelink"), Some(EntryKind::FileLink));
    assert_eq!(kind("broken"), Some(EntryKind::FileLink));
    assert_eq!(kind("real"), Some(EntryKind::Directory));
    // The listed folder itself must not be a link.
    assert_eq!(
        browse::list(&dir.0.join("dirlink"), false),
        Err(BrowseError::Unreadable)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_names_are_skipped() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let dir = TempDir::new("browse-non-utf8");
    fs::write(dir.0.join(OsStr::from_bytes(b"bad\xff.txt")), b"").expect("write");
    dir.write("good.txt", b"");
    let listing = browse::list(&dir.0, false).expect("list");
    assert_eq!(names(&listing), ["good.txt"]);
    assert_eq!(listing.omitted, 1);
}
