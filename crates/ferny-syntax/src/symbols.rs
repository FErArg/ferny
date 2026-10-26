//! Symbols of a document with tree-sitter (H2.2, threats T6, T14).
//!
//! The text is untrusted: sources above `MAX_SOURCE_BYTES` are not parsed,
//! parsing and querying stop after `TIME_BUDGET`, and at most `MAX_SYMBOLS`
//! are returned. Definitions come from each grammar's own `tags.scm`.

use std::ops::ControlFlow;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tree_sitter::{
    Language, ParseOptions, Parser, Query, QueryCursor, QueryCursorOptions, StreamingIterator,
};

/// Largest source parsed, in bytes.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;
/// Most symbols returned.
pub const MAX_SYMBOLS: usize = 10_000;
/// Time allowed for parsing plus querying one document.
pub const TIME_BUDGET: Duration = Duration::from_secs(1);

/// One definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Name as written in the source.
    pub name: String,
    /// `function`, `method`, `class`, `interface`, `module`, `macro`, …
    /// (the suffix of the grammar's `@definition.*` capture).
    pub kind: String,
    /// 1-based line.
    pub line: u32,
}

/// Why no symbols were produced. No contents inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SymbolError {
    /// No grammar for this filetype.
    #[error("no symbols for this filetype")]
    Unsupported,
    /// Larger than `MAX_SOURCE_BYTES`.
    #[error("file too large for symbols")]
    TooLarge,
    /// `TIME_BUDGET` exceeded.
    #[error("symbols took too long")]
    Timeout,
}

struct Grammar {
    filetype: &'static str,
    language: Language,
    tags: Query,
}

fn grammars() -> &'static [Grammar] {
    static GRAMMARS: OnceLock<Vec<Grammar>> = OnceLock::new();
    GRAMMARS.get_or_init(|| {
        let all: [(&str, Language, &str); 5] = [
            (
                "c",
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::TAGS_QUERY,
            ),
            (
                "cpp",
                tree_sitter_cpp::LANGUAGE.into(),
                tree_sitter_cpp::TAGS_QUERY,
            ),
            (
                "rust",
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::TAGS_QUERY,
            ),
            (
                "python",
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::TAGS_QUERY,
            ),
            (
                "javascript",
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::TAGS_QUERY,
            ),
        ];
        all.into_iter()
            .filter_map(|(filetype, language, tags)| {
                // The queries ship with the grammars; a broken one only
                // disables symbols for that filetype.
                let tags = Query::new(&language, tags).ok()?;
                Some(Grammar {
                    filetype,
                    language,
                    tags,
                })
            })
            .collect()
    })
}

/// Whether `filetype` (a Ferny filetype id) has symbols.
pub fn supported(filetype: &str) -> bool {
    grammars().iter().any(|g| g.filetype == filetype)
}

/// Every symbol kind the grammars can report, sorted (for translations).
pub fn kinds() -> Vec<&'static str> {
    let mut kinds: Vec<&str> = grammars()
        .iter()
        .flat_map(|g| g.tags.capture_names().iter().copied())
        .filter_map(|name| name.strip_prefix("definition."))
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// The definitions in `source`, by line, without duplicates.
pub fn extract(filetype: &str, source: &str) -> Result<Vec<Symbol>, SymbolError> {
    let grammar = grammars()
        .iter()
        .find(|g| g.filetype == filetype)
        .ok_or(SymbolError::Unsupported)?;
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SymbolError::TooLarge);
    }
    let deadline = Instant::now() + TIME_BUDGET;
    let mut stop = |_: &_| {
        if Instant::now() > deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };

    let mut parser = Parser::new();
    parser
        .set_language(&grammar.language)
        .map_err(|_| SymbolError::Unsupported)?;
    let bytes = source.as_bytes();
    let tree = parser
        .parse_with_options(
            &mut |offset, _| bytes.get(offset..).unwrap_or_default(),
            None,
            Some(ParseOptions::new().progress_callback(&mut stop)),
        )
        .ok_or(SymbolError::Timeout)?;

    let names = grammar.tags.capture_names();
    let mut cursor = QueryCursor::new();
    cursor.set_match_limit(MAX_SYMBOLS as u32);
    let mut stop_query = |_: &_| {
        if Instant::now() > deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let mut matches = cursor.matches_with_options(
        &grammar.tags,
        tree.root_node(),
        bytes,
        QueryCursorOptions::new().progress_callback(&mut stop_query),
    );
    let mut symbols = Vec::new();
    while let Some(m) = matches.next() {
        let mut name = None;
        let mut kind = None;
        for capture in m.captures() {
            let capture_name = names[capture.index as usize];
            if capture_name == "name" {
                name = capture.node.utf8_text(bytes).ok();
            } else if let Some(k) = capture_name.strip_prefix("definition.") {
                kind = Some((k, capture.node.start_position().row));
            }
        }
        if let (Some(name), Some((kind, row))) = (name, kind) {
            symbols.push(Symbol {
                name: name.to_owned(),
                kind: kind.to_owned(),
                line: u32::try_from(row + 1).unwrap_or(u32::MAX),
            });
            if symbols.len() >= MAX_SYMBOLS {
                break;
            }
        }
    }
    if Instant::now() > deadline {
        return Err(SymbolError::Timeout);
    }
    // A definition can match two patterns (a Rust method is also a
    // function): keep the first, which is the more specific one.
    symbols.sort_by_key(|s| s.line);
    let mut seen = std::collections::HashSet::new();
    symbols.retain(|s| seen.insert((s.line, s.name.clone())));
    Ok(symbols)
}
