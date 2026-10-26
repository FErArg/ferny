//! Fuzz target: what a language server sends (threats T6, T7): framing,
//! JSON, diagnostics, URIs and position conversion. Must never panic;
//! offsets stay inside the text.
#![no_main]

use std::io::Cursor;

use ferny_lsp::{byte_offset, parse_diagnostics, read_message, uri_to_path};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The JSON alone too: framing rarely survives mutation.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) {
        if let Some((uri, _)) = value.get("params").and_then(parse_diagnostics) {
            let _ = uri_to_path(&uri);
        }
        // Answers (H3.1b): bounded, and parameter ranges inside the label.
        assert!(ferny_lsp::parse_completion(&value).len() <= ferny_lsp::MAX_ITEMS);
        let _ = ferny_lsp::parse_hover(&value);
        if let Some((label, Some((start, end)))) = ferny_lsp::parse_signature(&value) {
            assert!(start <= end);
            let _ = label;
        }
        for (uri, _) in ferny_lsp::parse_locations(&value) {
            let _ = uri_to_path(&uri);
        }
        // Edits (H3.1c): applying them never panics.
        let text = String::from_utf8_lossy(data);
        if let Some(files) = ferny_lsp::parse_workspace_edit(&value) {
            for (_, edits) in files {
                let _ = ferny_lsp::apply_edits(&text, &edits, true);
            }
        }
        if let Some(edits) = ferny_lsp::parse_text_edits(&value) {
            let _ = ferny_lsp::apply_edits(&text, &edits, false);
        }
    }
    let mut reader = Cursor::new(data);
    while let Ok(body) = read_message(&mut reader) {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body)
            && let Some((uri, items)) = value.get("params").and_then(parse_diagnostics)
        {
            let _ = uri_to_path(&uri);
            let text = String::from_utf8_lossy(&body);
            for d in items {
                for utf16 in [true, false] {
                    let at = byte_offset(&text, d.start, utf16);
                    assert!(at <= text.len() && text.is_char_boundary(at));
                }
            }
        }
    }
});
