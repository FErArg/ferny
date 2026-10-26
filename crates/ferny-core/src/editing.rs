//! Editing helpers that are pure text transformations (testable without
//! Scintilla): comment toggling and the control characters Ferny makes
//! visible (threat T7, "Trojan Source").

/// Bidirectional control characters (Unicode Bidi Algorithm), with the
/// label drawn in their place.
pub const BIDI_CONTROLS: &[(char, &str)] = &[
    ('\u{202A}', "LRE"),
    ('\u{202B}', "RLE"),
    ('\u{202C}', "PDF"),
    ('\u{202D}', "LRO"),
    ('\u{202E}', "RLO"),
    ('\u{2066}', "LRI"),
    ('\u{2067}', "RLI"),
    ('\u{2068}', "FSI"),
    ('\u{2069}', "PDI"),
    ('\u{200E}', "LRM"),
    ('\u{200F}', "RLM"),
    ('\u{061C}', "ALM"),
];

/// Invisible format characters that can hide content, with their labels.
pub const INVISIBLE_CHARACTERS: &[(char, &str)] = &[
    ('\u{200B}', "ZWSP"),
    ('\u{200C}', "ZWNJ"),
    ('\u{200D}', "ZWJ"),
    ('\u{2060}', "WJ"),
    ('\u{FEFF}', "ZWNBSP"),
    ('\u{00AD}', "SHY"),
    ('\u{180E}', "MVS"),
];

/// Number of bidirectional control characters in `text`.
pub fn count_bidi_controls(text: &str) -> usize {
    text.chars()
        .filter(|c| BIDI_CONTROLS.iter().any(|(b, _)| b == c))
        .count()
}

/// Comment tokens of a filetype.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentTokens {
    /// Line comment (`//`, `#`).
    pub single: Option<String>,
    /// Block comment opener (`/*`).
    pub open: Option<String>,
    /// Block comment closer (`*/`).
    pub close: Option<String>,
}

/// Comments or uncomments whole lines.
///
/// `text` is one or more complete lines (`\n` or `\r\n` separated, without
/// a final line ending). If every non-blank line is already commented they
/// are uncommented, otherwise all non-blank lines are commented: with the
/// line token at the smallest indentation, or else with the block tokens
/// around each line. Blank lines are left alone. `None` if the filetype
/// has no comment tokens.
pub fn toggle_comment(text: &str, tokens: &CommentTokens) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (bodies, endings): (Vec<&str>, Vec<&str>) = lines
        .iter()
        .map(|line| match line.strip_suffix('\r') {
            Some(body) => (body, "\r"),
            None => (*line, ""),
        })
        .unzip();
    let non_blank = || bodies.iter().filter(|l| !l.trim().is_empty());

    let changed: Vec<String> =
        if let Some(single) = tokens.single.as_deref().filter(|s| !s.is_empty()) {
            let commented = non_blank().count() > 0
                && non_blank().all(|l| l[indentation(l)..].starts_with(single));
            if commented {
                bodies
                    .iter()
                    .map(|line| uncomment_single(line, single))
                    .collect()
            } else {
                let indent = non_blank().map(|l| indentation(l)).min().unwrap_or(0);
                bodies
                    .iter()
                    .map(|line| {
                        if line.trim().is_empty() {
                            (*line).to_owned()
                        } else {
                            format!("{}{single} {}", &line[..indent], &line[indent..])
                        }
                    })
                    .collect()
            }
        } else {
            let open = tokens.open.as_deref().filter(|s| !s.is_empty())?;
            let close = tokens.close.as_deref().filter(|s| !s.is_empty())?;
            let commented = non_blank().count() > 0
                && non_blank().all(|l| {
                    let t = l.trim();
                    t.starts_with(open) && t.ends_with(close) && t.len() >= open.len() + close.len()
                });
            bodies
                .iter()
                .map(|line| {
                    if line.trim().is_empty() {
                        (*line).to_owned()
                    } else if commented {
                        uncomment_block(line, open, close)
                    } else {
                        let indent = indentation(line);
                        format!(
                            "{}{open} {} {close}",
                            &line[..indent],
                            line[indent..].trim_end()
                        )
                    }
                })
                .collect()
        };

    Some(
        changed
            .iter()
            .zip(endings)
            .map(|(line, ending)| format!("{line}{ending}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Bytes of leading spaces and tabs. Only ASCII whitespace counts, so the
/// result is always a character boundary in every line it is applied to.
fn indentation(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn uncomment_single(line: &str, token: &str) -> String {
    let indent = indentation(line);
    match line[indent..].strip_prefix(token) {
        Some(rest) => format!(
            "{}{}",
            &line[..indent],
            rest.strip_prefix(' ').unwrap_or(rest)
        ),
        None => line.to_owned(),
    }
}

fn uncomment_block(line: &str, open: &str, close: &str) -> String {
    let indent = indentation(line);
    let body = line[indent..].trim_end();
    let inner = body
        .strip_prefix(open)
        .and_then(|b| b.strip_suffix(close))
        .unwrap_or(body);
    let inner = inner.strip_prefix(' ').unwrap_or(inner);
    let inner = inner.strip_suffix(' ').unwrap_or(inner);
    format!("{}{inner}", &line[..indent])
}

/// Text scanned for completions, in bytes (H2.3, threat T6).
pub const MAX_COMPLETION_SCAN: usize = 8 * 1024 * 1024;
/// Most completion candidates returned.
pub const MAX_COMPLETIONS: usize = 500;
/// Longest word offered, in bytes.
pub const MAX_WORD_BYTES: usize = 256;

/// Words of `text` (letters, digits, `_`) that start with `prefix` and are
/// longer than it, without duplicates, sorted. Only the first
/// `MAX_COMPLETION_SCAN` bytes are read.
pub fn completions(text: &str, prefix: &str) -> Vec<String> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let mut end = text.len().min(MAX_COMPLETION_SCAN);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut words = std::collections::BTreeSet::new();
    for word in text[..end].split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        if word.len() > prefix.len() && word.len() <= MAX_WORD_BYTES && word.starts_with(prefix) {
            words.insert(word);
        }
    }
    words
        .into_iter()
        .take(MAX_COMPLETIONS)
        .map(str::to_owned)
        .collect()
}

/// Lines that `detect_indent` reads; later lines are ignored (threat T6:
/// bounded work on untrusted files).
pub const MAX_INDENT_LINES: usize = 500;

/// Largest indentation step (in spaces) that `detect_indent` counts as a width.
pub const MAX_DETECTED_WIDTH: usize = 8;

/// Indentation style of a text, as seen in its first lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedIndent {
    /// Tabs are the more common indentation.
    pub use_tabs: bool,
    /// Most common step in spaces; `0` when the text has no space steps.
    pub width: usize,
}

/// Detects indentation from the first `MAX_INDENT_LINES` non-blank lines.
/// Returns `None` when no line is indented.
pub fn detect_indent(text: &str) -> Option<DetectedIndent> {
    let mut tab_lines = 0usize;
    let mut space_lines = 0usize;
    let mut votes = [0usize; MAX_DETECTED_WIDTH + 1];
    let mut previous = 0usize;
    for line in text.lines().take(MAX_INDENT_LINES) {
        let body = line.trim_start_matches([' ', '\t']);
        if body.is_empty() {
            continue; // blank lines say nothing about indentation
        }
        let lead = &line[..line.len() - body.len()];
        if lead.contains('\t') {
            tab_lines += 1;
            previous = 0;
            continue;
        }
        let spaces = lead.len();
        if spaces > 0 {
            space_lines += 1;
        }
        if spaces > previous && spaces - previous <= MAX_DETECTED_WIDTH {
            votes[spaces - previous] += 1;
        }
        previous = spaces;
    }
    if tab_lines == 0 && space_lines == 0 {
        return None;
    }
    let width = (1..=MAX_DETECTED_WIDTH)
        .max_by_key(|&w| (votes[w], std::cmp::Reverse(w)))
        .filter(|&w| votes[w] > 0)
        .unwrap_or(0);
    Some(DetectedIndent {
        use_tabs: tab_lines > space_lines,
        width,
    })
}
