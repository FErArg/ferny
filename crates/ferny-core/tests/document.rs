//! Opening documents: limits, file types, encodings, BOM, line endings
//! (threat T6).

mod common;

use std::fs;

use common::TempDir;
use ferny_core::document::{
    Documents, LARGE_DOCUMENT_BYTES, OpenError, ReadOnlyReason, read_text_file,
};
use ferny_core::fsio::{self, ReadError};
use ferny_core::text::Eol;

fn open(
    path: &std::path::Path,
) -> Result<(Documents, ferny_core::document::DocumentId, String), OpenError> {
    let mut docs = Documents::new();
    let (id, text) = docs.open(path, None)?;
    Ok((docs, id, text))
}

#[test]
fn reads_utf8_file() {
    let dir = TempDir::new("doc-utf8");
    let path = dir.write("a.rs", "fn main() { println!(\"ñ€\"); }\n".as_bytes());
    let (docs, id, text) = open(&path).expect("open");
    assert_eq!(text, "fn main() { println!(\"ñ€\"); }\n");
    let doc = docs.get(id).expect("document");
    assert_eq!(doc.encoding().name(), "UTF-8");
    assert!(!doc.bom());
    assert_eq!(doc.eol(), Eol::Lf);
    assert_eq!(doc.read_only(), None);
}

#[test]
fn reads_empty_file_with_native_eol() {
    let dir = TempDir::new("doc-empty");
    let (docs, id, text) = open(&dir.write("empty.txt", b"")).expect("open");
    assert_eq!(text, "");
    assert_eq!(docs.get(id).map(|d| d.eol()), Some(Eol::native()));
}

#[test]
fn bounded_read_accepts_exact_limit_and_rejects_more() {
    let dir = TempDir::new("doc-limit");
    let ten = dir.write("ten.txt", b"0123456789");
    assert_eq!(fsio::read_limited(&ten, 10).map(|f| f.bytes.len()), Ok(10));
    let eleven = dir.write("eleven.txt", b"0123456789A");
    assert_eq!(
        fsio::read_limited(&eleven, 10).map(|f| f.bytes.len()),
        Err(ReadError::TooLarge)
    );
}

#[test]
fn large_flag_above_threshold() {
    let dir = TempDir::new("doc-large");
    let size = usize::try_from(LARGE_DOCUMENT_BYTES).expect("fits") + 1;
    let path = dir.write("large.txt", &vec![b'a'; size]);
    let (docs, id, _) = open(&path).expect("open");
    assert!(docs.get(id).expect("document").is_large());
    let small = dir.write("small.txt", b"a");
    let (docs, id, _) = open(&small).expect("open");
    assert!(!docs.get(id).expect("document").is_large());
}

#[test]
fn detects_legacy_encoding() {
    let dir = TempDir::new("doc-latin1");
    // "café au lait, señor" in windows-1252 / ISO-8859-1.
    let path = dir.write("latin1.txt", b"caf\xe9 au lait, se\xf1or\n");
    let (docs, id, text) = open(&path).expect("open");
    assert_eq!(text, "café au lait, señor\n");
    assert_eq!(
        docs.get(id).map(|d| d.encoding().name()),
        Some("windows-1252")
    );
}

#[test]
fn utf8_bom_is_stripped_and_remembered() {
    let dir = TempDir::new("doc-bom");
    let path = dir.write("bom.txt", b"\xEF\xBB\xBFhola\r\n");
    let (docs, id, text) = open(&path).expect("open");
    assert_eq!(text, "hola\r\n");
    let doc = docs.get(id).expect("document");
    assert!(doc.bom());
    assert_eq!(doc.eol(), Eol::CrLf);
}

#[test]
fn utf16_with_bom_is_decoded() {
    let dir = TempDir::new("doc-utf16");
    let mut bytes = vec![0xFF, 0xFE];
    "añ\n"
        .encode_utf16()
        .for_each(|u| bytes.extend_from_slice(&u.to_le_bytes()));
    let (docs, id, text) = open(&dir.write("utf16.txt", &bytes)).expect("open");
    assert_eq!(text, "añ\n");
    assert_eq!(docs.get(id).map(|d| d.encoding().name()), Some("UTF-16LE"));
}

#[test]
fn binary_file_is_refused() {
    let dir = TempDir::new("doc-binary");
    let path = dir.write("blob.bin", b"\x7fELF\x02\x01\x01\x00\x00\x00\xff\xfe");
    assert_eq!(read_text_file(&path), Err(OpenError::Binary));
}

#[test]
fn utf8_with_nul_bytes_is_text() {
    let dir = TempDir::new("doc-nul");
    let path = dir.write("nul.txt", b"a\0b\n");
    assert_eq!(read_text_file(&path), Ok("a\0b\n".to_owned()));
}

#[test]
fn malformed_input_opens_read_only() {
    let dir = TempDir::new("doc-malformed");
    // Not valid Shift_JIS: a lead byte followed by an invalid trail byte.
    let path = dir.write("sjis.txt", b"abc\x81\x20def\n");
    let mut docs = Documents::new();
    let (id, text) = docs.open(&path, Some("Shift_JIS")).expect("open");
    assert!(text.contains('\u{FFFD}'));
    assert_eq!(
        docs.get(id).and_then(|d| d.read_only()),
        Some(ReadOnlyReason::MalformedEncoding)
    );
}

#[test]
fn forced_encoding_and_unknown_label() {
    let dir = TempDir::new("doc-forced");
    let path = dir.write("koi8.txt", b"\xf0\xd2\xc9\xd7\xc5\xd4\n");
    let mut docs = Documents::new();
    let (_, text) = docs.open(&path, Some("KOI8-R")).expect("open");
    assert_eq!(text, "Привет\n");
    assert_eq!(
        docs.open(&path, Some("no-such-encoding")).map(|_| ()),
        Err(OpenError::UnknownEncoding)
    );
    assert_eq!(
        docs.open(&path, Some("replacement")).map(|_| ()),
        Err(OpenError::UnknownEncoding)
    );
}

#[test]
fn reload_with_another_encoding_keeps_the_id() {
    let dir = TempDir::new("doc-reload");
    let path = dir.write("text.txt", b"\xe9t\xe9\n");
    let mut docs = Documents::new();
    let (id, first) = docs.open(&path, None).expect("open");
    assert_eq!(first, "été\n");
    let again = docs.reload(id, Some("ISO-8859-7")).expect("reload");
    assert_eq!(again, "ιtι\n");
    assert_eq!(
        docs.get(id).map(|d| d.encoding().name()),
        Some("ISO-8859-7")
    );
}

#[test]
fn mixed_line_endings_are_reported() {
    let dir = TempDir::new("doc-eol");
    let path = dir.write("mixed.txt", b"a\r\nb\r\nc\n");
    let (docs, id, _) = open(&path).expect("open");
    let doc = docs.get(id).expect("document");
    assert_eq!(doc.eol(), Eol::CrLf);
    assert!(doc.mixed_eol());
}

#[test]
fn rejects_missing_file_and_directory() {
    let dir = TempDir::new("doc-missing");
    assert_eq!(
        read_text_file(&dir.0.join("nope")),
        Err(OpenError::NotFound)
    );
    assert_eq!(read_text_file(&dir.0), Err(OpenError::NotAFile));
}

#[cfg(unix)]
#[test]
fn rejects_fifo_without_blocking() {
    let dir = TempDir::new("doc-fifo");
    let fifo = dir.0.join("pipe");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(status.success());
    assert_eq!(read_text_file(&fifo), Err(OpenError::NotAFile));
}

#[cfg(unix)]
#[test]
fn read_only_permissions_are_reported() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("doc-ro");
    let path = dir.write("ro.txt", b"x");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("chmod");
    let (docs, id, _) = open(&path).expect("open");
    assert_eq!(
        docs.get(id).and_then(|d| d.read_only()),
        Some(ReadOnlyReason::Permissions)
    );
}

#[test]
fn error_messages_carry_no_path() {
    let dir = TempDir::new("doc-message");
    let err = read_text_file(&dir.0.join("secret-name.txt")).expect_err("missing");
    assert!(!err.to_string().contains("secret-name"));
}

#[test]
fn new_document_uses_chosen_defaults() {
    let mut docs = Documents::new();
    let id = docs.new_document_with("latin1", Eol::CrLf);
    let doc = docs.get(id).expect("document");
    assert_eq!(doc.encoding().name(), "windows-1252");
    assert_eq!(doc.eol(), Eol::CrLf);
    let id = docs.new_document_with("no-such", Eol::Lf);
    assert_eq!(docs.get(id).map(|d| d.encoding().name()), Some("UTF-8"));
}

#[test]
fn editorconfig_charset_decides_the_decoding() {
    let dir = TempDir::new("doc-ec-charset");
    fs::write(
        dir.0.join(".editorconfig"),
        "root = true\n[*.txt]\ncharset = latin1\n",
    )
    .expect("write");
    // Valid UTF-8 bytes, but the project says latin1: decoded as such.
    let path = dir.write("a.txt", "é".as_bytes());
    let mut docs = Documents::new();
    let (id, text) = docs.open(&path, None).expect("open");
    assert_eq!(text, "Ã©");
    assert_eq!(
        docs.get(id).map(|d| d.encoding().name()),
        Some("windows-1252")
    );
    // An explicit choice of the user wins.
    let (_, text) = docs.open(&path, Some("UTF-8")).expect("open");
    assert_eq!(text, "é");
}

#[test]
fn editorconfig_utf8_bom_and_end_of_line() {
    let dir = TempDir::new("doc-ec-bom");
    fs::write(
        dir.0.join(".editorconfig"),
        "root = true\n[*]\ncharset = utf-8-bom\nend_of_line = crlf\n",
    )
    .expect("write");
    let empty = dir.write("empty.txt", b"");
    let mut docs = Documents::new();
    let (id, _) = docs.open(&empty, None).expect("open");
    let doc = docs.get(id).expect("document");
    assert!(doc.bom());
    assert_eq!(doc.eol(), Eol::CrLf);
    // Existing line endings are not converted.
    let lf = dir.write("lf.txt", b"a\nb\n");
    let (id, _) = docs.open(&lf, None).expect("open");
    assert_eq!(docs.get(id).map(|d| d.eol()), Some(Eol::Lf));
}
