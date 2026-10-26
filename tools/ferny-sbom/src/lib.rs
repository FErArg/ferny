//! CycloneDX 1.5 SBOM of a Ferny build (H4.3, threat T14): the crates of
//! `cargo metadata` (licences) with the SHA-256 of `Cargo.lock`, the
//! vendored C/C++ code listed in `third_party/README.md`, Lua and Qt.
//! Deterministic: no random serial number; the time is the commit's.

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// Code built into Ferny that does not come from crates.io.
pub struct Vendored {
    /// Component name.
    pub name: String,
    /// Version.
    pub version: String,
    /// SPDX licence expression.
    pub license: String,
}

/// Licences of the vendored folders Ferny ships (`third_party/README.md`
/// does not record them; `docs/DEPENDENCIES.md` does).
fn vendored_license(name: &str) -> Option<&'static str> {
    match name {
        "scintilla" | "lexilla" => Some("HPND"),
        "geany" => Some("GPL-2.0-or-later"),
        _ => None, // build tools (corrosion, cxx-qt-cmake) are not shipped
    }
}

/// The shipped rows of the `third_party/README.md` table.
pub fn vendored_from_readme(readme: &str) -> Vec<Vendored> {
    readme
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let name = cells.get(1)?.trim_matches('`').trim_end_matches('/');
            let version = cells.get(2)?.split_whitespace().next()?;
            let license = vendored_license(name)?;
            Some(Vendored {
                name: name.to_owned(),
                version: version.to_owned(),
                license: license.to_owned(),
            })
        })
        .collect()
}

/// `name@version` → SHA-256 from `Cargo.lock`.
fn checksums(lock: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(table) = lock.parse::<toml::Table>() else {
        return out;
    };
    for package in table
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let field = |k: &str| package.get(k).and_then(toml::Value::as_str);
        if let (Some(name), Some(version), Some(sum)) =
            (field("name"), field("version"), field("checksum"))
        {
            out.insert(format!("{name}@{version}"), sum.to_owned());
        }
    }
    out
}

/// The SBOM document.
pub fn cyclonedx(
    metadata: &Value,
    lock: &str,
    vendored: &[Vendored],
    qt_version: &str,
    ferny_version: &str,
    timestamp: &str,
) -> Value {
    let sums = checksums(lock);
    let mut components = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        // Workspace members have no source: they are Ferny itself.
        let Some(source) = package["source"].as_str() else {
            continue;
        };
        let name = package["name"].as_str().unwrap_or_default();
        let version = package["version"].as_str().unwrap_or_default();
        let mut component = json!({
            "type": "library",
            "name": name,
            "version": version,
            "purl": format!("pkg:cargo/{name}@{version}"),
            "externalReferences": [{"type": "distribution", "url": source}],
        });
        if let Some(license) = package["license"].as_str() {
            component["licenses"] = json!([{"expression": license}]);
        }
        if let Some(sum) = sums.get(&format!("{name}@{version}")) {
            component["hashes"] = json!([{"alg": "SHA-256", "content": sum}]);
        }
        components.push(component);
    }
    for v in vendored {
        components.push(json!({
            "type": "library",
            "name": v.name,
            "version": v.version,
            "purl": format!("pkg:generic/{}@{}", v.name, v.version),
            "licenses": [{"expression": v.license}],
            "description": "vendored in third_party/",
        }));
    }
    components.push(json!({
        "type": "framework",
        "name": "qt",
        "version": qt_version,
        "purl": format!("pkg:generic/qt@{qt_version}"),
        "licenses": [{"expression": "LGPL-3.0-only"}],
        "description": "dynamically linked, bundled",
    }));
    components.sort_by(|a, b| {
        (a["name"].as_str(), a["version"].as_str())
            .cmp(&(b["name"].as_str(), b["version"].as_str()))
    });
    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "timestamp": timestamp,
            "tools": {"components": [{"type": "application", "name": "ferny-sbom", "version": ferny_version}]},
            "component": {
                "type": "application",
                "name": "ferny",
                "version": ferny_version,
                "licenses": [{"expression": "GPL-2.0-or-later"}],
            },
        },
        "components": components,
    })
}
