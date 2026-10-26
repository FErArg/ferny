//! Local log (H3.2, threats T10, T17).

mod common;

#[cfg(unix)]
use common::TempDir;
#[cfg(unix)]
use ferny_core::log::{self, MAX_LOG_BYTES};
use ferny_core::log::{MAX_MESSAGE_BYTES, line};

#[test]
fn lines_are_escaped_and_bounded() {
    let text = line("plugin hello", "a\nb \u{1b}[31m \u{202E}", 1_792_000_000);
    assert!(text.starts_with("2026-10-14T"), "{text}");
    assert!(
        text.ends_with("plugin hello: a\\u{000A}b \\u{001B}[31m \\u{202E}\n"),
        "{text}"
    );
    assert_eq!(text.matches('\n').count(), 1);
    let long = line("x", &"é".repeat(1000), 0);
    assert!(long.len() <= MAX_MESSAGE_BYTES + 40, "{}", long.len());
    assert!(
        line("c\n;rm", "m", 0).contains(" crm: m"),
        "component cleaned"
    );
    assert!(line("x", "m", 0).starts_with("1970-01-01T00:00:00Z"));
}

#[cfg(unix)]
#[test]
fn log_is_bounded_and_private() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new("log");
    let path = dir.0.join("ferny.log");
    log::init(&path);
    log::write("test", "first");
    assert_eq!(
        std::fs::metadata(&path).expect("meta").mode() & 0o777,
        0o600
    );
    // Past the limit the file rotates into ferny.log.1.
    std::fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).expect("fill");
    log::write("test", "after rotation");
    assert!(dir.0.join("ferny.log.1").exists());
    let current = std::fs::read_to_string(&path).expect("read");
    assert!(current.contains("test: after rotation") && current.len() < 200);
    // A link planted at the log's path is not followed.
    std::fs::remove_file(&path).expect("rm");
    let target = dir.0.join("elsewhere");
    std::os::unix::fs::symlink(&target, &path).expect("link");
    log::write("test", "nope");
    assert!(!target.exists());
    log::init(std::path::Path::new(""));
}
