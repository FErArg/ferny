//! User filetypes and colour schemes on top of the built-in catalog.

use std::fs;
use std::path::PathBuf;

use ferny_syntax::Catalog;

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-user-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("filetypes")).expect("mkdir");
    fs::create_dir_all(dir.join("colorschemes")).expect("mkdir");
    dir
}

#[test]
fn user_catalog_rejects_invalid_files() {
    let dir = temp("invalid");
    for (name, text) in [
        ("bad_id.toml", "id = \"../etc\"\nbase = \"c\"\n"),
        ("no_base.toml", "id = \"newtype\"\n"),
        (
            "command.toml",
            "id = \"c\"\n[settings]\ncontext_action_cmd = \"rm -rf /\"\n",
        ),
        ("garbage.toml", "this is = = not toml"),
    ] {
        fs::write(dir.join("filetypes").join(name), text).expect("write");
    }
    fs::write(
        dir.join("colorschemes/bad.toml"),
        "id = \"Bad Id\"\nname = \"x\"\ndescription = \"\"\nauthor = \"\"\nlicense = \"\"\n",
    )
    .expect("write");
    let (catalog, warnings) = Catalog::with_user_dir(&dir);
    assert_eq!(warnings.len(), 5, "{warnings:?}");
    assert_eq!(
        catalog.filetypes().len(),
        Catalog::builtin().filetypes().len()
    );
    assert!(
        !catalog
            .filetype("c")
            .expect("c")
            .settings
            .contains_key("context_action_cmd")
    );
}

#[test]
fn user_catalog_missing_dir_is_builtin() {
    let (catalog, warnings) = Catalog::with_user_dir(&temp("missing").join("nope"));
    assert!(warnings.is_empty());
    assert_eq!(catalog.schemes().len(), Catalog::builtin().schemes().len());
}
