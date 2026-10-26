//! Fuzz target: crafted `session.toml` contents (threat T10).
#![no_main]

use ferny_core::session;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let Ok(loaded) = session::parse(bytes) else {
        return;
    };
    assert!(loaded.files.len() <= session::MAX_SESSION_FILES);
    assert!(loaded.recent.len() <= session::MAX_RECENT_FILES);
    assert!(loaded.geometry.len() <= session::MAX_GEOMETRY_BYTES);
    assert!(loaded.files.is_empty() || loaded.active < loaded.files.len());
    assert!(loaded.files.iter().all(|f| f.path.is_absolute()));
});
