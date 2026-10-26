//! Saving documents: atomic writes, permissions, links, external changes,
//! encodings (threat T9).

mod common;

use std::fs;

use common::TempDir;
use ferny_core::document::{Documents, SaveError};
use ferny_core::fsio::{self, ExternalState, WriteError};

#[test]
fn save_round_trips_encoding_bom_and_eol() {
    let dir = TempDir::new("save-roundtrip");
    let original: &[u8] = b"\xEF\xBB\xBFl\xc3\xadnea 1\r\nl\xc3\xadnea 2\r\n";
    let path = dir.write("bom.txt", original);
    let mut docs = Documents::new();
    let (id, text) = docs.open(&path, None).expect("open");
    docs.save(id, &text, false).expect("save");
    assert_eq!(fs::read(&path).expect("read"), original);

    let latin = dir.write("latin.txt", b"caf\xe9\n");
    let (id, text) = docs.open(&latin, None).expect("open");
    docs.save(id, &format!("{text}ñ\n"), false).expect("save");
    assert_eq!(fs::read(&latin).expect("read"), b"caf\xe9\n\xf1\n");
}

#[test]
fn save_is_atomic_rename_and_leaves_no_temp_files() {
    let dir = TempDir::new("save-atomic");
    let path = dir.write("a.txt", b"old");
    let mut docs = Documents::new();
    let (id, _) = docs.open(&path, None).expect("open");
    docs.save(id, "new", false).expect("save");
    assert_eq!(fs::read(&path).expect("read"), b"new");
    assert_eq!(dir.entries(), ["a.txt"]);
}

#[test]
fn save_refuses_unencodable_characters() {
    let dir = TempDir::new("save-unencodable");
    let path = dir.write("latin.txt", b"caf\xe9\n");
    let mut docs = Documents::new();
    let (id, _) = docs.open(&path, None).expect("open");
    assert_eq!(
        docs.save(id, "café 日本\n", false),
        Err(SaveError::Unencodable)
    );
    assert_eq!(fs::read(&path).expect("read"), b"caf\xe9\n");
    docs.set_encoding(id, "UTF-8").expect("set encoding");
    docs.save(id, "café 日本\n", false).expect("save as UTF-8");
    assert_eq!(fs::read_to_string(&path).expect("read"), "café 日本\n");
}

#[test]
fn save_detects_external_modification() {
    let dir = TempDir::new("save-external");
    let path = dir.write("a.txt", b"one");
    let mut docs = Documents::new();
    let (id, _) = docs.open(&path, None).expect("open");
    assert_eq!(docs.external_state(id), ExternalState::Unchanged);
    fs::write(&path, b"changed by another program").expect("external write");
    assert_eq!(docs.external_state(id), ExternalState::Modified);
    assert_eq!(
        docs.save(id, "mine", false),
        Err(SaveError::ModifiedExternally)
    );
    assert_eq!(
        fs::read(&path).expect("read"),
        b"changed by another program"
    );
    docs.save(id, "mine", true).expect("forced save");
    assert_eq!(fs::read(&path).expect("read"), b"mine");
    assert_eq!(docs.external_state(id), ExternalState::Unchanged);
}

#[test]
fn check_reports_deleted() {
    let dir = TempDir::new("save-deleted");
    let path = dir.write("a.txt", b"one");
    let mut docs = Documents::new();
    let (id, _) = docs.open(&path, None).expect("open");
    fs::remove_file(&path).expect("delete");
    assert_eq!(docs.external_state(id), ExternalState::Deleted);
    assert_eq!(
        docs.save(id, "x", false),
        Err(SaveError::ModifiedExternally)
    );
}

#[test]
fn new_document_needs_save_as() {
    let dir = TempDir::new("save-new");
    let mut docs = Documents::new();
    let id = docs.new_document();
    assert_eq!(docs.save(id, "x", false), Err(SaveError::NoPath));
    let path = dir.0.join("new.txt");
    docs.save_as(id, &path, "hola\n").expect("save as");
    assert_eq!(fs::read(&path).expect("read"), b"hola\n");
    docs.save(id, "chau\n", false).expect("save");
    assert_eq!(fs::read(&path).expect("read"), b"chau\n");
}

#[test]
fn save_refuses_directory_target() {
    let dir = TempDir::new("save-dir");
    fs::create_dir(dir.0.join("sub")).expect("mkdir");
    assert_eq!(
        fsio::write_atomic(&dir.0.join("sub"), b"x", None),
        Err(WriteError::NotAFile)
    );
}

#[test]
fn read_only_document_cannot_be_saved_but_can_be_saved_as() {
    let dir = TempDir::new("save-readonly");
    let path = dir.write("sjis.txt", b"abc\x81\x20def\n");
    let mut docs = Documents::new();
    let (id, text) = docs.open(&path, Some("Shift_JIS")).expect("open");
    assert_eq!(docs.save(id, &text, false), Err(SaveError::ReadOnly));
    assert_eq!(fs::read(&path).expect("read"), b"abc\x81\x20def\n");
    docs.set_encoding(id, "UTF-8").expect("encoding");
    docs.save_as(id, &dir.0.join("copy.txt"), &text)
        .expect("save as");
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    #[test]
    fn save_preserves_mode() {
        let dir = TempDir::new("save-mode");
        let path = dir.write("script.sh", b"#!/bin/sh\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).expect("chmod");
        let mut docs = Documents::new();
        let (id, _) = docs.open(&path, None).expect("open");
        docs.save(id, "#!/bin/sh\necho hi\n", false).expect("save");
        assert_eq!(fs::metadata(&path).expect("meta").mode() & 0o777, 0o750);
    }

    #[test]
    fn save_failure_keeps_original() {
        let dir = TempDir::new("save-failure");
        let path = dir.write("a.txt", b"precious");
        let mut docs = Documents::new();
        let (id, _) = docs.open(&path, None).expect("open");
        // A read-only directory: the temp file cannot be created.
        fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o555)).expect("chmod dir");
        let result = docs.save(id, "lost?", false);
        fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).expect("restore");
        assert_eq!(result, Err(SaveError::PermissionDenied));
        assert_eq!(fs::read(&path).expect("read"), b"precious");
        assert_eq!(dir.entries(), ["a.txt"]);
    }

    #[test]
    fn save_refuses_read_only_file() {
        let dir = TempDir::new("save-ro-file");
        let path = dir.write("ro.txt", b"keep");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).expect("chmod");
        assert_eq!(
            fsio::write_atomic(&path, b"x", None),
            Err(WriteError::PermissionDenied)
        );
        assert_eq!(fs::read(&path).expect("read"), b"keep");
    }

    #[test]
    fn save_through_symlink_writes_target_keeps_link() {
        let dir = TempDir::new("save-symlink");
        let target = dir.write("real.txt", b"old");
        let link = dir.0.join("link.txt");
        symlink(&target, &link).expect("symlink");
        let mut docs = Documents::new();
        let (id, _) = docs.open(&link, None).expect("open");
        docs.save(id, "new", false).expect("save");
        assert!(
            fs::symlink_metadata(&link)
                .expect("meta")
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).expect("read"), b"new");
    }

    #[test]
    fn save_to_dangling_symlink_is_refused() {
        let dir = TempDir::new("save-dangling");
        let link = dir.0.join("link.txt");
        symlink(dir.0.join("missing.txt"), &link).expect("symlink");
        assert_eq!(
            fsio::write_atomic(&link, b"x", None),
            Err(WriteError::NotFound)
        );
        assert!(!dir.0.join("missing.txt").exists());
    }

    #[test]
    fn save_keeps_hard_links() {
        let dir = TempDir::new("save-hardlink");
        let path = dir.write("a.txt", b"old");
        let other = dir.0.join("b.txt");
        fs::hard_link(&path, &other).expect("hard link");
        let mut docs = Documents::new();
        let (id, _) = docs.open(&path, None).expect("open");
        docs.save(id, "new", false).expect("save");
        assert_eq!(fs::read(&other).expect("read"), b"new");
        assert_eq!(fs::metadata(&path).expect("meta").nlink(), 2);
    }

    #[test]
    fn temp_file_never_follows_existing_symlink() {
        // Whatever name the temp file gets, create_new refuses existing
        // entries; here every entry in the directory is a planted symlink
        // pointing outside, and none of them may be written through.
        let dir = TempDir::new("save-planted");
        let outside = TempDir::new("save-planted-outside");
        let victim = outside.write("victim.txt", b"untouched");
        for i in 0..8 {
            symlink(&victim, dir.0.join(format!(".planted-{i}"))).expect("plant");
        }
        let path = dir.write("a.txt", b"old");
        fsio::write_atomic(&path, b"new", None).expect("save");
        assert_eq!(fs::read(&victim).expect("read"), b"untouched");
        assert_eq!(fs::read(&path).expect("read"), b"new");
    }
}
