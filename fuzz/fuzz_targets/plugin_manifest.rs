//! Fuzz target: plugin manifests (threats T4, T17). Must never panic; an
//! accepted manifest has a valid id equal to its folder and only known
//! permissions.
#![no_main]

use ferny_plugin::{PERMISSIONS, parse_manifest};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(manifest) = parse_manifest(&text, "demo") {
        assert_eq!(manifest.id, "demo");
        assert!(manifest.permissions.iter().all(|p| PERMISSIONS.contains(&p.as_str())));
    }
});
