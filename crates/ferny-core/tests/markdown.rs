//! Markdown preview (H4.2b, threats T6, T7).

use ferny_core::markdown::{MAX_MARKDOWN_BYTES, to_html};

#[test]
fn renders_commonmark() {
    let html = to_html("# Title\n\n*a* | b\n--|--\n1 | 2\n\n- [x] done\n").expect("html");
    assert!(html.contains("<h1>Title</h1>"));
    assert!(html.contains("<table>"));
    assert!(html.contains("checked"));
}

#[test]
fn raw_html_is_text() {
    let html = to_html("<script>alert(1)</script>\n\ntext <b onclick=x>bold</b>\n").expect("html");
    assert!(!html.contains("<script>") && !html.contains("<b "));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
}

#[test]
fn images_are_not_loaded() {
    let html = to_html("![logo](file:///etc/passwd) ![x](https://e.example/t.png)").expect("html");
    assert!(!html.contains("<img"));
    assert!(html.contains("[image: logo]") && html.contains("[image: x]"));
}

#[test]
fn links_are_filtered() {
    let html = to_html(
        "[ok](https://example.com) [mail](mailto:a@b.c) [top](#top) \
         [js](javascript:alert(1)) [file](file:///etc/passwd) [rel](../secret.md)",
    )
    .expect("html");
    for good in ["https://example.com", "mailto:a@b.c", "#top"] {
        assert!(html.contains(&format!("href=\"{good}\"")), "{good}");
    }
    assert!(!html.contains("javascript:") && !html.contains("file:") && !html.contains("secret"));
    assert_eq!(html.matches("href=\"\"").count(), 3);
}

#[test]
fn large_documents_are_not_previewed() {
    assert!(to_html(&"a".repeat(MAX_MARKDOWN_BYTES + 1)).is_none());
    // Pathological nesting stays fast.
    let started = std::time::Instant::now();
    assert!(to_html(&"[".repeat(100_000)).is_some());
    assert!(to_html(&"> ".repeat(50_000)).is_some());
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}
