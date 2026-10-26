//! Settings layers and keybindings (untrusted files, validated per key).

use std::fs;
use std::path::PathBuf;

use ferny_config::keybindings::{Keybindings, is_plugin_command};
use ferny_config::settings::{DESCRIPTORS, MAX_SETTINGS_BYTES, Settings, Value, parse_value};
use ferny_core::command::builtin_registry;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-config-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        Self(dir)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, text).expect("write");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn defaults_are_valid() {
    let settings = Settings::default();
    for d in DESCRIPTORS {
        assert!(parse_value(d.kind, d.default).is_some(), "{}", d.key);
        assert!(settings.get(d.key).is_some(), "{}", d.key);
    }
    assert_eq!(settings.int("editor.indent_width"), 4);
    assert!(settings.bool("editor.auto_close"));
    assert_eq!(settings.text("files.default_encoding"), "UTF-8");
}

#[test]
fn layers_user_over_system_over_defaults() {
    let dir = TempDir::new("layers");
    let system = dir.write(
        "system.toml",
        "[editor]\nindent_width = 2\nline_numbers = false\n",
    );
    let user = dir.write("user.toml", "[editor]\nindent_width = 8\n");
    let settings = Settings::load(Some(&system), Some(&user));
    assert_eq!(settings.int("editor.indent_width"), 8);
    assert!(!settings.bool("editor.line_numbers"));
    assert!(settings.warnings().is_empty());
}

#[test]
fn invalid_values_are_ignored() {
    let dir = TempDir::new("invalid");
    let user = dir.write(
        "user.toml",
        "[editor]\nindent_width = 99\nfont_size = \"big\"\ncolor_scheme = \"../../etc\"\n\
         auto_close = false\n[files]\ndefault_eol = \"lfcr\"\ndefault_encoding = \"klingon\"\n",
    );
    let settings = Settings::load(None, Some(&user));
    assert_eq!(settings.int("editor.indent_width"), 4);
    assert_eq!(settings.int("editor.font_size"), 12);
    assert_eq!(settings.text("editor.color_scheme"), "default");
    assert!(
        !settings.bool("editor.auto_close"),
        "valid keys still apply"
    );
    assert_eq!(settings.text("files.default_eol"), "native");
    assert_eq!(settings.warnings().len(), 5);
}

#[test]
fn broken_file_uses_defaults() {
    let dir = TempDir::new("broken");
    let user = dir.write("user.toml", "this is = = not toml");
    let settings = Settings::load(None, Some(&user));
    assert_eq!(settings.int("editor.indent_width"), 4);
    assert_eq!(settings.warnings().len(), 1);
    let huge = dir.write(
        "huge.toml",
        &"#".repeat(usize::try_from(MAX_SETTINGS_BYTES).expect("fits") + 1),
    );
    assert_eq!(Settings::load(None, Some(&huge)).warnings().len(), 1);
    // A missing file is not a problem.
    assert!(
        Settings::load(None, Some(&dir.0.join("missing.toml")))
            .warnings()
            .is_empty()
    );
}

#[test]
fn set_validates() {
    let mut settings = Settings::default();
    assert!(settings.set("editor.indent_width", "2"));
    assert!(!settings.set("editor.indent_width", "0"));
    assert!(!settings.set("editor.no_such_key", "1"));
    assert!(settings.set("files.default_encoding", "latin1"));
    assert_eq!(
        settings.get("files.default_encoding"),
        Some(&Value::Text("windows-1252".to_owned()))
    );
}

#[test]
fn save_keeps_unknown_keys_and_writes_only_changes() {
    let dir = TempDir::new("save");
    let system = dir.write("system.toml", "[editor]\nline_numbers = false\n");
    let user = dir.write(
        "user.toml",
        "[future]\nknob = 3\n[editor]\nfont_size = 14\n",
    );
    let mut settings = Settings::load(Some(&system), Some(&user));
    assert!(settings.set("editor.indent_width", "2"));
    assert!(settings.set("editor.font_size", "12")); // back to the default
    settings.save(&user).expect("save");
    let text = fs::read_to_string(&user).expect("read");
    let table: toml::Table = text.parse().expect("toml");
    assert_eq!(table["future"]["knob"].as_integer(), Some(3));
    assert_eq!(table["editor"]["indent_width"].as_integer(), Some(2));
    assert!(table["editor"].get("font_size").is_none());
    assert!(
        table["editor"].get("line_numbers").is_none(),
        "system value not copied"
    );
    let reloaded = Settings::load(Some(&system), Some(&user));
    assert_eq!(reloaded.int("editor.indent_width"), 2);
}

#[test]
fn keybindings_override_and_validate() {
    let registry = builtin_registry();
    let dir = TempDir::new("keys");
    let path = dir.write(
        "keybindings.toml",
        "[keys]\n\"file.save\" = [\"Ctrl+Shift+S\"]\n\"search.goto_line\" = []\n\
         \"no.such\" = [\"Ctrl+X\"]\n\"file.open\" = [\"Ctrl+O; rm\"]\n",
    );
    let mut keys = Keybindings::load(&path, &registry);
    assert_eq!(keys.keys(&registry, "file.save"), ["Ctrl+Shift+S"]);
    assert!(keys.keys(&registry, "search.goto_line").is_empty());
    assert_eq!(
        keys.keys(&registry, "file.open"),
        ["Ctrl+O"],
        "invalid entry dropped"
    );
    assert_eq!(keys.warnings().len(), 2);
    // Ctrl+Shift+S now collides with Save As.
    assert_eq!(
        keys.conflicts(&registry, "file.save", "Ctrl+Shift+S"),
        ["file.save_as"]
    );
    assert!(keys.set(&registry, "file.save", None));
    assert_eq!(keys.keys(&registry, "file.save"), ["Ctrl+S"]);
    assert!(!keys.set(&registry, "file.save", Some(vec!["Ctrl+".to_owned()])));
    assert!(!keys.set(&registry, "no.such", Some(vec![])));
    assert!(keys.set(&registry, "file.close", Some(vec!["Ctrl+F4".to_owned()])));
    keys.save(&path).expect("save");
    let reloaded = Keybindings::load(&path, &registry);
    assert_eq!(reloaded.keys(&registry, "file.close"), ["Ctrl+F4"]);
    assert!(reloaded.keys(&registry, "search.goto_line").is_empty());
}

#[test]
fn macos_reserved_shortcuts_have_no_default() {
    let registry = builtin_registry();
    let replace = registry.get("search.replace").expect("command");
    let marker = registry.get("edit.toggle_marker").expect("command");
    if cfg!(target_os = "macos") {
        assert_eq!(replace.default_keys(), ["Ctrl+H"]);
        assert!(marker.default_keys().is_empty());
    } else {
        assert_eq!(replace.default_keys(), ["Ctrl+H"]);
        assert_eq!(marker.default_keys(), ["Ctrl+M"]);
    }
    // H2.3: Option+arrows and Cmd+Space belong to macOS.
    let back = registry.get("view.navigate_back").expect("command");
    let complete = registry.get("edit.complete_word").expect("command");
    if cfg!(target_os = "macos") {
        assert_eq!(back.default_keys(), ["Ctrl+Meta+Left"]);
        assert_eq!(complete.default_keys(), ["Alt+Esc"]);
    } else {
        assert_eq!(back.default_keys(), ["Alt+Left"]);
        assert_eq!(complete.default_keys(), ["Ctrl+Space"]);
    }
}

#[test]
fn plugin_keys_never_take_built_in_keys() {
    let registry = builtin_registry();
    let dir = TempDir::new("plugin-keys");
    let path = dir.write(
        "keybindings.toml",
        "[keys]\n\"plugin.hello.stamp\" = [\"Ctrl+Alt+H\", \"Ctrl+S\"]\n\"plugin.Bad.x\" = [\"Ctrl+Alt+J\"]\n",
    );
    let keys = Keybindings::load(&path, &registry);
    assert_eq!(
        keys.plugin_keys(&registry, "plugin.hello.stamp"),
        ["Ctrl+Alt+H"]
    );
    assert!(keys.plugin_keys(&registry, "plugin.Bad.x").is_empty());
    assert_eq!(keys.warnings().len(), 1);
    assert!(keys.plugin_keys(&registry, "file.save").is_empty());
    for (id, ok) in [
        ("plugin.hello.stamp", true),
        ("plugin.my-tool.run_2", true),
        ("plugin.hello", false),
        ("plugin.hello.stamp.x", false),
        ("plugin.hello.Stamp", false),
        ("file.save", false),
    ] {
        assert_eq!(is_plugin_command(id), ok, "{id}");
    }
}
