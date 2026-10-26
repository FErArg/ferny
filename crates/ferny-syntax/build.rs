//! Embeds the generated filetypes and colour schemes (`data/`) into the
//! binary, so Ferny needs no data directory at runtime.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

fn list(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("{}: {err}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    files
}

fn main() {
    let root =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("../../data");
    let mut out = String::new();
    for (name, sub) in [
        ("FILETYPES", "filetypes"),
        ("COLOR_SCHEMES", "colorschemes"),
    ] {
        let dir = root.join(sub);
        println!("cargo::rerun-if-changed={}", dir.display());
        let files = list(&dir);
        writeln!(out, "pub(crate) static {name}: &[&str] = &[").expect("write");
        for file in files {
            let path = file.canonicalize().expect("canonical path");
            writeln!(out, "    include_str!({:?}),", path.display().to_string()).expect("write");
        }
        writeln!(out, "];").expect("write");
    }
    let dest = PathBuf::from(env::var("OUT_DIR").expect("out dir")).join("embedded.rs");
    fs::write(dest, out).expect("write embedded.rs");
}
