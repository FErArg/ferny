//! LSP client core (H3.1a, threats T1, T2, T6, T7, T16).
//!
//! Servers run only for documents of trusted workspaces (checked by the
//! caller), with a reduced environment, and everything they send is
//! untrusted: messages are bounded, parsed with `serde_json`, and only the
//! parts Ferny uses are read. Secret-looking files are never sent.
#![forbid(unsafe_code)]

pub mod client;
pub mod config;

use std::ffi::OsString;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

/// Largest message accepted from a server, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
/// Largest header block, in bytes.
pub const MAX_HEADER_BYTES: usize = 8 * 1024;
/// Most diagnostics kept per document.
pub const MAX_DIAGNOSTICS: usize = 1000;
/// Longest diagnostic message kept, in bytes.
pub const MAX_DIAGNOSTIC_BYTES: usize = 4096;

/// Why a message could not be read. No contents inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// The stream ended.
    #[error("closed")]
    Closed,
    /// Bad headers, missing or oversized length.
    #[error("invalid message")]
    Invalid,
}

/// Reads one `Content-Length` framed message.
pub fn read_message(reader: &mut impl BufRead) -> Result<Vec<u8>, FrameError> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = Vec::new();
        // Bounded read of one header line.
        let read = (&mut *reader)
            .take((MAX_HEADER_BYTES - header_bytes + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|_| FrameError::Closed)?;
        if read == 0 {
            return Err(FrameError::Closed);
        }
        header_bytes += read;
        if header_bytes > MAX_HEADER_BYTES || !line.ends_with(b"\n") {
            return Err(FrameError::Invalid);
        }
        let text = std::str::from_utf8(&line).map_err(|_| FrameError::Invalid)?;
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            break;
        }
        let (name, value) = text.split_once(':').ok_or(FrameError::Invalid)?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            let n: usize = value.trim().parse().map_err(|_| FrameError::Invalid)?;
            length = Some(n);
        }
    }
    let length = length
        .filter(|n| *n <= MAX_MESSAGE_BYTES)
        .ok_or(FrameError::Invalid)?;
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|_| FrameError::Closed)?;
    Ok(body)
}

/// Writes one framed message.
pub fn write_message(writer: &mut impl Write, body: &[u8]) -> std::io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body)?;
    writer.flush()
}

/// Whether `path` looks like a file holding secrets: never sent to a
/// server (T16).
pub fn is_secret_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return true; // a name that is not UTF-8: do not send
    };
    let lower = name.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || [".pem", ".key", ".p12", ".pfx", ".kdbx"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        || ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa", "credentials"]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        || [".netrc", ".npmrc", ".pypirc"].contains(&lower.as_str())
}

/// The environment of a server: only the allowlisted variables (plus
/// `extra` names), with `PATH` stripped of empty, relative and workspace
/// entries.
pub fn server_environment(
    current: impl Iterator<Item = (OsString, OsString)>,
    extra: &[String],
    workspace: &Path,
) -> Vec<(OsString, OsString)> {
    let workspace = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let mut out = Vec::new();
    for (name, value) in current {
        let Some(key) = name.to_str() else {
            continue;
        };
        if key == "PATH" {
            let kept: Vec<PathBuf> = std::env::split_paths(&value)
                .filter(|p| p.is_absolute())
                .filter(|p| {
                    let real = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
                    !real.starts_with(&workspace)
                })
                .collect();
            if let Ok(path) = std::env::join_paths(kept) {
                out.push((name, path));
            }
        } else if matches!(key, "HOME" | "USER" | "LANG" | "TMPDIR")
            || key.starts_with("LC_")
            || extra.iter().any(|e| e == key)
        {
            out.push((name, value));
        }
    }
    out
}

/// `file://` URI of an absolute path (percent-encoded).
pub fn path_to_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    let text = path.to_string_lossy().replace('\\', "/");
    if !text.starts_with('/') {
        uri.push('/'); // Windows: file:///C:/…
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            uri.push(byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// The path of a `file://` URI; `None` for anything else or bad escapes.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(rest.len());
    let mut iter = rest.bytes();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let hex = [iter.next()?, iter.next()?];
            let text = std::str::from_utf8(&hex).ok()?;
            bytes.push(u8::from_str_radix(text, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    let text = String::from_utf8(bytes).ok()?;
    if text.chars().any(char::is_control) {
        return None;
    }
    // file:///C:/x on Windows.
    let text = if cfg!(windows) {
        text.trim_start_matches('/').to_owned()
    } else {
        text
    };
    let path = PathBuf::from(text);
    path.is_absolute().then_some(path)
}

/// One diagnostic as the server sent it (positions not yet converted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Start line and character (in the negotiated encoding).
    pub start: (u32, u32),
    /// End line and character.
    pub end: (u32, u32),
    /// 1 error, 2 warning, 3 information, 4 hint.
    pub severity: u8,
    /// The message, neutralised and bounded.
    pub message: String,
}

/// Parses the `params` of `textDocument/publishDiagnostics`.
pub fn parse_diagnostics(params: &serde_json::Value) -> Option<(String, Vec<Diagnostic>)> {
    let uri = params.get("uri")?.as_str()?.to_owned();
    let list = params.get("diagnostics")?.as_array()?;
    let position = |v: Option<&serde_json::Value>| -> Option<(u32, u32)> {
        let v = v?;
        let line = u32::try_from(v.get("line")?.as_u64()?).ok()?;
        let character = u32::try_from(v.get("character")?.as_u64()?).ok()?;
        Some((line, character))
    };
    let diagnostics = list
        .iter()
        .take(MAX_DIAGNOSTICS)
        .filter_map(|d| {
            let range = d.get("range")?;
            let mut message = d.get("message")?.as_str()?.to_owned();
            if message.len() > MAX_DIAGNOSTIC_BYTES {
                let mut end = MAX_DIAGNOSTIC_BYTES;
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message.truncate(end);
            }
            Some(Diagnostic {
                start: position(range.get("start"))?,
                end: position(range.get("end"))?,
                severity: d
                    .get("severity")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|s| u8::try_from(s).ok())
                    .filter(|s| (1..=4).contains(s))
                    .unwrap_or(1),
                message: ferny_build::neutralise(message.as_bytes()),
            })
        })
        .collect();
    Some((uri, diagnostics))
}

/// Byte offset in `text` of a server position. `utf16`: the character is
/// counted in UTF-16 code units (LSP's default), else in bytes. Clamped to
/// the line and the document.
pub fn byte_offset(text: &str, (line, character): (u32, u32), utf16: bool) -> usize {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len(),
        }
    }
    let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let line_text = &text[start..line_end];
    let character = character as usize;
    if !utf16 {
        let mut end = character.min(line_text.len());
        while !line_text.is_char_boundary(end) {
            end -= 1;
        }
        return start + end;
    }
    let mut units = 0;
    for (index, c) in line_text.char_indices() {
        if units >= character {
            return start + index;
        }
        units += c.len_utf16();
    }
    line_end
}

/// The server position of byte `offset` in `text` (inverse of
/// [`byte_offset`]); `utf16`: characters in UTF-16 code units.
pub fn lsp_position(text: &str, offset: usize, utf16: bool) -> (u32, u32) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let in_line = &before[line_start..];
    let character = if utf16 {
        in_line.chars().map(char::len_utf16).sum()
    } else {
        in_line.len()
    };
    (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(character).unwrap_or(u32::MAX),
    )
}

/// Most items kept from one answer (completion, locations).
pub const MAX_ITEMS: usize = 500;
/// Longest text kept from one item or hover, in bytes.
pub const MAX_ITEM_BYTES: usize = 4096;

fn bounded(text: &str) -> String {
    let mut end = text.len().min(MAX_ITEM_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    neutralised(&text[..end])
}

fn neutralised(text: &str) -> String {
    ferny_build::neutralise(text.as_bytes())
}

/// Completion answer: the text each item inserts (plain text only; Ferny
/// does not announce snippet support), in the server's order.
pub fn parse_completion(result: &serde_json::Value) -> Vec<String> {
    let items = result
        .get("items")
        .and_then(serde_json::Value::as_array)
        .or_else(|| result.as_array());
    let Some(items) = items else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let text = item
                .pointer("/textEdit/newText")
                .or_else(|| item.get("insertText"))
                .or_else(|| item.get("label"))?
                .as_str()?;
            let text = bounded(text.trim());
            (!text.is_empty() && !text.contains('\n')).then_some(text)
        })
        .take(MAX_ITEMS)
        .collect()
}

/// Hover answer as plain text (markdown kept as written).
pub fn parse_hover(result: &serde_json::Value) -> String {
    fn piece(v: &serde_json::Value) -> Option<String> {
        v.as_str().map(str::to_owned).or_else(|| {
            v.get("value")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
    }
    let Some(contents) = result.get("contents") else {
        return String::new();
    };
    let text = match contents.as_array() {
        Some(list) => list.iter().filter_map(piece).collect::<Vec<_>>().join("\n"),
        None => piece(contents).unwrap_or_default(),
    };
    bounded(text.trim())
}

/// Signature help answer: the active signature's label and the byte range
/// of its active parameter in that label (if known).
pub fn parse_signature(result: &serde_json::Value) -> Option<(String, Option<(usize, usize)>)> {
    let signatures = result.get("signatures")?.as_array()?;
    let active = result
        .get("activeSignature")
        .and_then(serde_json::Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
        .unwrap_or(0);
    let signature = signatures.get(active).or_else(|| signatures.first())?;
    let label = signature.get("label")?.as_str()?;
    if label.len() > MAX_ITEM_BYTES || label.contains('\n') {
        return None;
    }
    let parameter = signature
        .get("activeParameter")
        .or_else(|| result.get("activeParameter"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| signature.get("parameters")?.as_array()?.get(i));
    let range = parameter.and_then(|p| {
        let label_of = p.get("label")?;
        if let Some(text) = label_of.as_str() {
            let start = label.find(text)?;
            Some((start, start + text.len()))
        } else {
            // [start, end] in UTF-16 code units of the label.
            let pair = label_of.as_array()?;
            let to_byte = |units: u64| -> Option<usize> {
                let mut count = 0u64;
                for (i, c) in label.char_indices() {
                    if count >= units {
                        return Some(i);
                    }
                    count += c.len_utf16() as u64;
                }
                (count >= units).then_some(label.len())
            };
            Some((
                to_byte(pair.first()?.as_u64()?)?,
                to_byte(pair.get(1)?.as_u64()?)?,
            ))
        }
    });
    Some((neutralised(label), range.filter(|(s, e)| s <= e)))
}

/// A location in an answer: URI and start position.
pub type Location = (String, (u32, u32));

/// Definition or references answer: `Location`, `Location[]` or
/// `LocationLink[]`.
pub fn parse_locations(result: &serde_json::Value) -> Vec<Location> {
    let one = |v: &serde_json::Value| -> Option<Location> {
        let uri = v
            .get("uri")
            .or_else(|| v.get("targetUri"))?
            .as_str()?
            .to_owned();
        let start = v
            .pointer("/range/start")
            .or_else(|| v.pointer("/targetSelectionRange/start"))?;
        let line = u32::try_from(start.get("line")?.as_u64()?).ok()?;
        let character = u32::try_from(start.get("character")?.as_u64()?).ok()?;
        Some((uri, (line, character)))
    };
    match result.as_array() {
        Some(list) => list.iter().take(MAX_ITEMS).filter_map(one).collect(),
        None => one(result).into_iter().collect(),
    }
}

/// The sidebar's name for an LSP `SymbolKind`; `None` for kinds the
/// Symbols sidebar does not list (variables, fields, properties…).
pub fn symbol_kind_name(kind: u64) -> Option<&'static str> {
    Some(match kind {
        2..=4 => "module",
        5 => "class",
        6 | 9 => "method",
        10 | 23 | 26 => "type",
        11 => "interface",
        12 => "function",
        14 | 22 => "constant",
        _ => return None,
    })
}

/// Most levels of nested document symbols read.
const MAX_SYMBOL_DEPTH: usize = 8;

/// A symbol: name (neutralised), LSP kind and location (URI empty for the
/// hierarchical `DocumentSymbol` form, which belongs to the asked document).
pub type SymbolInfo = (String, u64, Location);

/// `textDocument/documentSymbol` or `workspace/symbol` answer:
/// `DocumentSymbol[]` (nested, flattened), `SymbolInformation[]` or
/// `WorkspaceSymbol[]`; at most `MAX_ITEMS` (H4.2a).
pub fn parse_symbols(result: &serde_json::Value) -> Vec<SymbolInfo> {
    fn walk(list: &serde_json::Value, depth: usize, out: &mut Vec<SymbolInfo>) {
        let Some(list) = list.as_array() else {
            return;
        };
        for item in list {
            if out.len() >= MAX_ITEMS {
                return;
            }
            let (Some(name), Some(kind)) = (
                item.get("name").and_then(serde_json::Value::as_str),
                item.get("kind").and_then(serde_json::Value::as_u64),
            ) else {
                continue;
            };
            let uri = item
                .pointer("/location/uri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let start = item
                .pointer("/selectionRange/start")
                .or_else(|| item.pointer("/location/range/start"))
                .or_else(|| item.pointer("/range/start"));
            let position = start
                .and_then(|s| {
                    Some((
                        u32::try_from(s.get("line")?.as_u64()?).ok()?,
                        u32::try_from(s.get("character")?.as_u64()?).ok()?,
                    ))
                })
                .unwrap_or((0, 0));
            out.push((neutralised(name), kind, (uri.to_owned(), position)));
            if depth + 1 < MAX_SYMBOL_DEPTH
                && let Some(children) = item.get("children")
            {
                walk(children, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(result, 0, &mut out);
    out
}

/// Most files a workspace edit may touch.
pub const MAX_EDIT_FILES: usize = 200;
/// Most text edits in one answer.
pub const MAX_EDITS: usize = 10_000;
/// Largest total of new text in one answer, in bytes.
pub const MAX_EDIT_BYTES: usize = 16 * 1024 * 1024;

/// One text edit: start, end (server positions) and the new text.
pub type TextEdit = ((u32, u32), (u32, u32), String);

fn parse_edit_list(list: &serde_json::Value, budget: &mut (usize, usize)) -> Option<Vec<TextEdit>> {
    let position = |v: Option<&serde_json::Value>| -> Option<(u32, u32)> {
        let v = v?;
        Some((
            u32::try_from(v.get("line")?.as_u64()?).ok()?,
            u32::try_from(v.get("character")?.as_u64()?).ok()?,
        ))
    };
    let mut edits = Vec::new();
    for edit in list.as_array()? {
        let text = edit.get("newText")?.as_str()?;
        budget.0 += 1;
        budget.1 += text.len();
        if budget.0 > MAX_EDITS || budget.1 > MAX_EDIT_BYTES {
            return None; // too large: refused as a whole
        }
        let range = edit.get("range")?;
        edits.push((
            position(range.get("start"))?,
            position(range.get("end"))?,
            text.to_owned(),
        ));
    }
    Some(edits)
}

/// Formatting answer: the edits of the document (`None` if malformed or
/// too large; an empty list means nothing to change).
pub fn parse_text_edits(result: &serde_json::Value) -> Option<Vec<TextEdit>> {
    if result.is_null() {
        return Some(Vec::new());
    }
    parse_edit_list(result, &mut (0, 0))
}

/// Rename answer (`WorkspaceEdit`): edits per URI, from `changes` or
/// `documentChanges`. File operations (create, rename, delete) are not
/// supported and make the whole answer refused, as do malformed or
/// oversized answers.
pub fn parse_workspace_edit(result: &serde_json::Value) -> Option<Vec<(String, Vec<TextEdit>)>> {
    let mut budget = (0, 0);
    let mut files = Vec::new();
    if let Some(changes) = result
        .get("documentChanges")
        .and_then(serde_json::Value::as_array)
    {
        for change in changes {
            if change.get("kind").is_some() {
                return None; // a file operation
            }
            let uri = change.pointer("/textDocument/uri")?.as_str()?.to_owned();
            files.push((uri, parse_edit_list(change.get("edits")?, &mut budget)?));
        }
    } else if let Some(changes) = result.get("changes").and_then(serde_json::Value::as_object) {
        for (uri, edits) in changes {
            files.push((uri.clone(), parse_edit_list(edits, &mut budget)?));
        }
    }
    (files.len() <= MAX_EDIT_FILES).then_some(files)
}

/// Applies `edits` to `text`: positions converted (UTF-16 or bytes),
/// edits applied from the end; `None` if two edits overlap.
pub fn apply_edits(text: &str, edits: &[TextEdit], utf16: bool) -> Option<String> {
    let mut ranges: Vec<(usize, usize, &str)> = edits
        .iter()
        .map(|(start, end, new)| {
            let s = byte_offset(text, *start, utf16);
            let e = byte_offset(text, *end, utf16).max(s);
            (s, e, new.as_str())
        })
        .collect();
    ranges.sort_by_key(|r| (r.0, r.1));
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        return None;
    }
    let mut out = text.to_owned();
    for (start, end, new) in ranges.into_iter().rev() {
        out.replace_range(start..end, new);
    }
    Some(out)
}
