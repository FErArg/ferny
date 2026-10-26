//! Helpers shared by the integration tests (each test crate uses a subset).
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

/// A directory under Cargo's per-build temporary directory (inside the
/// target directory, not the shared system temp dir), removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-core-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        // The temp dir lives inside the repository: stop .editorconfig
        // lookups here so the repository's own settings do not apply.
        fs::write(dir.join(".editorconfig"), "root = true\n").expect("write .editorconfig");
        Self(dir)
    }

    pub fn write(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).expect("write fixture");
        path
    }

    /// Names of the entries in the directory, sorted (the isolating
    /// `.editorconfig` excluded).
    pub fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name != ".editorconfig")
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
