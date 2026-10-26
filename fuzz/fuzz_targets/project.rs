//! Fuzz target: project files and Geany project files (threats T3, T6,
//! T11). Must never panic; whatever is accepted stays inside the project
//! and holds only valid commands.
#![no_main]

use std::path::Path;

use ferny_config::project::{self, from_text, plan_geany_project_text};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let root = std::env::temp_dir();
    if let Ok((opened, _)) = from_text(&root, &text) {
        assert!(opened.base.starts_with(&opened.root));
        assert!(opened.build.values().all(|t| project::valid_command(t)));
    }
    if let Ok(plan) = plan_geany_project_text(&text, Path::new(&root)) {
        assert!(plan.target.is_dir());
    }
});
