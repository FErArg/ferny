//! HTML and LaTeX export (H4.1a, threat T7).

use std::collections::BTreeMap;

use ferny_core::export::{StyleLook, html, latex};

fn looks() -> BTreeMap<u8, StyleLook> {
    let mut looks = BTreeMap::new();
    looks.insert(
        0,
        StyleLook {
            fore: 0x00_0000,
            back: 0xff_ffff,
            bold: false,
            italic: false,
        },
    );
    looks.insert(
        5,
        StyleLook {
            fore: 0x00_00ff,
            back: 0xff_ffff,
            bold: true,
            italic: false,
        },
    );
    looks.insert(
        7,
        StyleLook {
            fore: 0x80_8080,
            back: 0xff_ffff,
            bold: false,
            italic: true,
        },
    );
    looks
}

/// Styles per byte: style 5 for `int`, 7 for the comment, 0 elsewhere.
fn sample() -> (String, Vec<u8>) {
    let text = "int x; // <ñ>\n".to_owned();
    let mut styles = vec![0u8; text.len()];
    styles[..3].fill(5);
    let comment = text.find("//").expect("comment");
    styles[comment..text.len() - 1].fill(7);
    (text, styles)
}

#[test]
fn html_has_colours_and_runs() {
    let (text, styles) = sample();
    let page = html(&text, &styles, &looks(), 0xff_ffff, "a.c");
    assert!(page.contains(".s5 { color: #0000ff; background: #ffffff; font-weight: bold; }"));
    assert!(page.contains("<span class=\"s5\">int</span>"));
    assert!(page.contains("<span class=\"s7\">// &lt;ñ&gt;</span>"));
    assert!(page.contains("<title>a.c</title>"));
}

#[test]
fn html_escapes_everything() {
    let text = "</pre><script>alert('x')</script>&\"";
    let styles = vec![0u8; text.len()];
    let page = html(text, &styles, &looks(), 0, "<title>\"");
    assert!(!page.contains("<script>") && !page.contains("</pre><"));
    assert!(
        page.contains("&lt;/pre&gt;&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;&amp;&quot;")
    );
    assert!(page.contains("<title>&lt;title&gt;&quot;</title>"));
    // Exactly the page's own tags.
    assert_eq!(page.matches("<pre>").count(), 1);
    assert_eq!(page.matches("</pre>").count(), 1);
}

#[test]
fn latex_escapes_everything() {
    let text = "\\input{/etc/passwd} \\write18{rm} $x^2_i$ 100% #1 & ~{}";
    let styles = vec![5u8; text.len()];
    let doc = latex(text, &styles, &looks(), 4);
    let body = doc.split("\\begin{alltt}").nth(1).expect("body");
    assert!(!body.contains("\\input") && !body.contains("\\write18"));
    assert!(body.contains("\\textbackslash{}input\\{/etc/passwd\\}"));
    for raw in ["$", "%", "#", "&", "^", "_", "~"] {
        let code = format!("\\char{}{{}}", u32::from(raw.chars().next().expect("c")));
        assert!(body.contains(&code), "{raw}");
    }
    assert!(doc.contains("\\definecolor{s5}{HTML}{0000FF}"));
    assert!(body.contains("\\textbf{\\textcolor{s5}{"));
}

#[test]
fn controls_are_visible() {
    let text = "a\u{202E}b\u{1b}[31m\tc\r\n";
    let styles = vec![0u8; text.len()];
    let page = html(text, &styles, &looks(), 0, "t");
    assert!(!page.contains('\u{202E}') && !page.contains('\u{1b}'));
    assert!(page.contains("\tc\r\n"));
    let doc = latex(text, &styles, &looks(), 4);
    assert!(!doc.contains('\u{202E}') && !doc.contains('\u{1b}') && !doc.contains('\r'));
    // The tab expands to the next multiple of 4 after the escapes.
    assert!(!doc.contains('\t'));
}
