//! Comment toggling and visible control characters (threat T7).

use ferny_core::editing::{
    BIDI_CONTROLS, CommentTokens, DetectedIndent, INVISIBLE_CHARACTERS, MAX_INDENT_LINES,
    count_bidi_controls, detect_indent, toggle_comment,
};

fn line(token: &str) -> CommentTokens {
    CommentTokens {
        single: Some(token.to_owned()),
        open: None,
        close: None,
    }
}

fn block(open: &str, close: &str) -> CommentTokens {
    CommentTokens {
        single: None,
        open: Some(open.to_owned()),
        close: Some(close.to_owned()),
    }
}

#[test]
fn comment_toggle_line_tokens_round_trip() {
    let text = "    int a = 1;\n\n        a++;\n    return a;";
    let commented = toggle_comment(text, &line("//")).expect("tokens");
    assert_eq!(
        commented,
        "    // int a = 1;\n\n    //     a++;\n    // return a;"
    );
    assert_eq!(
        toggle_comment(&commented, &line("//")).as_deref(),
        Some(text)
    );
}

#[test]
fn comment_toggle_mixed_lines_comments_all() {
    let text = "# done\nx = 1";
    assert_eq!(
        toggle_comment(text, &line("#")).as_deref(),
        Some("# # done\n# x = 1")
    );
}

#[test]
fn comment_toggle_keeps_crlf() {
    let text = "a\r\nb";
    let commented = toggle_comment(text, &line("--")).expect("tokens");
    assert_eq!(commented, "-- a\r\n-- b");
    assert_eq!(
        toggle_comment(&commented, &line("--")).as_deref(),
        Some(text)
    );
}

#[test]
fn comment_toggle_without_space_after_token() {
    assert_eq!(
        toggle_comment("//x\n  //y", &line("//")).as_deref(),
        Some("x\n  y")
    );
}

#[test]
fn comment_toggle_block_tokens_round_trip() {
    let text = "  body { }\n  p { }";
    let commented = toggle_comment(text, &block("/*", "*/")).expect("tokens");
    assert_eq!(commented, "  /* body { } */\n  /* p { } */");
    assert_eq!(
        toggle_comment(&commented, &block("/*", "*/")).as_deref(),
        Some(text)
    );
}

#[test]
fn comment_toggle_blank_and_missing_tokens() {
    assert_eq!(toggle_comment("  \n", &line("#")).as_deref(), Some("  \n"));
    assert_eq!(toggle_comment("x", &CommentTokens::default()), None);
    assert_eq!(toggle_comment("x", &block("/*", "")), None);
}

#[test]
fn comment_toggle_with_unicode_whitespace_does_not_split_characters() {
    // U+3000 (3 bytes) is not indentation; no slicing inside it.
    let text = " a\n\u{3000}b";
    let commented = toggle_comment(text, &line("#")).expect("tokens");
    assert_eq!(commented, "#  a\n# \u{3000}b");
    assert_eq!(
        toggle_comment(&commented, &line("#")).as_deref(),
        Some(text)
    );
}

#[test]
fn bidi_controls_are_found() {
    // The classic Trojan Source "commenting-out" pattern.
    let text = "/*\u{202E} } \u{2066}if (isAdmin)\u{2069} \u{2066} begin admins only */";
    assert_eq!(count_bidi_controls(text), 4);
    assert_eq!(count_bidi_controls("plain ascii and ñandú"), 0);
    for (c, _) in BIDI_CONTROLS {
        assert_eq!(count_bidi_controls(&c.to_string()), 1, "{c:?}");
    }
}

#[test]
fn invisible_characters_are_listed() {
    let labels: Vec<&str> = INVISIBLE_CHARACTERS.iter().map(|(_, l)| *l).collect();
    assert!(labels.contains(&"ZWSP"));
    for (c, label) in BIDI_CONTROLS.iter().chain(INVISIBLE_CHARACTERS) {
        assert!(!label.is_empty() && label.is_ascii(), "{c:?}");
        // None of them are printable on their own.
        assert!(!c.is_alphanumeric());
    }
}

#[test]
fn completions_from_the_document() {
    use ferny_core::editing::completions;
    let text = "helper help hello helper_two h ñandú ñan x.helpful";
    assert_eq!(
        completions(text, "hel"),
        ["hello", "help", "helper", "helper_two", "helpful"]
    );
    assert_eq!(completions(text, "ñan"), ["ñandú"]);
    assert!(completions(text, "").is_empty());
    assert!(completions(text, "zzz").is_empty());
}

#[test]
fn completions_are_bounded() {
    use ferny_core::editing::{MAX_COMPLETION_SCAN, MAX_COMPLETIONS, MAX_WORD_BYTES, completions};
    let many: String = (0..2000).map(|i| format!("w{i} ")).collect();
    assert_eq!(completions(&many, "w").len(), MAX_COMPLETIONS);
    let long = format!("w{}", "x".repeat(MAX_WORD_BYTES));
    assert!(completions(&long, "w").is_empty());
    // A word past the scan limit is not offered.
    let far = format!("{} wanted", " ".repeat(MAX_COMPLETION_SCAN));
    assert!(completions(&far, "wan").is_empty());
}

#[test]
fn detect_indent_finds_width_and_style() {
    let four = "fn f() {\n    let a = 1;\n    if a {\n        b();\n    }\n}\n";
    assert_eq!(
        detect_indent(four),
        Some(DetectedIndent {
            use_tabs: false,
            width: 4
        })
    );
    let two = "a:\n  b: 1\n  c:\n    d: 2\n";
    assert_eq!(detect_indent(two).map(|d| d.width), Some(2));
    let tabs = "int f() {\n\tint a;\n\tif (a) {\n\t\ta++;\n\t}\n}\n";
    assert_eq!(
        detect_indent(tabs),
        Some(DetectedIndent {
            use_tabs: true,
            width: 0
        })
    );
    assert_eq!(detect_indent("no indentation\nat all\n"), None);
    assert_eq!(detect_indent("\n   \n\t\n"), None);
}

#[test]
fn detect_indent_reads_a_bounded_number_of_lines() {
    let mut text = "x\n".repeat(MAX_INDENT_LINES);
    text.push_str("y:\n\tz\n");
    // Tabs appear only after the limit, so they are not seen.
    assert_eq!(detect_indent(&text), None);
}
