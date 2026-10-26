//! Fuzz target: arbitrary patterns (plain and regex) over arbitrary text
//! (threat T6). Must never panic; matches must be valid byte ranges.
#![no_main]

use ferny_core::search::{self, Query};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&flags, rest)) = data.split_first() else {
        return;
    };
    let text = String::from_utf8_lossy(rest);
    // First line is the pattern, the rest the text.
    let (pattern, body) = text.split_once('\n').unwrap_or((&text, ""));
    let query = Query {
        pattern: pattern.to_owned(),
        regex: flags & 1 != 0,
        match_case: flags & 2 != 0,
        whole_word: flags & 4 != 0,
    };
    let from = usize::from(flags) % (body.len() + 1);
    if let Ok(Some((start, end))) = search::find(body, &query, from, flags & 8 != 0) {
        assert!(start < end && end <= body.len());
        assert!(body.is_char_boundary(start) && body.is_char_boundary(end));
        let _ = search::replacement_for(body, &query, start, end, "$1x");
    }
    let _ = search::replace_all(body, &query, "$0");
});
