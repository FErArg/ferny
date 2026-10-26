//! SBOM generation (H4.3).

use ferny_sbom::{cyclonedx, vendored_from_readme};
use serde_json::json;

#[test]
fn sbom_lists_crates_vendored_code_and_qt() {
    let metadata = json!({"packages": [
        {"name": "ferny-core", "version": "0.4.7", "license": "GPL-2.0-or-later", "source": null},
        {"name": "toml", "version": "1.1.8", "license": "MIT OR Apache-2.0",
         "source": "registry+https://github.com/rust-lang/crates.io-index"},
        {"name": "nolicense", "version": "0.1.0", "license": null,
         "source": "registry+https://github.com/rust-lang/crates.io-index"}
    ]});
    let lock = "[[package]]\nname = \"toml\"\nversion = \"1.1.8\"\nchecksum = \"abc123\"\n";
    let readme = "| Directory | Version | Manifest |\n|---|---|---|\n\
                  | `scintilla/` | 5.6.7 (includes the Qt layer) | `scintilla.sha256` |\n\
                  | `corrosion/` | 0.6.1 (cmake) | `corrosion.sha256` |\n\
                  | `geany/` | 2.1.0 subset | `geany.sha256` |\n";
    let vendored = vendored_from_readme(readme);
    assert_eq!(vendored.len(), 2, "build tools are not shipped");
    let sbom = cyclonedx(
        &metadata,
        lock,
        &vendored,
        "6.12.0",
        "0.4.7",
        "2026-10-17T00:00:00Z",
    );
    assert_eq!(sbom["bomFormat"], "CycloneDX");
    assert_eq!(sbom["metadata"]["component"]["name"], "ferny");
    let components = sbom["components"].as_array().expect("components");
    let names: Vec<&str> = components
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(names, ["geany", "nolicense", "qt", "scintilla", "toml"]);
    let toml = &components[4];
    assert_eq!(toml["purl"], "pkg:cargo/toml@1.1.8");
    assert_eq!(toml["hashes"][0]["content"], "abc123");
    assert_eq!(toml["licenses"][0]["expression"], "MIT OR Apache-2.0");
    assert_eq!(components[3]["licenses"][0]["expression"], "HPND");
    assert!(components[1].get("licenses").is_none());
    // Deterministic: the same input gives the same document.
    assert_eq!(
        sbom,
        cyclonedx(
            &metadata,
            lock,
            &vendored,
            "6.12.0",
            "0.4.7",
            "2026-10-17T00:00:00Z"
        )
    );
}
