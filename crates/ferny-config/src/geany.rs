//! Geany configuration importer (DESIGN §5.1, threat T11).
//!
//! A pure parser: it reads a Geany configuration directory (untrusted),
//! builds an [`ImportPlan`] that the user previews, and only then writes
//! Ferny files. Nothing is executed; build commands and every key outside
//! the allowlists are reported as not imported, never silently dropped.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ferny_core::command::{Registry, is_valid_key_sequence};
use ferny_core::fsio;
use ferny_syntax::Catalog;
use ferny_syntax::user::{NamedValue, NamedWords, USER_SETTINGS, UserFiletype, valid_id};

use crate::keybindings::Keybindings;
use crate::settings::Settings;

/// Largest Geany file read, in bytes.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Most files read per directory.
pub const MAX_FILES: usize = 500;

/// What an import would do, shown to the user before anything is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    /// Ferny settings to set (`key`, value as text).
    pub settings: Vec<(String, String)>,
    /// Keybindings to set (command id, key sequences).
    pub keybindings: Vec<(String, Vec<String>)>,
    /// Files to write under Ferny's configuration directory
    /// (relative path, contents).
    pub files: Vec<(PathBuf, String)>,
    /// Everything that was not imported, by name (no values).
    pub not_imported: Vec<String>,
}

// --- GKeyFile subset ------------------------------------------------------------

/// One group: name, optional base (`[styling=C]`), entries in order.
pub(crate) type Group = (String, Option<String>, Vec<(String, String)>);

/// Groups of a key file in order.
#[derive(Debug, Default)]
pub(crate) struct KeyFile {
    pub(crate) groups: Vec<Group>,
}

impl KeyFile {
    pub(crate) fn parse(text: &str) -> Self {
        let mut file = KeyFile::default();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let (name, base) = match header.split_once('=') {
                    Some((n, b)) => (n.trim().to_owned(), Some(b.trim().to_owned())),
                    None => (header.trim().to_owned(), None),
                };
                file.groups.push((name, base, Vec::new()));
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if key.contains('[') {
                continue; // localised keys
            }
            if let Some((_, _, entries)) = file.groups.last_mut() {
                entries.push((key.to_owned(), value.trim().to_owned()));
            }
        }
        file
    }

    pub(crate) fn group(&self, name: &str) -> Option<&Group> {
        self.groups.iter().find(|(n, _, _)| n == name)
    }

    pub(crate) fn get(&self, group: &str, key: &str) -> Option<&str> {
        self.group(group)?
            .2
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

pub(crate) fn read(path: &Path) -> Option<String> {
    let file = fsio::read_limited(path, MAX_FILE_BYTES).ok()?;
    String::from_utf8(file.bytes).ok()
}

/// Regular files of a directory (symlinks not followed), sorted, bounded.
fn regular_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    files.sort();
    files.truncate(MAX_FILES);
    files
}

fn to_id(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

// --- geany.conf -------------------------------------------------------------------

/// `[geany]` keys that map to Ferny settings.
const GEANY_KEYS: &[&str] = &[
    "editor_font",
    "pref_editor_tab_width",
    "indent_type",
    "show_linenumber_margin",
    "long_line_enabled",
    "long_line_column",
    "autoclose_chars",
    "pref_editor_smart_home_key",
    "pref_editor_newline_strip",
    "use_tab_to_indent",
    "backspace_unindent",
    "indent_mode",
    "indent_hard_tab_width",
    "color_scheme",
    "pref_editor_default_new_encoding",
    "default_eol_character",
    "pref_editor_trail_space",
    "pref_editor_new_line",
    "pref_main_load_session",
    "reload_clean_doc_on_file_change",
    "pref_template_developer",
    "pref_template_initial",
    "pref_template_mail",
    "pref_template_company",
    "pref_template_version",
];

fn import_settings(dir: &Path, plan: &mut ImportPlan) {
    if let Some(text) = read(&dir.join("geany.conf")) {
        settings_from_text(&text, plan);
    }
}

/// Maps the text of a `geany.conf` (public for fuzzing).
pub fn settings_from_text(text: &str, plan: &mut ImportPlan) {
    let conf = KeyFile::parse(text);
    let get = |key: &str| conf.get("geany", key);
    let mut set = |key: &str, value: String| plan.settings.push((key.to_owned(), value));

    if let Some(font) = get("editor_font") {
        // Pango description: "Family Name 10".
        match font.rsplit_once(' ') {
            Some((family, size)) if size.parse::<u32>().is_ok() => {
                set("editor.font_family", family.trim().to_owned());
                set("editor.font_size", size.to_owned());
            }
            _ => set("editor.font_family", font.to_owned()),
        }
    }
    if let Some(width) = get("pref_editor_tab_width") {
        set("editor.indent_width", width.to_owned());
    }
    if let Some(kind) = get("indent_type") {
        // 0 spaces, 1 tabs, 2 tabs and spaces.
        set("editor.indent_use_tabs", (kind != "0").to_string());
    }
    if let Some(numbers) = get("show_linenumber_margin") {
        set("editor.line_numbers", numbers.to_owned());
    }
    let enabled = get("long_line_enabled").is_none_or(|v| v == "true");
    if let Some(column) = get("long_line_column") {
        set(
            "editor.long_line_marker",
            if enabled {
                column.to_owned()
            } else {
                "0".to_owned()
            },
        );
    } else if !enabled {
        set("editor.long_line_marker", "0".to_owned());
    }
    if let Some(chars) = get("autoclose_chars").and_then(|v| v.parse::<u32>().ok()) {
        // 1 (), 2 {}, 4 [], 8 '', 16 "" — Ferny closes the three brackets.
        set("editor.auto_close", (chars & 7 != 0).to_string());
        set("editor.auto_close_quotes", (chars & 24 != 0).to_string());
    }
    // Geany 2.1 keyfile names, from src/keyfile.c. indent_mode: 0 is "none".
    if let Some(mode) = get("indent_mode") {
        set("editor.auto_indent", (mode != "0").to_string());
    }
    if let Some(width) = get("indent_hard_tab_width") {
        set("editor.tab_width", width.to_owned());
    }
    if let Some(scheme) = get("color_scheme").filter(|s| !s.is_empty()) {
        let stem = scheme.strip_suffix(".conf").unwrap_or(scheme);
        set("editor.color_scheme", to_id(stem));
    }
    if let Some(encoding) = get("pref_editor_default_new_encoding") {
        set("files.default_encoding", encoding.to_owned());
    }
    if let Some(eol) = get("default_eol_character") {
        let value = match eol {
            "0" => "crlf",
            "1" => "cr",
            _ => "lf",
        };
        set("files.default_eol", value.to_owned());
    }
    for (geany, ferny) in [
        ("pref_editor_trail_space", "files.trim_trailing_whitespace"),
        ("pref_editor_smart_home_key", "editor.smart_home"),
        (
            "pref_editor_newline_strip",
            "editor.newline_strips_trailing",
        ),
        ("use_tab_to_indent", "editor.tab_indents"),
        ("backspace_unindent", "editor.backspace_unindents"),
        ("pref_editor_new_line", "files.ensure_final_newline"),
        ("pref_main_load_session", "files.restore_session"),
        ("pref_template_developer", "templates.developer"),
        ("pref_template_initial", "templates.initials"),
        ("pref_template_mail", "templates.mail"),
        ("pref_template_company", "templates.company"),
        ("pref_template_version", "templates.version"),
    ] {
        if let Some(value) = get(geany) {
            set(ferny, value.to_owned());
        }
    }
    if let Some(reload) = get("reload_clean_doc_on_file_change") {
        set(
            "files.reload_on_external_change",
            if reload == "true" { "always" } else { "ask" }.to_owned(),
        );
    }
    for (group, _, entries) in &conf.groups {
        for (key, _) in entries {
            if !(group == "geany" && GEANY_KEYS.contains(&key.as_str())) {
                plan.not_imported
                    .push(format!("geany.conf: [{group}] {key}"));
            }
        }
    }
}

// --- keybindings.conf ---------------------------------------------------------

/// Geany keybinding names → Ferny commands.
const KEY_MAP: &[(&str, &str)] = &[
    ("menu_new", "file.new"),
    ("menu_open", "file.open"),
    ("menu_save", "file.save"),
    ("menu_saveas", "file.save_as"),
    ("menu_reloadfile", "file.reload"),
    ("menu_close", "file.close"),
    ("menu_closeall", "file.close_all"),
    ("file_openlasttab", "file.reopen_closed"),
    ("menu_quit", "app.quit"),
    ("menu_undo", "edit.undo"),
    ("menu_redo", "edit.redo"),
    ("menu_cut", "edit.cut"),
    ("menu_copy", "edit.copy"),
    ("menu_paste", "edit.paste"),
    ("menu_selectall", "edit.select_all"),
    ("edit_duplicateline", "edit.duplicate_line"),
    ("edit_deleteline", "edit.delete_line"),
    ("edit_movelineup", "edit.move_line_up"),
    ("edit_movelinedown", "edit.move_line_down"),
    ("edit_commentlinetoggle", "edit.toggle_comment"),
    ("edit_togglemarker", "edit.toggle_marker"),
    ("edit_gotonextmarker", "edit.next_marker"),
    ("edit_gotopreviousmarker", "edit.previous_marker"),
    ("menu_preferences", "app.preferences"),
    ("menu_find", "search.find"),
    ("menu_findnext", "search.find_next"),
    ("menu_findprevious", "search.find_previous"),
    ("menu_replace", "search.replace"),
    ("menu_findinfiles", "search.find_in_files"),
    ("menu_gotoline", "search.goto_line"),
    ("switch_tabright", "view.next_tab"),
    ("switch_tableft", "view.previous_tab"),
];

/// GTK accelerator (`<Primary><Shift>s`) → Qt portable text (`Ctrl+Shift+S`).
pub fn gtk_accelerator(accel: &str) -> Option<String> {
    let mut rest = accel.trim();
    let mut parts: Vec<&str> = Vec::new();
    while let Some(after) = rest.strip_prefix('<') {
        let (modifier, tail) = after.split_once('>')?;
        let qt = match modifier.to_ascii_lowercase().as_str() {
            "primary" | "control" | "ctrl" => "Ctrl",
            "shift" => "Shift",
            "alt" | "mod1" => "Alt",
            "super" | "meta" => "Meta",
            _ => return None,
        };
        if !parts.contains(&qt) {
            parts.push(qt);
        }
        rest = tail;
    }
    let key = match rest {
        "" => return None,
        "Page_Up" => "PgUp".to_owned(),
        "Page_Down" => "PgDown".to_owned(),
        "Return" | "KP_Enter" => "Return".to_owned(),
        "Delete" => "Del".to_owned(),
        "Insert" => "Ins".to_owned(),
        "BackSpace" => "Backspace".to_owned(),
        "Escape" => "Esc".to_owned(),
        "space" => "Space".to_owned(),
        "comma" => ",".to_owned(),
        "period" => ".".to_owned(),
        "slash" => "/".to_owned(),
        "backslash" => "\\".to_owned(),
        "minus" => "-".to_owned(),
        "plus" => "+".to_owned(),
        "equal" => "=".to_owned(),
        "semicolon" => ";".to_owned(),
        "apostrophe" => "'".to_owned(),
        "grave" => "`".to_owned(),
        "bracketleft" => "[".to_owned(),
        "bracketright" => "]".to_owned(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_owned(),
    };
    let mut sequence = parts.join("+");
    if !sequence.is_empty() {
        sequence.push('+');
    }
    sequence.push_str(&key);
    is_valid_key_sequence(&sequence).then_some(sequence)
}

fn import_keybindings(dir: &Path, registry: &Registry, plan: &mut ImportPlan) {
    if let Some(text) = read(&dir.join("keybindings.conf")) {
        keybindings_from_text(&text, registry, plan);
    }
}

/// Maps the text of a `keybindings.conf` (public for fuzzing).
pub fn keybindings_from_text(text: &str, registry: &Registry, plan: &mut ImportPlan) {
    let conf = KeyFile::parse(text);
    let Some((_, _, entries)) = conf.group("Bindings") else {
        return;
    };
    for (name, accel) in entries {
        let Some((_, id)) = KEY_MAP.iter().find(|(g, _)| g == name) else {
            plan.not_imported.push(format!("keybindings.conf: {name}"));
            continue;
        };
        if registry.get(id).is_none() {
            continue;
        }
        if accel.is_empty() {
            plan.keybindings.push(((*id).to_owned(), Vec::new()));
            continue;
        }
        match gtk_accelerator(accel) {
            Some(keys) => plan.keybindings.push(((*id).to_owned(), vec![keys])),
            None => plan
                .not_imported
                .push(format!("keybindings.conf: {name} (shortcut)")),
        }
    }
}

// --- filedefs and colour schemes ------------------------------------------------

/// Settings groups never imported from filetype files (commands).
const FILEDEF_SKIPPED: &[&str] = &["build-menu", "build_settings"];

fn import_filetypes(dir: &Path, plan: &mut ImportPlan) {
    let catalog = Catalog::builtin();
    let extensions = read(&dir.join("filetype_extensions.conf"))
        .map(|text| KeyFile::parse(&text))
        .unwrap_or_default();
    let patterns = |name: &str| {
        extensions.get("Extensions", name).map(|v| {
            v.split(';')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
    };
    for path in regular_files(&dir.join("filedefs")) {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = file_name.strip_prefix("filetypes.") else {
            continue;
        };
        let (name, custom) = match stem.strip_suffix(".conf") {
            Some(name) => (name, true),
            None => (stem, false),
        };
        let Some(text) = read(&path) else {
            plan.not_imported
                .push(format!("filedefs/{file_name} (unreadable or too large)"));
            continue;
        };
        let conf = KeyFile::parse(&text);
        let id = to_id(name);
        let builtin = catalog.filetype(&id).is_some();
        let base = if builtin {
            None
        } else {
            // Custom filetype: its lexer comes from `lexer_filetype`.
            conf.get("settings", "lexer_filetype").and_then(|lexer| {
                catalog
                    .filetypes()
                    .iter()
                    .find(|ft| ft.name == lexer)
                    .map(|ft| ft.id.clone())
            })
        };
        if !builtin && base.is_none() {
            plan.not_imported
                .push(format!("filedefs/{file_name} (no known lexer)"));
            continue;
        }
        // filetype_extensions.conf uses display names (`C`, `Foo`).
        let display = catalog.filetype(&id).map_or(name, |ft| ft.name.as_str());
        let mut user = UserFiletype {
            id: id.clone(),
            base,
            name: (!builtin && custom).then(|| name.to_owned()),
            extensions: patterns(display),
            ..UserFiletype::default()
        };
        for (group, _, entries) in &conf.groups {
            for (key, value) in entries {
                match group.as_str() {
                    "styling" => user.styles.push(NamedValue {
                        key: key.clone(),
                        value: value.clone(),
                    }),
                    "keywords" => user.keywords.push(NamedWords {
                        key: key.clone(),
                        words: value.clone(),
                    }),
                    "settings" if USER_SETTINGS.contains(&key.as_str()) => {
                        user.settings.insert(key.clone(), value.clone());
                    }
                    "settings" if key == "lexer_filetype" => {}
                    g if FILEDEF_SKIPPED.contains(&g)
                        || g == "settings"
                        || g == "indentation"
                        || g == "lexer_properties" =>
                    {
                        plan.not_imported
                            .push(format!("filedefs/{file_name}: [{g}] {key}"));
                    }
                    g => plan
                        .not_imported
                        .push(format!("filedefs/{file_name}: [{g}] {key}")),
                }
            }
        }
        if !valid_id(&user.id) || !user.is_valid() {
            plan.not_imported
                .push(format!("filedefs/{file_name} (invalid values)"));
            continue;
        }
        if let Ok(text) = toml::to_string(&user) {
            plan.files
                .push((PathBuf::from(format!("filetypes/{}.toml", user.id)), text));
        }
    }
    // Extension changes for built-in filetypes without a filedefs file.
    if let Some((_, _, entries)) = extensions.group("Extensions") {
        for (name, _) in entries {
            let Some(ft) = catalog.filetypes().iter().find(|ft| &ft.name == name) else {
                continue; // custom filetypes were handled with their filedefs
            };
            let path = PathBuf::from(format!("filetypes/{}.toml", ft.id));
            if plan.files.iter().any(|(p, _)| *p == path) {
                continue;
            }
            let user = UserFiletype {
                id: ft.id.clone(),
                extensions: patterns(name),
                ..UserFiletype::default()
            };
            if user.is_valid()
                && let Ok(text) = toml::to_string(&user)
            {
                plan.files.push((path, text));
            }
        }
    }
}

fn import_schemes(dir: &Path, plan: &mut ImportPlan) {
    for path in regular_files(&dir.join("colorschemes")) {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(".conf") else {
            continue;
        };
        let Some(text) = read(&path) else {
            plan.not_imported.push(format!(
                "colorschemes/{file_name} (unreadable or too large)"
            ));
            continue;
        };
        let conf = KeyFile::parse(&text);
        let id = to_id(stem);
        let entries = |group: &str| conf.group(group).map(|g| g.2.clone()).unwrap_or_default();
        let info = entries("theme_info");
        let field = |key: &str| info.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let scheme = ferny_syntax::ColorScheme {
            id: id.clone(),
            name: field("name").unwrap_or_else(|| stem.to_owned()),
            description: field("description").unwrap_or_default(),
            author: field("author").unwrap_or_default(),
            license: "imported from the user's Geany configuration".to_owned(),
            colors: entries("named_colors")
                .into_iter()
                .collect::<BTreeMap<_, _>>(),
            styles: entries("named_styles")
                .into_iter()
                .map(|(name, value)| ferny_syntax::NamedStyleEntry { name, value })
                .collect(),
        };
        if !valid_id(&id) {
            plan.not_imported
                .push(format!("colorschemes/{file_name} (invalid name)"));
            continue;
        }
        if let Ok(text) = toml::to_string(&scheme) {
            plan.files
                .push((PathBuf::from(format!("colorschemes/{id}.toml")), text));
        }
    }
}

/// Builds the import plan for the Geany configuration in `dir`.
pub fn plan(dir: &Path, registry: &Registry) -> ImportPlan {
    let mut plan = ImportPlan::default();
    import_settings(dir, &mut plan);
    import_keybindings(dir, registry, &mut plan);
    import_filetypes(dir, &mut plan);
    import_schemes(dir, &mut plan);
    import_templates(dir, &mut plan);
    plan
}

/// The user's `snippets.conf` and file templates (H2.5b): text only, copied
/// as they are (Geany's formats); `{command:…}` is never run by Ferny.
fn import_templates(dir: &Path, plan: &mut ImportPlan) {
    if let Some(text) = read(&dir.join("snippets.conf")) {
        plan.files.push((PathBuf::from("snippets.conf"), text));
    }
    for path in regular_files(&dir.join("templates/files")) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let safe = !name.starts_with('.')
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        match read(&path).filter(|_| safe) {
            Some(text) => plan
                .files
                .push((PathBuf::from("templates/files").join(name), text)),
            None => plan.not_imported.push(format!("templates/files/{name}")),
        }
    }
}

/// Applies a plan: settings and keybindings in memory (the caller saves
/// them), files written atomically under `config_dir`. Values that fail
/// validation are added to the returned list.
pub fn apply(
    plan: &ImportPlan,
    config_dir: &Path,
    registry: &Registry,
    settings: &mut Settings,
    keybindings: &mut Keybindings,
) -> Vec<String> {
    let mut rejected = Vec::new();
    for (key, value) in &plan.settings {
        if !settings.set(key, value) {
            rejected.push(format!("setting {key}"));
        }
    }
    for (id, keys) in &plan.keybindings {
        if !keybindings.set(registry, id, Some(keys.clone())) {
            rejected.push(format!("shortcut {id}"));
        }
    }
    for (relative, text) in &plan.files {
        // Paths come from validated names; still refuse anything else.
        let parts: Vec<_> = relative.components().collect();
        let normal = parts
            .iter()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        let folder = relative.parent().and_then(Path::to_str).unwrap_or("");
        let safe = normal
            && match parts.len() {
                1 => relative == Path::new("snippets.conf"),
                2 => matches!(folder, "filetypes" | "colorschemes"),
                3 => folder == "templates/files",
                _ => false,
            };
        if !safe {
            rejected.push("an output path".to_owned());
            continue;
        }
        let path = config_dir.join(relative);
        let written = path
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .ok()
            .and_then(|_| fsio::write_atomic(&path, text.as_bytes(), None).ok());
        if written.is_none() {
            rejected.push(format!("file {}", relative.display()));
        }
    }
    rejected
}
