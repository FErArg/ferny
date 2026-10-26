//! Backup copies (H4.2a).

use std::fs;
use std::path::PathBuf;

use ferny_core::backup::{BackupError, MAX_COPIES, backup};

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-backup-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn copies_are_private_and_bounded() {
    let dir = temp("bounded");
    let file = dir.join("work/a.txt");
    fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
    let store = dir.join("data/backups");
    for i in 0..8u64 {
        fs::write(&file, format!("version {i}")).expect("write");
        // Two copies in the same second keep distinct names.
        let copy = backup(&store, &file, 1_800_000_000 + i / 2).expect("backup");
        assert_eq!(
            fs::read_to_string(&copy).expect("copy"),
            format!("version {i}")
        );
        assert!(copy.starts_with(&store));
    }
    let folder = fs::read_dir(&store)
        .expect("store")
        .next()
        .expect("folder")
        .expect("entry")
        .path();
    let mut names: Vec<String> = fs::read_dir(&folder)
        .expect("folder")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), MAX_COPIES);
    assert!(names.iter().all(|n| n.ends_with("a.txt")));
    // The oldest were removed: the newest copy is version 7.
    assert_eq!(
        fs::read_to_string(folder.join(names.last().expect("last"))).expect("read"),
        "version 7"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode =
            |p: &std::path::Path| fs::metadata(p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&folder.join(&names[0])), 0o600);
    }
    // Nothing next to the file.
    assert_eq!(
        fs::read_dir(file.parent().expect("parent"))
            .expect("work")
            .count(),
        1
    );
}

#[test]
fn nothing_to_back_up_and_linked_store() {
    let dir = temp("nothing");
    assert_eq!(
        backup(&dir.join("s"), &dir.join("missing"), 0),
        Err(BackupError::Nothing)
    );
    assert_eq!(backup(&dir.join("s"), &dir, 0), Err(BackupError::Nothing));
    #[cfg(unix)]
    {
        let file = dir.join("b.txt");
        fs::write(&file, "x").expect("write");
        fs::create_dir_all(dir.join("elsewhere")).expect("mkdir");
        std::os::unix::fs::symlink(dir.join("elsewhere"), dir.join("linked")).expect("link");
        assert_eq!(
            backup(&dir.join("linked"), &file, 0),
            Err(BackupError::Failed)
        );
        assert_eq!(fs::read_dir(dir.join("elsewhere")).expect("dir").count(), 0);
    }
}
