//! Projects and the `.geany` importer (H2.5a, threats T1, T3, T10, T11).

use std::fs;
use std::path::PathBuf;

use ferny_config::project::{
    self, MAX_PROJECT_BYTES, PROJECT_FILE, ProjectError, apply_geany_project, plan_geany_project,
};

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-project-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".ferny")).expect("mkdir");
    dir
}

fn write_project(dir: &std::path::Path, text: &str) {
    fs::write(dir.join(PROJECT_FILE), text).expect("write");
}

#[test]
fn open_valid_project() {
    let dir = temp("valid");
    fs::create_dir_all(dir.join("src")).expect("mkdir");
    write_project(
        &dir,
        "version = 1\nname = \"Demo\"\nbase_path = \"src\"\nfile_patterns = [\"*.c\", \"*.h\"]\n\
         [indentation]\nwidth = 3\nuse_tabs = true\n[build]\nmake = \"make -j4\"\ncompile = \"cc -c %f\"\n",
    );
    let (p, warnings) = project::open(&dir).expect("open");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(p.name, "Demo");
    assert_eq!(p.root, fs::canonicalize(&dir).expect("real"));
    assert_eq!(p.base, p.root.join("src"));
    assert_eq!(p.patterns, ["*.c", "*.h"]);
    assert_eq!((p.indent_width, p.indent_tabs), (Some(3), Some(true)));
    assert_eq!(p.build.get("make").map(String::as_str), Some("make -j4"));
}

#[test]
fn base_path_is_confined() {
    for base in ["../..", "/etc", "missing", "src/../.."] {
        let dir = temp("confined");
        fs::create_dir_all(dir.join("src")).expect("mkdir");
        write_project(
            &dir,
            &format!("version = 1\nname = \"x\"\nbase_path = \"{base}\"\n"),
        );
        let (p, warnings) = project::open(&dir).expect("open");
        assert_eq!(p.base, p.root, "{base}");
        assert_eq!(warnings.len(), 1, "{base}");
    }
    #[cfg(unix)]
    {
        // A link inside the project pointing outside it.
        let dir = temp("confined-link");
        std::os::unix::fs::symlink("/", dir.join("out")).expect("link");
        write_project(&dir, "version = 1\nname = \"x\"\nbase_path = \"out\"\n");
        let (p, _) = project::open(&dir).expect("open");
        assert_eq!(p.base, p.root);
    }
}

#[test]
fn invalid_entries_are_dropped() {
    let dir = temp("dropped");
    write_project(
        &dir,
        "version = 1\nname = \"x\"\nfile_patterns = [\"*.c\", \"../x\", \"\"]\n\
         [indentation]\nwidth = 99\n[build]\nbuild = \"make && rm -rf ~\"\nhack = \"x\"\nexecute = \"./%e\"\n",
    );
    let (p, warnings) = project::open(&dir).expect("open");
    assert_eq!(p.patterns, ["*.c"]);
    assert_eq!(p.indent_width, None);
    assert_eq!(p.build.keys().collect::<Vec<_>>(), ["execute"]);
    assert_eq!(warnings.len(), 5, "{warnings:?}");
}

#[test]
fn invalid_files_are_refused() {
    let dir = temp("refused");
    assert_eq!(
        project::open(&dir.join("none")),
        Err(ProjectError::NotFound)
    );
    assert_eq!(project::open(&dir), Err(ProjectError::NotFound));
    for text in [
        "not = = toml".to_owned(),
        "version = 2\nname = \"x\"\n".to_owned(),
        "version = 1\nname = \"\"\n".to_owned(),
        "version = 1\nname = \"a\\u0007\"\n".to_owned(),
        format!(
            "version = 1\nname = \"x\"\n#{}\n",
            "x".repeat(MAX_PROJECT_BYTES as usize)
        ),
    ] {
        write_project(&dir, &text);
        assert_eq!(
            project::open(&dir),
            Err(ProjectError::Invalid),
            "{text:.40}"
        );
    }
}

#[test]
fn create_never_overwrites() {
    let dir = temp("create");
    fs::remove_dir_all(dir.join(".ferny")).expect("rm");
    project::create(&dir, "New").expect("create");
    assert_eq!(project::open(&dir).expect("open").0.name, "New");
    assert_eq!(project::create(&dir, "Again"), Err(ProjectError::Exists));
    assert_eq!(project::create(&dir, ""), Err(ProjectError::Invalid));
}

#[test]
fn geany_project_import_maps_and_lists() {
    let dir = temp("geany");
    fs::remove_dir_all(dir.join(".ferny")).expect("rm");
    fs::create_dir_all(dir.join("code")).expect("mkdir");
    let geany = dir.join("demo.geany");
    fs::write(
        &geany,
        "[editor]\nline_wrapping=false\n\n[project]\nname=Demo\nbase_path=code\n\
         description=x\nfile_patterns=*.c;*.h;\n\n[indentation]\nindent_width=2\nindent_type=0\n\n\
         [build-menu]\nNF_00_CM=make\nFT_00_CM=gcc -c \"%f\"\nEX_00_CM=./run.sh && rm -rf ~\n\n\
         [files]\ncurrent_page=0\nFILE_NAME_0=1;C;0;EUTF-8;1;1;0;%2Fhome%2Fx.c;0;4\n",
    )
    .expect("write");
    let plan = plan_geany_project(&geany).expect("plan");
    assert_eq!(
        plan.target,
        fs::canonicalize(dir.join("code")).expect("real")
    );
    assert!(
        plan.not_imported.iter().any(|n| n.contains("EX_00_CM")),
        "{plan:?}"
    );
    for listed in [
        "editor/line_wrapping",
        "project/description",
        "files/FILE_NAME_0",
    ] {
        assert!(plan.not_imported.iter().any(|n| n == listed), "{listed}");
    }
    apply_geany_project(&plan).expect("apply");
    let (p, warnings) = project::open(&dir.join("code")).expect("open");
    assert!(warnings.is_empty());
    assert_eq!(p.name, "Demo");
    assert_eq!(p.patterns, ["*.c", "*.h"]);
    assert_eq!((p.indent_width, p.indent_tabs), (Some(2), Some(false)));
    assert_eq!(p.build.keys().collect::<Vec<_>>(), ["compile", "make"]);
    assert_eq!(apply_geany_project(&plan), Err(ProjectError::Exists));
}

#[test]
fn session_is_outside_the_project() {
    let id = project::session_id(std::path::Path::new("/home/x/src/demo"));
    assert_eq!(id.len(), 16);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(
        id,
        project::session_id(std::path::Path::new("/home/x/src/demo2"))
    );
}
