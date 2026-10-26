//! Fuzz target: arbitrary bytes as indentation input (threat T6). Must never
//! panic; the result is only a width and a style.
#![no_main]

use ferny_core::editing::{MAX_DETECTED_WIDTH, detect_indent};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Some(found) = detect_indent(&text) {
        assert!(found.width <= MAX_DETECTED_WIDTH);
    }
});
