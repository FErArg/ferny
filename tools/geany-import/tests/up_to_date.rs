//! The generated `data/` matches what the converter produces from the
//! vendored Geany data: nobody edited it by hand and it is not stale.

use std::fs;
use std::path::PathBuf;

#[test]
fn generated_data_is_up_to_date() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let files = geany_import::generate(&root).expect("generate");
    assert!(files.len() >= 136);
    for (relative, text) in &files {
        let on_disk = fs::read_to_string(root.join(relative)).unwrap_or_default();
        assert!(
            on_disk == *text,
            "{} is out of date: run `cargo run -p geany-import -- .`",
            relative.display()
        );
    }
    // No stray files in the generated directories.
    for dir in [
        "data/filetypes",
        "data/colorschemes",
        "data/templates/files",
    ] {
        for entry in fs::read_dir(root.join(dir)).expect("read dir") {
            let path = PathBuf::from(dir).join(entry.expect("entry").file_name());
            assert!(files.contains_key(&path), "unexpected {}", path.display());
        }
    }
}
