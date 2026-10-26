//! Usage: `cargo run -p geany-import -- <repository root> [--check]`
//!
//! Writes `data/filetypes/*.toml` and `data/colorschemes/*.toml`. With
//! `--check`, writes nothing and fails if the files on disk differ.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Arguments only select the repository to read and whether to check;
    // argv[0] is skipped and nothing security-relevant depends on them.
    // nosemgrep: rust.lang.security.args.args
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let Some(root) = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
    else {
        eprintln!("usage: geany-import <repository root> [--check]");
        return ExitCode::from(2);
    };
    let files = match geany_import::generate(&root) {
        Ok(files) => files,
        Err(err) => {
            eprintln!("geany-import: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut stale = 0;
    for (relative, text) in &files {
        let path = root.join(relative);
        if check {
            if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                eprintln!("out of date: {}", relative.display());
                stale += 1;
            }
            continue;
        }
        if let Some(parent) = path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            eprintln!("geany-import: {err}");
            return ExitCode::FAILURE;
        }
        if let Err(err) = fs::write(&path, text) {
            eprintln!("geany-import: {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    }
    if stale > 0 {
        return ExitCode::FAILURE;
    }
    println!(
        "geany-import: {} files {}",
        files.len(),
        if check { "up to date" } else { "written" }
    );
    ExitCode::SUCCESS
}
