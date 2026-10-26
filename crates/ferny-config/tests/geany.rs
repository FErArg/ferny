//! Geany configuration importer (threat T11).

use std::fs;
use std::path::PathBuf;

use ferny_config::geany::{self, gtk_accelerator};
use ferny_config::keybindings::Keybindings;
use ferny_config::settings::Settings;
use ferny_core::command::builtin_registry;
use ferny_syntax::Catalog;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures/geany-config")
}

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-geany-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn settings_are_mapped() {
    let plan = geany::plan(&fixture(), &builtin_registry());
    let get = |key: &str| {
        plan.settings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("editor.font_family"), Some("DejaVu Sans Mono"));
    assert_eq!(get("editor.font_size"), Some("13"));
    assert_eq!(get("editor.indent_width"), Some("2"));
    assert_eq!(get("editor.indent_use_tabs"), Some("false"));
    assert_eq!(get("editor.line_numbers"), Some("false"));
    assert_eq!(get("editor.long_line_marker"), Some("100"));
    assert_eq!(get("editor.auto_close"), Some("true"));
    assert_eq!(get("editor.auto_close_quotes"), Some("true"));
    assert_eq!(get("editor.color_scheme"), Some("my_theme"));
    assert_eq!(get("files.default_eol"), Some("lf"));
    assert_eq!(get("files.restore_session"), Some("false"));
    assert_eq!(get("files.reload_on_external_change"), Some("always"));
}

#[test]
fn report_lists_unmapped() {
    let plan = geany::plan(&fixture(), &builtin_registry());
    for expected in [
        "geany.conf: [geany] sidebar_pos",
        "keybindings.conf: edit_transposeline",
        "keybindings.conf: menu_find (shortcut)",
        "filedefs/filetypes.Mystery.conf (no known lexer)",
    ] {
        assert!(
            plan.not_imported.iter().any(|n| n == expected),
            "{expected}"
        );
    }
}

#[test]
fn does_not_import_commands() {
    let plan = geany::plan(&fixture(), &builtin_registry());
    let everything = format!("{:?}", plan.files) + &format!("{:?}", plan.settings);
    for forbidden in ["rm -rf", "gcc -o evil", "xdg-open", "example.invalid"] {
        assert!(!everything.contains(forbidden), "{forbidden}");
    }
    for expected in [
        "geany.conf: [build-menu] EX_00_CM",
        "filedefs/filetypes.c: [build-menu] FT_00_CM",
        "filedefs/filetypes.c: [settings] context_action_cmd",
    ] {
        assert!(
            plan.not_imported.iter().any(|n| n == expected),
            "{expected}"
        );
    }
}

#[test]
fn keybindings_are_translated() {
    let plan = geany::plan(&fixture(), &builtin_registry());
    let get = |id: &str| {
        plan.keybindings
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(get("file.save"), Some(vec!["Ctrl+Shift+S".to_owned()]));
    assert_eq!(get("search.find_next"), Some(vec!["F3".to_owned()]));
    assert_eq!(get("edit.next_marker"), Some(vec!["Ctrl+.".to_owned()]));
    assert_eq!(get("view.next_tab"), Some(vec!["Ctrl+PgDown".to_owned()]));
    assert_eq!(get("search.replace"), Some(vec![]), "empty = no shortcut");
    assert_eq!(
        gtk_accelerator("<Control><Alt>Delete").as_deref(),
        Some("Ctrl+Alt+Del")
    );
    assert_eq!(gtk_accelerator("<Shift>"), None);
    assert_eq!(gtk_accelerator("<Primary>s; rm"), None);
}

#[test]
fn apply_writes_files_inside_the_config_dir_and_the_catalog_uses_them() {
    let registry = builtin_registry();
    let plan = geany::plan(&fixture(), &registry);
    let out = temp("apply");
    let mut settings = Settings::default();
    let mut keys = Keybindings::default();
    let rejected = geany::apply(&plan, &out, &registry, &mut settings, &mut keys);
    assert!(rejected.is_empty(), "{rejected:?}");
    assert_eq!(settings.text("files.default_encoding"), "ISO-8859-15");
    assert_eq!(keys.keys(&registry, "file.save"), ["Ctrl+Shift+S"]);
    assert!(out.join("filetypes/c.toml").is_file());
    assert!(out.join("filetypes/foo.toml").is_file());
    assert!(out.join("colorschemes/my_theme.toml").is_file());

    let (catalog, warnings) = Catalog::with_user_dir(&out);
    assert!(warnings.is_empty(), "{warnings:?}");
    let foo = catalog.detect("x.foo", "").expect("custom filetype");
    assert_eq!((foo.id.as_str(), foo.lexer.as_str()), ("foo", "cpp"));
    assert!(foo.keywords[0].words.contains("baz"));
    let c = catalog.filetype("c").expect("c");
    assert!(c.keywords[0].words.contains("custom_keyword"));
    assert!(c.extensions.iter().any(|e| e == "*.inc"));
    let named = catalog.named_styles("my_theme");
    assert_eq!(named.get("default").map(|s| s.back), Some(0x101010));
}

#[test]
fn names_cannot_escape() {
    let registry = builtin_registry();
    let mut plan = geany::plan(&fixture(), &registry);
    plan.files
        .push((PathBuf::from("../escape.toml"), "x".to_owned()));
    plan.files
        .push((PathBuf::from("filetypes/../../escape.toml"), "x".to_owned()));
    let out = temp("escape");
    let rejected = geany::apply(
        &plan,
        &out.join("config"),
        &registry,
        &mut Settings::default(),
        &mut Keybindings::default(),
    );
    assert_eq!(
        rejected.iter().filter(|r| *r == "an output path").count(),
        2
    );
    assert!(!out.join("escape.toml").exists());
}

#[cfg(unix)]
#[test]
fn limits_and_special_files() {
    use std::os::unix::fs::symlink;
    let dir = temp("limits");
    fs::create_dir_all(dir.join("colorschemes")).expect("mkdir");
    let outside = temp("limits-outside");
    fs::write(
        outside.join("secret.conf"),
        "[named_styles]\ndefault=#000;#fff\n",
    )
    .expect("write");
    symlink(
        outside.join("secret.conf"),
        dir.join("colorschemes/linked.conf"),
    )
    .expect("symlink");
    let huge = format!("[theme_info]\nname=x\n{}", "#".repeat(1024 * 1024 + 10));
    fs::write(dir.join("colorschemes/huge.conf"), huge).expect("write");
    let plan = geany::plan(&dir, &builtin_registry());
    assert!(
        plan.files.is_empty(),
        "symlinked and oversized files are not imported"
    );
    assert!(plan.not_imported.iter().any(|n| n.contains("huge.conf")));
}

#[test]
fn missing_directory_imports_nothing() {
    let plan = geany::plan(&temp("empty").join("nope"), &builtin_registry());
    assert_eq!(plan, geany::ImportPlan::default());
}

#[test]
fn imports_snippets_templates_and_developer() {
    let geany_dir = temp("templates-src");
    fs::create_dir_all(geany_dir.join("templates/files")).expect("mkdir");
    fs::write(geany_dir.join("snippets.conf"), "[C]\nhi=hello\n").expect("write");
    fs::write(geany_dir.join("templates/files/mine.c"), "{fileheader}\n").expect("write");
    fs::write(geany_dir.join("templates/files/bad name;x"), "x").expect("write");
    fs::write(
        geany_dir.join("geany.conf"),
        "[geany]\npref_template_developer=Ada\npref_template_mail=ada@example.invalid\n",
    )
    .expect("write");
    let plan = geany::plan(&geany_dir, &builtin_registry());
    assert!(
        plan.settings
            .contains(&("templates.developer".into(), "Ada".into()))
    );
    assert!(
        plan.not_imported.iter().any(|n| n.contains("bad name")),
        "{plan:?}"
    );
    let out = temp("templates-out");
    let mut settings = Settings::default();
    let mut keys = Keybindings::default();
    let rejected = geany::apply(&plan, &out, &builtin_registry(), &mut settings, &mut keys);
    assert!(rejected.is_empty(), "{rejected:?}");
    assert_eq!(
        fs::read_to_string(out.join("snippets.conf")).expect("read"),
        "[C]\nhi=hello\n"
    );
    assert!(out.join("templates/files/mine.c").is_file());
    assert_eq!(settings.text("templates.developer"), "Ada");
}

#[test]
fn editing_keys_are_mapped() {
    let mut plan = geany::ImportPlan::default();
    geany::settings_from_text(
        "[geany]\npref_editor_smart_home_key=false\npref_editor_newline_strip=true\n\
         use_tab_to_indent=false\nbackspace_unindent=true\nindent_mode=0\n\
         indent_hard_tab_width=4\n",
        &mut plan,
    );
    let get = |key: &str| {
        plan.settings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("editor.smart_home"), Some("false"));
    assert_eq!(get("editor.newline_strips_trailing"), Some("true"));
    assert_eq!(get("editor.tab_indents"), Some("false"));
    assert_eq!(get("editor.backspace_unindents"), Some("true"));
    assert_eq!(get("editor.auto_indent"), Some("false"));
    assert_eq!(get("editor.tab_width"), Some("4"));
}
