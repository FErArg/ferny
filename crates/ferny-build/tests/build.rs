//! Build commands (H2.4, threats T1–T3, T7).

use std::path::Path;

use ferny_build::{
    BuildError, Placeholders, UserCommands, error_location, expand, neutralise, split,
};
// Only the unix-gated resolution tests use `resolve`.
#[cfg(unix)]
use ferny_build::resolve;

fn words(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn geany_templates_split() {
    assert_eq!(
        split(r#"gcc -Wall -o "%e" "%f""#),
        Ok(words(&["gcc", "-Wall", "-o", "%e", "%f"]))
    );
    assert_eq!(split(r#""./%e""#), Ok(words(&["./%e"])));
    assert_eq!(
        split(r#"sh -c 'make && ./a' x\ y "q\"t""#),
        Ok(words(&["sh", "-c", "make && ./a", "x y", "q\"t"]))
    );
    assert_eq!(split(""), Err(BuildError::Invalid));
    assert_eq!(split("gcc \"open"), Err(BuildError::Invalid));
    assert_eq!(split("gcc\n-c"), Err(BuildError::Invalid));
}

#[test]
fn shell_syntax_is_rejected() {
    for bad in [
        "make && ./a",
        "cat x | sort",
        "a; b",
        "a > out",
        "a < in",
        "a &",
        "echo `id`",
        "echo $(id)",
    ] {
        assert_eq!(split(bad), Err(BuildError::ShellSyntax), "{bad}");
    }
    assert!(
        split("echo $HOME").is_ok(),
        "no expansion, but not an operator"
    );
}

#[test]
fn placeholders_never_split_arguments() {
    let path = Path::new("/w/a b;rm -rf ~ $(id)\"x.c");
    let values = Placeholders::for_file(path, 7);
    let argv = expand(
        &split(r#"gcc -o "%e" "%f" -DLINE=%l %%"#).expect("ok"),
        &values,
    )
    .expect("ok");
    assert_eq!(
        argv,
        words(&[
            "gcc",
            "-o",
            "a b;rm -rf ~ $(id)\"x",
            "a b;rm -rf ~ $(id)\"x.c",
            "-DLINE=7",
            "%"
        ])
    );
    assert_eq!(values.dir, "/w");
    let with_project = Placeholders {
        project: "/w/proj ect".to_owned(),
        ..values.clone()
    };
    assert_eq!(
        expand(&words(&["-C%p"]), &with_project),
        Ok(words(&["-C/w/proj ect"]))
    );
    assert_eq!(
        expand(&words(&["%q"]), &values),
        Err(BuildError::Placeholder)
    );
}

#[cfg(unix)]
#[test]
fn resolution_skips_project_and_relative_path() {
    use std::os::unix::fs::PermissionsExt;
    let base = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ferny-build-resolve");
    let _ = std::fs::remove_dir_all(&base);
    let project = base.join("project");
    let tools = base.join("tools");
    std::fs::create_dir_all(project.join("bin")).expect("ok");
    std::fs::create_dir_all(&tools).expect("ok");
    for dir in [project.clone(), project.join("bin"), tools.clone()] {
        let exe = dir.join("cc-test");
        std::fs::write(&exe, "#!/bin/sh\n").expect("ok");
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).expect("ok");
    }
    // `.`, an empty entry, a relative entry and a folder inside the project
    // come first on PATH: all skipped.
    let path = std::env::join_paths([
        Path::new("."),
        Path::new(""),
        Path::new("bin"),
        &project.join("bin"),
        &tools,
    ])
    .expect("ok");
    let found = resolve("cc-test", &project, &project, &path, false).expect("ok");
    assert_eq!(
        found,
        std::fs::canonicalize(tools.join("cc-test")).expect("ok")
    );
    // ./program only for Execute.
    assert_eq!(
        resolve("./cc-test", &project, &project, &path, false),
        Err(BuildError::NotFound)
    );
    assert!(resolve("./cc-test", &project, &project, &path, true).is_ok());
    assert_eq!(
        resolve("missing-tool", &project, &project, &path, false),
        Err(BuildError::NotFound)
    );
}

/// S1: the excluded folder is the workspace root, not the file's folder. A
/// program in a project subfolder on PATH must be skipped even when the file
/// sits in another subfolder.
#[cfg(unix)]
#[test]
fn resolution_excludes_workspace_root_not_file_folder() {
    use std::os::unix::fs::PermissionsExt;
    let base = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ferny-build-s1");
    let _ = std::fs::remove_dir_all(&base);
    let project = base.join("project");
    let venv_bin = project.join(".venv").join("bin");
    let src = project.join("src");
    std::fs::create_dir_all(&venv_bin).expect("ok");
    std::fs::create_dir_all(&src).expect("ok");
    let exe = venv_bin.join("cc-s1");
    std::fs::write(&exe, "#!/bin/sh\n").expect("ok");
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).expect("ok");
    let path = std::env::join_paths([venv_bin.as_path()]).expect("ok");
    // Excluding the file's folder (the old behaviour) lets the project copy run.
    assert!(resolve("cc-s1", &src, &src, &path, false).is_ok());
    // Excluding the workspace root refuses it.
    assert_eq!(
        resolve("cc-s1", &src, &project, &path, false),
        Err(BuildError::NotFound)
    );
}

#[test]
fn output_is_neutralised() {
    let raw = b"\x1b[1;31merror\x1b[0m: x\r\n\x1b]0;title\x07ok\tz \xe2\x80\xae evil \x07 \xff\n";
    assert_eq!(
        neutralise(raw),
        "error: x\nok\tz \\u{202E} evil \\u{0007} \u{FFFD}\n"
    );
}

#[test]
fn error_locations() {
    let loc = error_location("main.c:12:5: error: expected ';'").expect("ok");
    assert_eq!((loc.file.as_str(), loc.line, loc.column), ("main.c", 12, 5));
    let loc = error_location("  --> src/main.rs:3:9").expect("ok");
    assert_eq!(
        (loc.file.as_str(), loc.line, loc.column),
        ("src/main.rs", 3, 9)
    );
    let loc = error_location("x.py:4: warning").expect("ok");
    assert_eq!((loc.line, loc.column), (4, 0));
    assert!(error_location("Compilation finished").is_none());
    assert!(error_location("file.c:0: nope").is_none());
    assert!(error_location(&format!("{}:1:", "a".repeat(5000))).is_none());
}

#[test]
fn build_store_is_validated() {
    let (store, warnings) = UserCommands::parse(
        "[c]\ncompile = \"clang -c %f\"\nbuild = \"make && run\"\nnope = \"x\"\n[\"Bad Id\"]\nlint = \"x\"\n",
    );
    assert_eq!(store.get("c", "compile"), Some("clang -c %f"));
    assert_eq!(store.get("c", "build"), None);
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    let (_, warnings) = UserCommands::parse("not = = toml");
    assert_eq!(warnings.len(), 1);
    let mut store = store;
    store.set("c", "compile", "").expect("ok");
    assert_eq!(store.get("c", "compile"), None);
    assert_eq!(UserCommands::parse(&store.to_toml()).0, store);
}
