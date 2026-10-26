//! Fuzz target: crafted `settings.toml` (H1.5a). Never panics; every
//! value that comes out is valid for its descriptor; saving round-trips.
#![no_main]

use ferny_config::settings::{DESCRIPTORS, Settings, parse_value};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let settings = Settings::from_user_text(text);
    for descriptor in DESCRIPTORS {
        let value = settings.get(descriptor.key).expect("every key has a value");
        assert!(parse_value(descriptor.kind, &value.to_text()).is_some());
    }
    let saved = settings.user_toml();
    let again = Settings::from_user_text(&saved);
    for descriptor in DESCRIPTORS {
        assert_eq!(settings.get(descriptor.key), again.get(descriptor.key));
    }
});
