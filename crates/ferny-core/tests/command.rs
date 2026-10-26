//! Tests for the command registry.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ferny_core::command::{
    Command, CommandError, CommandId, Dispatch, Handler, MAX_ID_LEN, MenuItem, Registry,
    builtin_registry,
};

fn ui(id: &str) -> Command {
    Command::new(id, "Title", &[], Handler::Ui).expect("valid command")
}

#[test]
fn accepts_well_formed_ids() {
    for id in ["file.open", "edit.duplicate_line", "plugin.git2.commit_all"] {
        assert_eq!(
            CommandId::parse(id).map(|c| c.to_string()),
            Ok(id.to_owned())
        );
    }
    assert!(CommandId::parse_menu("file").is_ok());
}

#[test]
fn rejects_malformed_ids() {
    let too_long = format!("a.{}", "b".repeat(MAX_ID_LEN));
    for id in [
        "",
        "file",
        "File.open",
        "file..open",
        ".file.open",
        "file.open.",
        "file.2open",
        "file.open-all",
        "file.öpen",
        "file.open ",
        "file.open\n",
        "../etc.passwd",
        too_long.as_str(),
    ] {
        assert_eq!(CommandId::parse(id), Err(CommandError::InvalidId), "{id:?}");
    }
    assert_eq!(CommandId::parse_menu("File"), Err(CommandError::InvalidId));
}

#[test]
fn rejects_bad_titles() {
    for title in ["", "   ", "Open\nfile", "Tab\there", &"x".repeat(129)] {
        assert_eq!(
            Command::new("file.open", title, &[], Handler::Ui).map(|_| ()),
            Err(CommandError::InvalidTitle),
            "{title:?}"
        );
    }
}

#[test]
fn validates_key_sequences() {
    for keys in [
        &["Ctrl+O"][..],
        &["F5", "Ctrl+K, Ctrl+C"][..],
        &["Ctrl+Shift+,"][..],
    ] {
        assert!(
            Command::new("file.open", "Open", keys, Handler::Ui).is_ok(),
            "{keys:?}"
        );
    }
    for keys in [
        &[""][..],
        &["+O"][..],
        &["Ctrl+"][..],
        &["Ctrl++O"][..],
        &["Ctrl+Hyper+O"][..],
        &["Ctrl+ "][..],
        &["Ctrl+O,Ctrl+P"][..],
        &["Ctrl+O; rm"][..],
        &["Ctrl+O\n"][..],
        &["A", "B", "C", "D", "E"][..],
    ] {
        assert_eq!(
            Command::new("file.open", "Open", keys, Handler::Ui).map(|_| ()),
            Err(CommandError::InvalidKeys),
            "{keys:?}"
        );
    }
}

#[test]
fn rejects_duplicate_commands_and_menus() {
    let mut registry = Registry::new();
    registry.register(ui("file.open")).expect("first");
    assert_eq!(
        registry.register(ui("file.open")),
        Err(CommandError::Duplicate)
    );
    registry
        .add_menu("file", "&File", &[Some("file.open")])
        .expect("menu");
    assert_eq!(
        registry.add_menu("file", "&File", &[]),
        Err(CommandError::Duplicate)
    );
}

#[test]
fn menus_reference_registered_commands_only() {
    let mut registry = Registry::new();
    registry.register(ui("file.open")).expect("register");
    assert_eq!(
        registry.add_menu("file", "&File", &[Some("file.open"), Some("file.missing")]),
        Err(CommandError::Unknown)
    );
    assert!(registry.menus().is_empty());
}

#[test]
fn invoke_runs_core_handlers_and_reports_ui_commands() {
    let runs = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&runs);
    let mut registry = Registry::new();
    registry
        .register(
            Command::new(
                "view.zoom_in",
                "Zoom &In",
                &["Ctrl+="],
                Handler::Core(Arc::new(move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                })),
            )
            .expect("valid"),
        )
        .expect("register");
    registry.register(ui("file.open")).expect("register");

    assert_eq!(registry.invoke("view.zoom_in"), Ok(Dispatch::Done));
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(registry.invoke("file.open"), Ok(Dispatch::Ui));
}

#[test]
fn invoke_fails_closed_on_unknown_or_malformed_ids() {
    let registry = builtin_registry();
    for id in ["file.delete_everything", "", "FILE.OPEN", "file.open\0"] {
        assert_eq!(registry.invoke(id), Err(CommandError::Unknown), "{id:?}");
    }
}

#[test]
fn builtin_registry_has_file_and_help_menus() {
    let registry = builtin_registry();
    let menus: Vec<&str> = registry.menus().iter().map(|m| m.id().as_str()).collect();
    assert_eq!(
        menus,
        [
            "file", "edit", "search", "view", "document", "project", "build", "help"
        ]
    );

    let file = registry.menu("file").expect("file menu");
    assert_eq!(file.title(), "&File");
    assert_eq!(file.items().len(), 24);
    assert_eq!(file.items()[3], MenuItem::Separator);

    let open = registry.get("file.open").expect("file.open");
    assert_eq!(open.title(), "&Open…");
    assert_eq!(open.default_keys(), ["Ctrl+O"]);
    for id in [
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "file.reload",
        "file.reload_with_encoding",
        "file.close",
        "file.close_others",
        "file.close_all",
        "file.reopen_closed",
        "app.quit",
        "view.next_tab",
        "view.previous_tab",
        "document.set_encoding",
        "help.about",
    ] {
        assert_eq!(registry.invoke(id), Ok(Dispatch::Ui), "{id}");
    }
}
