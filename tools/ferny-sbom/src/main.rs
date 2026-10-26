//! `ferny-sbom --metadata <cargo metadata JSON> --lock <Cargo.lock>
//!  --third-party <third_party/README.md> --qt <version> --version <v>
//!  --timestamp <RFC 3339>`: prints the CycloneDX SBOM (H4.3).

use std::collections::HashMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Options of a release tool run by the maintainer, not a security decision.
    // nosemgrep: rust.lang.security.args.args
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = HashMap::new();
    for pair in args.chunks(2) {
        if let [key, value] = pair {
            opts.insert(key.trim_start_matches("--").to_owned(), value.clone());
        }
    }
    let get = |k: &str| opts.get(k).cloned().unwrap_or_default();
    let read = |k: &str| std::fs::read_to_string(get(k)).ok();
    let (Some(metadata), Some(lock), Some(readme)) =
        (read("metadata"), read("lock"), read("third-party"))
    else {
        eprintln!("ferny-sbom: --metadata, --lock and --third-party must be readable files");
        return ExitCode::from(2);
    };
    let Ok(metadata) = serde_json::from_str(&metadata) else {
        eprintln!("ferny-sbom: --metadata is not JSON");
        return ExitCode::from(2);
    };
    let sbom = ferny_sbom::cyclonedx(
        &metadata,
        &lock,
        &ferny_sbom::vendored_from_readme(&readme),
        &get("qt"),
        &get("version"),
        &get("timestamp"),
    );
    match serde_json::to_string_pretty(&sbom) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(_) => ExitCode::FAILURE,
    }
}
