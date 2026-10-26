//! Projects: `<root>/.ferny/project.toml` and the `.geany` importer (H2.5a,
//! threats T1, T3, T10, T11).
//!
//! A project file usually comes with the code it describes, so it is
//! untrusted: it is read strictly and bounded, its base folder must stay
//! inside the project, and its build commands are validated here and run
//! only when the project root is trusted (checked by the caller).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ferny_build::{Placeholders, expand, split};
use ferny_core::fsio::{self, FileMode};
use serde::{Deserialize, Serialize};

use crate::geany::{KeyFile, read};

/// The project file, relative to the project root.
pub const PROJECT_FILE: &str = ".ferny/project.toml";
/// Largest project file read, in bytes.
pub const MAX_PROJECT_BYTES: u64 = 64 * 1024;
/// Most file patterns kept.
pub const MAX_PATTERNS: usize = 64;
/// Commands a project may set: the filetype ones plus Make.
pub const PROJECT_COMMANDS: [&str; 5] = ["compile", "build", "lint", "make", "execute"];

const VERSION: u32 = 1;

/// Why a project cannot be opened or written. No paths inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectError {
    /// No `.ferny/project.toml` in that folder.
    #[error("no project in this folder")]
    NotFound,
    /// Too large, not UTF-8, not TOML, or an unknown version.
    #[error("invalid project file")]
    Invalid,
    /// A project file already exists (never overwritten).
    #[error("a project already exists in this folder")]
    Exists,
    /// It could not be written.
    #[error("I/O error")]
    Io,
}

/// An open project, validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// Canonical folder holding `.ferny/project.toml` (the workspace).
    pub root: PathBuf,
    /// Display name.
    pub name: String,
    /// Canonical base folder (inside `root`): Find in Files and project
    /// commands start here.
    pub base: PathBuf,
    /// File patterns for Find in Files.
    pub patterns: Vec<String>,
    /// Indentation width (1–16), if set.
    pub indent_width: Option<u32>,
    /// Indentation with tabs, if set.
    pub indent_tabs: Option<bool>,
    /// Build command templates by command name, validated.
    pub build: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Default)]
struct IndentFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    use_tabs: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    version: u32,
    name: String,
    #[serde(default = "dot")]
    base_path: String,
    #[serde(default)]
    file_patterns: Vec<String>,
    #[serde(default)]
    indentation: IndentFile,
    #[serde(default)]
    build: BTreeMap<String, String>,
}

fn dot() -> String {
    ".".to_owned()
}

/// Whether `template` is a usable command (no shell syntax, known
/// placeholders).
pub fn valid_command(template: &str) -> bool {
    split(template)
        .and_then(|w| expand(&w, &Placeholders::for_file(Path::new("/x/y.c"), 1)))
        .is_ok()
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}

fn valid_pattern(pattern: &str) -> bool {
    !pattern.is_empty()
        && pattern.len() <= 128
        && !pattern.contains(['/', '\\'])
        && !pattern.chars().any(char::is_control)
}

/// Opens the project in `folder` (the folder holding `.ferny`). Invalid
/// entries are dropped and reported; the file as a whole must be valid.
pub fn open(folder: &Path) -> Result<(Project, Vec<String>), ProjectError> {
    let root = std::fs::canonicalize(folder).map_err(|_| ProjectError::NotFound)?;
    let file =
        fsio::read_limited(&root.join(PROJECT_FILE), MAX_PROJECT_BYTES).map_err(
            |err| match err {
                fsio::ReadError::NotFound => ProjectError::NotFound,
                _ => ProjectError::Invalid,
            },
        )?;
    let text = std::str::from_utf8(&file.bytes).map_err(|_| ProjectError::Invalid)?;
    from_text(&root, text)
}

/// Validates the text of a project file whose root is `root` (canonical).
pub fn from_text(root: &Path, text: &str) -> Result<(Project, Vec<String>), ProjectError> {
    let root = root.to_path_buf();
    let parsed: ProjectFile = toml::from_str(text).map_err(|_| ProjectError::Invalid)?;
    if parsed.version != VERSION || !valid_name(&parsed.name) {
        return Err(ProjectError::Invalid);
    }
    let mut warnings = Vec::new();
    // The base folder must be inside the project, links resolved.
    let base = Some(Path::new(&parsed.base_path))
        .filter(|p| p.is_relative())
        .and_then(|p| std::fs::canonicalize(root.join(p)).ok())
        .filter(|p| p.starts_with(&root) && p.is_dir())
        .unwrap_or_else(|| {
            warnings.push(
                "base_path: outside the project or missing; the project folder is used".to_owned(),
            );
            root.clone()
        });
    let mut patterns = Vec::new();
    for pattern in parsed.file_patterns {
        if valid_pattern(&pattern) && patterns.len() < MAX_PATTERNS {
            patterns.push(pattern);
        } else {
            warnings.push("file_patterns: an invalid pattern was ignored".to_owned());
        }
    }
    let indent_width = match parsed.indentation.width {
        Some(w) if (1..=16).contains(&w) => u32::try_from(w).ok(),
        Some(_) => {
            warnings.push("indentation.width: out of range, ignored".to_owned());
            None
        }
        None => None,
    };
    let mut build = BTreeMap::new();
    for (name, template) in parsed.build {
        if PROJECT_COMMANDS.contains(&name.as_str()) && valid_command(&template) {
            build.insert(name, template);
        } else {
            warnings.push(format!("build.{name}: not valid, ignored"));
        }
    }
    let project = Project {
        root,
        name: parsed.name,
        base,
        patterns,
        indent_width,
        indent_tabs: parsed.indentation.use_tabs,
        build,
    };
    Ok((project, warnings))
}

fn write_new(root: &Path, file: &ProjectFile) -> Result<(), ProjectError> {
    let path = root.join(PROJECT_FILE);
    if path.exists() || path.symlink_metadata().is_ok() {
        return Err(ProjectError::Exists);
    }
    let text = toml::to_string(file).map_err(|_| ProjectError::Io)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| ProjectError::Io)?;
    }
    fsio::write_atomic_with_mode(&path, text.as_bytes(), None, FileMode::Preserve)
        .map_err(|_| ProjectError::Io)?;
    Ok(())
}

/// Creates `.ferny/project.toml` in `folder` (never overwrites).
pub fn create(folder: &Path, name: &str) -> Result<(), ProjectError> {
    if !valid_name(name) {
        return Err(ProjectError::Invalid);
    }
    let root = std::fs::canonicalize(folder).map_err(|_| ProjectError::NotFound)?;
    if root.parent().is_none() || !root.is_dir() {
        return Err(ProjectError::Invalid);
    }
    write_new(
        &root,
        &ProjectFile {
            version: VERSION,
            name: name.to_owned(),
            base_path: dot(),
            file_patterns: Vec::new(),
            indentation: IndentFile::default(),
            build: BTreeMap::new(),
        },
    )
}

/// A previewed `.geany` import.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GeanyProjectPlan {
    /// Folder where `.ferny/project.toml` would be written (the project's
    /// base folder).
    pub target: PathBuf,
    /// Imported items (name, patterns, indentation, each command).
    pub imported: Vec<String>,
    /// Everything else, listed.
    pub not_imported: Vec<String>,
    file: Option<ProjectFileData>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectFileData {
    name: String,
    patterns: Vec<String>,
    width: Option<i64>,
    tabs: Option<bool>,
    build: BTreeMap<String, String>,
}

/// Reads a Geany project file and plans its import. Nothing is written.
pub fn plan_geany_project(geany_file: &Path) -> Result<GeanyProjectPlan, ProjectError> {
    let text = read(geany_file).ok_or(ProjectError::Invalid)?;
    let geany_dir = geany_file.parent().ok_or(ProjectError::Invalid)?;
    plan_geany_project_text(&text, geany_dir)
}

/// [`plan_geany_project`] for the text of a `.geany` file in `geany_dir`.
pub fn plan_geany_project_text(
    text: &str,
    geany_dir: &Path,
) -> Result<GeanyProjectPlan, ProjectError> {
    let keys = KeyFile::parse(text);
    let name = keys
        .get("project", "name")
        .filter(|n| valid_name(n))
        .ok_or(ProjectError::Invalid)?;
    // Geany's base_path is absolute or relative to the .geany file.
    let base = keys.get("project", "base_path").unwrap_or("./");
    let target = std::fs::canonicalize(geany_dir.join(base)).map_err(|_| ProjectError::NotFound)?;
    if !target.is_dir() || target.parent().is_none() {
        return Err(ProjectError::Invalid);
    }
    let mut plan = GeanyProjectPlan {
        target,
        ..GeanyProjectPlan::default()
    };
    plan.imported.push(format!("name: {name}"));
    let patterns: Vec<String> = keys
        .get("project", "file_patterns")
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .filter(|p| valid_pattern(p))
        .take(MAX_PATTERNS)
        .map(str::to_owned)
        .collect();
    if !patterns.is_empty() {
        plan.imported
            .push(format!("file patterns: {}", patterns.join(" ")));
    }
    let width = keys
        .get("indentation", "indent_width")
        .and_then(|w| w.parse::<i64>().ok())
        .filter(|w| (1..=16).contains(w));
    // Geany: 0 spaces, 1 tabs, 2 tabs and spaces.
    let tabs = keys
        .get("indentation", "indent_type")
        .and_then(|t| match t {
            "0" => Some(false),
            "1" | "2" => Some(true),
            _ => None,
        });
    if width.is_some() || tabs.is_some() {
        plan.imported.push("indentation".to_owned());
    }
    let mut build = BTreeMap::new();
    for (key, command) in [
        ("FT_00_CM", "compile"),
        ("FT_01_CM", "build"),
        ("FT_02_CM", "lint"),
        ("NF_00_CM", "make"),
        ("EX_00_CM", "execute"),
    ] {
        let Some(template) = keys.get("build-menu", key).filter(|t| !t.trim().is_empty()) else {
            continue;
        };
        if valid_command(template) {
            build.insert(command.to_owned(), template.to_owned());
            plan.imported
                .push(format!("build command {command} (runs only once trusted)"));
        } else {
            plan.not_imported
                .push(format!("build-menu/{key}: shell syntax or invalid"));
        }
    }
    // Everything else is listed, never silently dropped.
    let used = [
        ("project", "name"),
        ("project", "base_path"),
        ("project", "file_patterns"),
        ("indentation", "indent_width"),
        ("indentation", "indent_type"),
        ("build-menu", "FT_00_CM"),
        ("build-menu", "FT_01_CM"),
        ("build-menu", "FT_02_CM"),
        ("build-menu", "NF_00_CM"),
        ("build-menu", "EX_00_CM"),
    ];
    for (group, _, entries) in &keys.groups {
        for (key, _) in entries {
            if !used.contains(&(group.as_str(), key.as_str())) {
                plan.not_imported.push(format!("{group}/{key}"));
            }
        }
    }
    plan.file = Some(ProjectFileData {
        name: name.to_owned(),
        patterns,
        width,
        tabs,
        build,
    });
    Ok(plan)
}

/// Writes the previewed import (never overwrites an existing project).
pub fn apply_geany_project(plan: &GeanyProjectPlan) -> Result<(), ProjectError> {
    let data = plan.file.as_ref().ok_or(ProjectError::Invalid)?;
    write_new(
        &plan.target,
        &ProjectFile {
            version: VERSION,
            name: data.name.clone(),
            base_path: dot(),
            file_patterns: data.patterns.clone(),
            indentation: IndentFile {
                width: data.width,
                use_tabs: data.tabs,
            },
            build: data.build.clone(),
        },
    )
}

/// Stable id of a project root for its session file in the user's data
/// (FNV-1a 64 of the canonical path).
pub fn session_id(root: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in root.to_string_lossy().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
