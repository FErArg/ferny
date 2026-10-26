//! Fuzz target: build templates, placeholder expansion, output
//! neutralisation and error locations (threats T3, T6, T7). Must never
//! panic; neutralised output keeps no escape or bidi characters.
#![no_main]

use std::path::Path;

use ferny_build::{Placeholders, error_location, expand, neutralise, split};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(words) = split(&text) {
        assert!(!words.is_empty());
        let values = Placeholders::for_file(Path::new(text.as_ref()), 1);
        let _ = expand(&words, &values);
    }
    let shown = neutralise(data);
    assert!(!shown.contains('\u{1b}') && !shown.contains('\u{202E}'));
    for line in shown.lines() {
        if let Some(location) = error_location(line) {
            assert!(location.line > 0);
        }
    }
});
