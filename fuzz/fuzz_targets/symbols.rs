//! Fuzz target: arbitrary text for each tree-sitter grammar (threats T6,
//! T14). Must never panic or hang; symbols must point inside the text.
#![no_main]

use ferny_syntax::symbols;
use libfuzzer_sys::fuzz_target;

const FILETYPES: [&str; 5] = ["c", "cpp", "rust", "python", "javascript"];

fuzz_target!(|data: &[u8]| {
    let Some((&which, rest)) = data.split_first() else {
        return;
    };
    let text = String::from_utf8_lossy(rest);
    let filetype = FILETYPES[usize::from(which) % FILETYPES.len()];
    if let Ok(found) = symbols::extract(filetype, &text) {
        let lines = text.split('\n').count();
        for symbol in found {
            assert!(symbol.line >= 1 && symbol.line as usize <= lines);
            assert!(!symbol.kind.is_empty());
        }
    }
});
