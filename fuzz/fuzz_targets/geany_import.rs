//! Fuzz target: crafted Geany `geany.conf` / `keybindings.conf` (threat
//! T11). Never panics; every mapped shortcut is a valid key sequence and
//! every mapped setting a known key.
#![no_main]

use ferny_config::geany::{self, ImportPlan};
use ferny_config::settings;
use ferny_core::command::{builtin_registry, is_valid_key_sequence};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let registry = builtin_registry();
    let mut plan = ImportPlan::default();
    geany::settings_from_text(text, &mut plan);
    geany::keybindings_from_text(text, &registry, &mut plan);
    for (key, _) in &plan.settings {
        assert!(settings::descriptor(key).is_some(), "{key}");
    }
    for (id, keys) in &plan.keybindings {
        assert!(registry.get(id).is_some());
        assert!(keys.iter().all(|k| is_valid_key_sequence(k)));
    }
    let _ = geany::gtk_accelerator(text);
});
