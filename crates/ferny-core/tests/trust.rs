//! Workspace trust (threats T1–T3, decision R1).

mod common;

use std::fs;

use common::TempDir;
use ferny_core::trust::{MAX_TRUST_BYTES, TrustError, TrustStore};

#[test]
fn trusting_a_folder_covers_everything_below() {
    let dir = TempDir::new("trust-below");
    let project = dir.0.join("src/project");
    fs::create_dir_all(project.join("deep")).expect("mkdir");
    let file = dir.write("src/project/deep/main.c", b"");
    let mut store = TrustStore::default();
    assert!(!store.is_trusted(&file));
    store.trust(&dir.0.join("src")).expect("trust");
    assert!(store.is_trusted(&file));
    assert!(store.is_trusted(&project));
    assert!(!store.is_trusted(&dir.0));
}

#[test]
fn prefix_is_component_wise() {
    let dir = TempDir::new("trust-prefix");
    fs::create_dir_all(dir.0.join("src")).expect("mkdir");
    fs::create_dir_all(dir.0.join("src2")).expect("mkdir");
    let mut store = TrustStore::default();
    store.trust(&dir.0.join("src")).expect("trust");
    assert!(!store.is_trusted(&dir.0.join("src2")));
}

#[cfg(unix)]
#[test]
fn links_resolve_before_the_check() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new("trust-links");
    fs::create_dir_all(dir.0.join("trusted")).expect("mkdir");
    fs::create_dir_all(dir.0.join("outside")).expect("mkdir");
    // A link inside the trusted folder pointing outside it.
    symlink(dir.0.join("outside"), dir.0.join("trusted/escape")).expect("link");
    // A link outside pointing into it.
    symlink(dir.0.join("trusted"), dir.0.join("into")).expect("link");
    let mut store = TrustStore::default();
    store.trust(&dir.0.join("trusted")).expect("trust");
    assert!(!store.is_trusted(&dir.0.join("trusted/escape")));
    assert!(store.is_trusted(&dir.0.join("into")));
    // Trusting through a link stores the real folder.
    let mut other = TrustStore::default();
    let stored = other.trust(&dir.0.join("into")).expect("trust");
    assert_eq!(
        stored,
        fs::canonicalize(dir.0.join("trusted")).expect("real")
    );
}

#[test]
fn root_cannot_be_trusted() {
    let mut store = TrustStore::default();
    let root = if cfg!(windows) { "C:\\" } else { "/" };
    assert_eq!(
        store.trust(std::path::Path::new(root)),
        Err(TrustError::Root)
    );
    assert!(store.folders().is_empty());
}

#[test]
fn missing_paths_are_untrusted() {
    let dir = TempDir::new("trust-missing");
    let gone = dir.0.join("gone");
    fs::create_dir_all(&gone).expect("mkdir");
    let mut store = TrustStore::default();
    store.trust(&gone).expect("trust");
    fs::remove_dir(&gone).expect("rmdir");
    assert!(!store.is_trusted(&gone));
    assert_eq!(store.folders().len(), 1, "still listed and revocable");
    let file = dir.write("file.txt", b"");
    assert_eq!(store.trust(&file), Err(TrustError::NotAFolder));
    assert_eq!(
        store.trust(&dir.0.join("nope")),
        Err(TrustError::NotAFolder)
    );
}

#[test]
fn save_load_and_revoke() {
    let dir = TempDir::new("trust-store");
    fs::create_dir_all(dir.0.join("a")).expect("mkdir");
    let path = dir.0.join("cfg/trust.toml");
    let mut store = TrustStore::default();
    let real = store.trust(&dir.0.join("a")).expect("trust");
    store.trust(&dir.0.join("a")).expect("again");
    assert_eq!(store.folders().len(), 1, "no duplicates");
    store.save(&path).expect("save");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&path).expect("meta").mode() & 0o777, 0o600);
    }
    let (loaded, ok) = TrustStore::load(&path);
    assert!(ok);
    assert_eq!(loaded, store);
    let mut loaded = loaded;
    assert!(loaded.revoke(&real));
    assert!(!loaded.revoke(&real));
    assert!(!loaded.is_trusted(&dir.0.join("a")));
}

#[test]
fn invalid_store_trusts_nothing() {
    let dir = TempDir::new("trust-invalid");
    let (empty, ok) = TrustStore::load(&dir.0.join("none.toml"));
    assert!(
        ok && empty.folders().is_empty(),
        "missing file: empty, no warning"
    );
    let abs = if cfg!(windows) { "C:\\\\x" } else { "/x" };
    for (name, text) in [
        ("garbage.toml", "not = = toml".to_owned()),
        (
            "version.toml",
            format!("version = 2\nfolders = [\"{abs}\"]\n"),
        ),
        (
            "relative.toml",
            "version = 1\nfolders = [\"src\"]\n".to_owned(),
        ),
        (
            "control.toml",
            format!("version = 1\nfolders = [\"{abs}\\n\"]\n"),
        ),
        (
            "unknown.toml",
            format!("version = 1\nfolders = [\"{abs}\"]\nall = true\n"),
        ),
        (
            "big.toml",
            format!("version = 1\n#{}\n", "x".repeat(MAX_TRUST_BYTES as usize)),
        ),
    ] {
        let path = dir.write(name, text.as_bytes());
        let (store, ok) = TrustStore::load(&path);
        assert!(!ok, "{name}");
        assert!(store.folders().is_empty(), "{name}");
    }
    let many: Vec<String> = (0..257).map(|i| format!("\"{abs}{i}\"")).collect();
    let text = format!("version = 1\nfolders = [{}]\n", many.join(","));
    assert!(TrustStore::parse(text.as_bytes()).is_none());
}

#[test]
fn invalid_store_is_kept_aside_not_overwritten() {
    let dir = TempDir::new("trust-invalid-aside");
    let path = dir.write("trust.toml", b"not a trust store = [");
    let moved = TrustStore::set_invalid_aside(&path, 1_700_000_000).expect("moved");
    assert!(!path.exists());
    assert_eq!(
        moved.file_name().and_then(|n| n.to_str()),
        Some("trust.toml.invalid-1700000000")
    );
    assert_eq!(fs::read(&moved).expect("read"), b"not a trust store = [");
}

#[test]
fn aside_name_gets_a_counter_when_taken() {
    let dir = TempDir::new("trust-invalid-counter");
    let first = dir.write("trust.toml", b"one");
    let taken = dir.write("trust.toml.invalid-5", b"older");
    let moved = TrustStore::set_invalid_aside(&first, 5).expect("moved");
    assert_eq!(
        moved.file_name().and_then(|n| n.to_str()),
        Some("trust.toml.invalid-5-1")
    );
    assert_eq!(fs::read(&taken).expect("read"), b"older");
    assert_eq!(fs::read(&moved).expect("read"), b"one");
}
