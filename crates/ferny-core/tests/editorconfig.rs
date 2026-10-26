//! `.editorconfig` with an allowlist (threat T13).

mod common;

use std::fs;
use std::path::Path;
use std::time::Instant;

use common::TempDir;
use ferny_core::editorconfig::{self, Charset, EditorConfig, IndentStyle, MAX_FILE_BYTES};
use ferny_core::text::Eol;

fn config(dir: &TempDir, rel: &str) -> EditorConfig {
    editorconfig::for_file(&dir.0.join(rel))
}

fn write(dir: &Path, text: &str) {
    fs::create_dir_all(dir).expect("mkdir");
    fs::write(dir.join(".editorconfig"), text).expect("write");
}

#[test]
fn basic_properties() {
    let dir = TempDir::new("ec-basic");
    write(
        &dir.0,
        "root = true\n[*]\nindent_style = space\nindent_size = 2\nend_of_line = crlf\n\
         charset = utf-8-bom\ntrim_trailing_whitespace = true\ninsert_final_newline = false\n",
    );
    let c = config(&dir, "a.c");
    assert_eq!(c.indent_style, Some(IndentStyle::Space));
    assert_eq!(c.indent_size, Some(2));
    assert_eq!(c.tab_width, Some(2));
    assert_eq!(c.end_of_line, Some(Eol::CrLf));
    assert_eq!(c.charset, Some(Charset::Utf8Bom));
    assert_eq!(c.trim_trailing_whitespace, Some(true));
    assert_eq!(c.insert_final_newline, Some(false));
}

#[test]
fn globs_follow_the_spec() {
    let dir = TempDir::new("ec-globs");
    write(
        &dir.0,
        "root = true\n\
         [*.{c,h}]\nindent_size = 3\n\
         [lib/**.rs]\nindent_size = 4\n\
         [file?.txt]\nindent_size = 5\n\
         [[ab].md]\nindent_size = 6\n\
         [[!ab].md]\nindent_size = 7\n\
         [log{1..3}.txt]\nindent_size = 8\n\
         [/top.py]\nindent_size = 9\n",
    );
    let size = |rel: &str| config(&dir, rel).indent_size;
    assert_eq!(size("x.c"), Some(3));
    assert_eq!(size("deep/dir/x.h"), Some(3));
    assert_eq!(size("lib/a/b/m.rs"), Some(4));
    assert_eq!(size("other/m.rs"), None);
    assert_eq!(size("file1.txt"), Some(5));
    assert_eq!(size("file12.txt"), None);
    assert_eq!(size("a.md"), Some(6));
    assert_eq!(size("c.md"), Some(7));
    assert_eq!(size("log2.txt"), Some(8));
    assert_eq!(size("log4.txt"), None);
    assert_eq!(size("top.py"), Some(9));
    assert_eq!(size("sub/top.py"), None);
}

#[test]
fn nearer_files_and_later_sections_win_and_root_stops() {
    let dir = TempDir::new("ec-precedence");
    write(&dir.0, "[*]\nindent_size = 8\ntab_width = 8\n");
    write(
        &dir.0.join("project"),
        "root = true\n[*]\nindent_size = 2\n[*.py]\nindent_size = 4\n",
    );
    write(
        &dir.0.join("project/sub"),
        "[*.py]\nindent_size = 6\nindent_style = tab\n",
    );
    let c = config(&dir, "project/sub/a.py");
    assert_eq!(c.indent_size, Some(6));
    assert_eq!(c.indent_style, Some(IndentStyle::Tab));
    // root = true in project/: the outer tab_width = 8 is not read.
    assert_eq!(c.tab_width, Some(6));
    assert_eq!(config(&dir, "project/a.txt").indent_size, Some(2));
    assert_eq!(config(&dir, "outside.txt").indent_size, Some(8));
}

#[test]
fn unset_and_tab_sizes() {
    let dir = TempDir::new("ec-unset");
    write(
        &dir.0,
        "root = true\n[*]\nindent_size = 4\n[*.mk]\nindent_size = unset\n\
         [*.go]\nindent_style = tab\ntab_width = 8\nindent_size = tab\n",
    );
    assert_eq!(config(&dir, "x.mk").indent_size, None);
    let go = config(&dir, "x.go");
    assert_eq!((go.indent_size, go.tab_width), (Some(8), Some(8)));
}

#[test]
fn ignores_unknown_keys() {
    let dir = TempDir::new("ec-unknown");
    write(
        &dir.0,
        "root = true\n[*]\nbuild_command = rm -rf /\nmax_line_length = 80\nindent_size = 2\n",
    );
    assert_eq!(config(&dir, "a").indent_size, Some(2));
}

#[test]
fn rejects_bad_values() {
    let dir = TempDir::new("ec-bad");
    write(
        &dir.0,
        "root = true\n[*]\nindent_size = 0\ntab_width = 999\nindent_style = sideways\n\
         end_of_line = lfcr\ncharset = ebcdic\ninsert_final_newline = yes\n",
    );
    assert_eq!(config(&dir, "a"), EditorConfig::default());
}

#[test]
fn oversized_editorconfig_is_ignored() {
    let dir = TempDir::new("ec-huge");
    // The outer file isolates the test from any .editorconfig above it.
    write(&dir.0, "root = true\n[*]\nindent_size = 3\n");
    let mut text = String::from("root = true\n[*]\nindent_size = 2\n");
    while (text.len() as u64) <= MAX_FILE_BYTES {
        text.push_str("# padding padding padding padding padding padding\n");
    }
    write(&dir.0.join("inner"), &text);
    // The oversized inner file is skipped entirely, its root included.
    assert_eq!(config(&dir, "inner/a").indent_size, Some(3));
}

#[test]
fn pathological_glob_is_fast() {
    let dir = TempDir::new("ec-redos");
    let glob = format!("[{}b]\nindent_size = 2\n", "*a".repeat(200));
    write(&dir.0, &format!("root = true\n{glob}"));
    let name = format!("{}.txt", "a".repeat(2000));
    let start = Instant::now();
    assert_eq!(config(&dir, &name).indent_size, None);
    assert!(start.elapsed().as_secs() < 2, "{:?}", start.elapsed());
}

#[test]
fn save_transforms_trim_and_final_newline() {
    let both = EditorConfig {
        trim_trailing_whitespace: Some(true),
        insert_final_newline: Some(true),
        ..EditorConfig::default()
    };
    assert_eq!(
        editorconfig::apply_on_save("a  \r\nb\t\r\nc ", &both, Eol::CrLf).as_deref(),
        Some("a\r\nb\r\nc\r\n")
    );
    assert_eq!(editorconfig::apply_on_save("a\n", &both, Eol::Lf), None);
    assert_eq!(editorconfig::apply_on_save("", &both, Eol::Lf), None);
    assert_eq!(
        editorconfig::apply_on_save("a  ", &EditorConfig::default(), Eol::Lf),
        None
    );
}
