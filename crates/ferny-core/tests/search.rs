//! Search, replace and find in files (threat T6).

mod common;

use std::fs;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use common::TempDir;
use ferny_core::search::{
    self, MAX_PATTERN_BYTES, MAX_RESULTS, Query, SearchError, find, replace_all, replacement_for,
};

fn plain(pattern: &str) -> Query {
    Query {
        pattern: pattern.to_owned(),
        ..Query::default()
    }
}

fn regex(pattern: &str) -> Query {
    Query {
        pattern: pattern.to_owned(),
        regex: true,
        match_case: true,
        ..Query::default()
    }
}

#[test]
fn find_forward_backward_and_wrap() {
    let text = "foo bar foo baz foo";
    let q = Query {
        match_case: true,
        ..plain("foo")
    };
    assert_eq!(find(text, &q, 0, true), Ok(Some((0, 3))));
    assert_eq!(find(text, &q, 1, true), Ok(Some((8, 11))));
    assert_eq!(find(text, &q, 17, true), Ok(Some((0, 3))), "wraps around");
    assert_eq!(find(text, &q, 16, false), Ok(Some((8, 11))));
    assert_eq!(
        find(text, &q, 2, false),
        Ok(Some((16, 19))),
        "wraps backwards"
    );
    assert_eq!(find(text, &plain("zzz"), 0, true), Ok(None));
}

#[test]
fn options_case_whole_word_and_literal() {
    let text = "Word sword word.";
    assert_eq!(
        find(text, &plain("word"), 0, true),
        Ok(Some((0, 4))),
        "case-insensitive by default"
    );
    let exact = Query {
        match_case: true,
        whole_word: true,
        ..plain("word")
    };
    assert_eq!(find(text, &exact, 0, true), Ok(Some((11, 15))));
    // Plain queries are literal: regex characters match themselves.
    assert_eq!(find("a.b axb", &plain("a.b"), 1, true), Ok(Some((0, 3))));
    assert_eq!(find("a.b axb", &regex("a.b"), 1, true), Ok(Some((4, 7))));
}

#[test]
fn utf8_offsets_are_byte_offsets() {
    let text = "ñandú y ñandú";
    let q = plain("ñandú");
    let (start, end) = find(text, &q, 1, true).expect("ok").expect("found");
    assert_eq!(&text[start..end], "ñandú");
    assert_eq!(start, "ñandú y ".len());
}

#[test]
fn replace_one_with_and_without_captures() {
    let text = "key=value";
    let q = regex(r"(\w+)=(\w+)");
    assert_eq!(
        replacement_for(text, &q, 0, 9, "$2=$1"),
        Ok(Some("value=key".to_owned()))
    );
    assert_eq!(
        replacement_for(text, &q, 0, 5, "x"),
        Ok(None),
        "not a match there"
    );
    // Plain queries never expand `$`.
    assert_eq!(
        replacement_for("a b", &plain("a"), 0, 1, "$1"),
        Ok(Some("$1".to_owned()))
    );
}

#[test]
fn replace_all_counts_and_expands() {
    assert_eq!(
        replace_all("a1 b2 c3", &regex(r"([a-z])(\d)"), "$2$1"),
        Ok(("1a 2b 3c".to_owned(), 3))
    );
    assert_eq!(
        replace_all("x.x", &plain("."), "$0"),
        Ok(("x$0x".to_owned(), 1))
    );
    assert_eq!(
        replace_all("abc", &plain("z"), "y"),
        Ok(("abc".to_owned(), 0))
    );
}

#[test]
fn invalid_pattern_is_reported() {
    assert_eq!(
        find("x", &regex("(unclosed"), 0, true),
        Err(SearchError::InvalidPattern)
    );
    assert_eq!(find("x", &plain(""), 0, true), Err(SearchError::BadLength));
    let long = "a".repeat(MAX_PATTERN_BYTES + 1);
    assert_eq!(
        find("x", &plain(&long), 0, true),
        Err(SearchError::BadLength)
    );
    // Huge repetition counts exceed the compiled size limit instead of
    // exhausting memory.
    assert_eq!(
        find("x", &regex("(a{1000}){1000}"), 0, true),
        Err(SearchError::InvalidPattern)
    );
}

#[test]
fn pathological_pattern_is_linear() {
    // Catastrophic for backtracking engines; linear here.
    let text = format!("{}!", "a".repeat(100_000));
    let start = Instant::now();
    assert_eq!(find(&text, &regex("(a+)+b"), 0, true), Ok(None));
    assert!(start.elapsed().as_secs() < 2, "{:?}", start.elapsed());
}

fn tree(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    dir.write("a.c", b"int main() {\n  return needle;\n}\n");
    dir.write("b.txt", b"no match here\nneedle at start\n");
    fs::create_dir_all(dir.0.join("sub/deeper")).expect("mkdir");
    fs::write(dir.0.join("sub/deeper/c.c"), b"/* needle */\n").expect("write");
    fs::create_dir_all(dir.0.join(".git")).expect("mkdir");
    fs::write(dir.0.join(".git/needle.c"), b"needle\n").expect("write");
    dir
}

#[test]
fn find_in_files_with_patterns() {
    let dir = tree("search-tree-patterns");
    let cancel = AtomicBool::new(false);
    let all = search::find_in_files(&dir.0, &plain("needle"), &[], &cancel).expect("search");
    let found: Vec<(String, u32, u32)> = all
        .hits
        .iter()
        .map(|h| {
            let rel = h
                .path
                .strip_prefix(&dir.0)
                .expect("inside")
                .to_string_lossy()
                .replace('\\', "/");
            (rel, h.line, h.column)
        })
        .collect();
    assert_eq!(
        found,
        [
            ("a.c".to_owned(), 2, 10),
            ("b.txt".to_owned(), 2, 1),
            ("sub/deeper/c.c".to_owned(), 1, 4),
        ],
        "version-control directories are skipped"
    );
    let only_c = search::find_in_files(&dir.0, &plain("needle"), &["*.c".to_owned()], &cancel)
        .expect("search");
    assert_eq!(only_c.hits.len(), 2);
}

#[test]
fn skips_large_and_binary() {
    let dir = TempDir::new("search-skip");
    dir.write("binary.bin", b"needle\x00\xff\xfe");
    let big =
        vec![b'n'; usize::try_from(ferny_core::fsio::LARGE_DOCUMENT_BYTES).expect("fits") + 1];
    dir.write("big.txt", &big);
    dir.write("ok.txt", b"needle\n");
    let result = search::find_in_files(&dir.0, &plain("needle"), &[], &AtomicBool::new(false))
        .expect("search");
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.files_skipped, 2);
}

#[test]
fn stops_at_result_limit() {
    let dir = TempDir::new("search-limit");
    dir.write("many.txt", "x\n".repeat(MAX_RESULTS + 5).as_bytes());
    let result =
        search::find_in_files(&dir.0, &plain("x"), &[], &AtomicBool::new(false)).expect("search");
    assert_eq!(result.hits.len(), MAX_RESULTS);
    assert!(result.truncated);
}

#[test]
fn cancel_stops_the_walk() {
    let dir = tree("search-tree-cancel");
    let result = search::find_in_files(&dir.0, &plain("needle"), &[], &AtomicBool::new(true))
        .expect("search");
    assert!(result.cancelled);
    assert!(result.hits.is_empty());
}

#[cfg(unix)]
#[test]
fn does_not_follow_symlinks_or_read_special_files() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new("search-links");
    let outside = TempDir::new("search-links-outside");
    outside.write("secret.txt", b"needle outside\n");
    symlink(&outside.0, dir.0.join("link-dir")).expect("symlink");
    symlink(outside.0.join("secret.txt"), dir.0.join("link-file.txt")).expect("symlink");
    symlink(&dir.0, dir.0.join("loop")).expect("symlink");
    let status = std::process::Command::new("mkfifo")
        .arg(dir.0.join("pipe"))
        .status()
        .expect("mkfifo");
    assert!(status.success());
    let result = search::find_in_files(&dir.0, &plain("needle"), &[], &AtomicBool::new(false))
        .expect("search");
    assert!(result.hits.is_empty());
    assert!(result.files_skipped >= 4);
}
