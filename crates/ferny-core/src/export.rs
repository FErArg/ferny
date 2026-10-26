//! HTML and LaTeX export with syntax colours (H4.1a, threat T7).
//!
//! The UI gives the text, one Scintilla style number per byte and the look
//! of each style; this module writes a standalone document. The text is
//! untrusted: every character that means something in the target format is
//! escaped, and controls (other than tab, newline and carriage return) and
//! deceptive characters are written as visible escapes, as in file names.

use std::collections::BTreeMap;
use std::fmt::Write;

/// How one style looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleLook {
    /// Text colour, `0xRRGGBB`.
    pub fore: u32,
    /// Background colour, `0xRRGGBB`.
    pub back: u32,
    /// Bold text.
    pub bold: bool,
    /// Italic text.
    pub italic: bool,
}

/// A run of text in one style.
fn runs<'a>(text: &'a str, styles: &[u8]) -> Vec<(u8, &'a str)> {
    let mut out: Vec<(u8, &'a str)> = Vec::new();
    let mut start = 0;
    let mut current: Option<u8> = None;
    for (i, _) in text.char_indices() {
        let style = styles.get(i).copied().unwrap_or(0);
        if current != Some(style) {
            if let Some(s) = current {
                out.push((s, &text[start..i]));
            }
            start = i;
            current = Some(style);
        }
    }
    if let Some(s) = current {
        out.push((s, &text[start..]));
    }
    out
}

/// The characters of `text` as they are exported: controls and deceptive
/// characters become visible escapes.
fn visible(text: &str) -> impl Iterator<Item = String> + '_ {
    text.chars().map(|c| {
        if matches!(c, '\t' | '\n' | '\r') {
            c.to_string()
        } else {
            crate::browse::display_name(&c.to_string())
        }
    })
}

fn html_escape(text: &str, out: &mut String) {
    for piece in visible(text) {
        for c in piece.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                c => out.push(c),
            }
        }
    }
}

/// A standalone HTML page: a `<pre>` with one class per style used.
pub fn html(
    text: &str,
    styles: &[u8],
    looks: &BTreeMap<u8, StyleLook>,
    background: u32,
    title: &str,
) -> String {
    let runs = runs(text, styles);
    let mut css = String::new();
    let mut used: Vec<u8> = runs.iter().map(|(s, _)| *s).collect();
    used.sort_unstable();
    used.dedup();
    for style in used {
        if let Some(look) = looks.get(&style) {
            let _ = writeln!(
                css,
                ".s{style} {{ color: #{:06x}; background: #{:06x};{}{} }}",
                look.fore & 0xff_ffff,
                look.back & 0xff_ffff,
                if look.bold { " font-weight: bold;" } else { "" },
                if look.italic {
                    " font-style: italic;"
                } else {
                    ""
                },
            );
        }
    }
    let mut body = String::new();
    for (style, run) in runs {
        let _ = write!(body, "<span class=\"s{style}\">");
        html_escape(run, &mut body);
        body.push_str("</span>");
    }
    let mut escaped_title = String::new();
    html_escape(title, &mut escaped_title);
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"generator\" content=\"Ferny\">\n<title>{escaped_title}</title>\n\
         <style>\nbody {{ background: #{:06x}; }}\npre {{ font-family: monospace; }}\n{css}</style>\n\
         </head>\n<body>\n<pre>{body}</pre>\n</body>\n</html>\n",
        background & 0xff_ffff
    )
}

fn latex_escape(text: &str, tab_width: usize, column: &mut usize, out: &mut String) {
    for piece in visible(text) {
        for c in piece.chars() {
            match c {
                '\n' => {
                    out.push('\n');
                    *column = 0;
                    continue;
                }
                '\r' => continue,
                '\t' => {
                    let width = tab_width.max(1);
                    let spaces = width - *column % width;
                    out.push_str(&" ".repeat(spaces));
                    *column += spaces;
                    continue;
                }
                '\\' => out.push_str("\\textbackslash{}"),
                '{' => out.push_str("\\{"),
                '}' => out.push_str("\\}"),
                '#' | '$' | '%' | '&' | '^' | '_' | '~' => {
                    let _ = write!(out, "\\char{}{{}}", u32::from(c));
                }
                c => out.push(c),
            }
            *column += 1;
        }
    }
}

/// A standalone LaTeX document (`alltt`, `xcolor`): one colour per style
/// used, bold and italic as in the editor.
pub fn latex(
    text: &str,
    styles: &[u8],
    looks: &BTreeMap<u8, StyleLook>,
    tab_width: usize,
) -> String {
    let runs = runs(text, styles);
    let mut colours = String::new();
    let mut used: Vec<u8> = runs.iter().map(|(s, _)| *s).collect();
    used.sort_unstable();
    used.dedup();
    for style in &used {
        if let Some(look) = looks.get(style) {
            let _ = writeln!(
                colours,
                "\\definecolor{{s{style}}}{{HTML}}{{{:06X}}}",
                look.fore & 0xff_ffff
            );
        }
    }
    let mut body = String::new();
    let mut column = 0;
    for (style, run) in runs {
        let look = looks.get(&style);
        // One group per line, so a style never spans a paragraph break.
        for (i, line) in run.split('\n').enumerate() {
            if i > 0 {
                body.push('\n');
                column = 0;
            }
            if line.is_empty() {
                continue;
            }
            let mut inner = String::new();
            latex_escape(line, tab_width, &mut column, &mut inner);
            match look {
                Some(look) => {
                    let mut wrapped = format!("\\textcolor{{s{style}}}{{{inner}}}");
                    if look.bold {
                        wrapped = format!("\\textbf{{{wrapped}}}");
                    }
                    if look.italic {
                        wrapped = format!("\\textit{{{wrapped}}}");
                    }
                    body.push_str(&wrapped);
                }
                None => body.push_str(&inner),
            }
        }
    }
    format!(
        "% Generated by Ferny\n\\documentclass[a4paper]{{article}}\n\
         \\usepackage[T1]{{fontenc}}\n\\usepackage[utf8]{{inputenc}}\n\
         \\usepackage{{xcolor}}\n\\usepackage{{alltt}}\n{colours}\
         \\begin{{document}}\n\\begin{{alltt}}\n{body}\n\\end{{alltt}}\n\\end{{document}}\n"
    )
}
