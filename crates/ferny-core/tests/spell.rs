//! Spell checking (H4.2c).

use std::fs;
use std::path::PathBuf;

use ferny_core::spell::{SpellError, Speller, valid_language};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures/spell")
}

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-spell-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn finds_misspelled_words() {
    let speller = Speller::load("xx_TEST", &[fixtures()], None).expect("load");
    let text = "hello wrold, ferny edtor café x 42abc it's";
    let found: Vec<&str> = speller
        .misspelled(text)
        .iter()
        .map(|(s, e)| &text[*s..*e])
        .collect();
    assert_eq!(found, ["wrold", "edtor", "it's"]);
    assert!(speller.suggest("wrold").contains(&"world".to_owned()));
    assert!(speller.suggest("not a word").is_empty());
}

#[test]
fn dictionaries_are_looked_up_in_order_and_validated() {
    let empty = temp("empty");
    assert_eq!(
        Speller::load("xx_TEST", std::slice::from_ref(&empty), None).err(),
        Some(SpellError::NotFound)
    );
    assert!(Speller::load("xx_TEST", &[empty.clone(), fixtures()], None).is_ok());
    for bad in ["../xx_TEST", "xx/TEST", "", "toolong_TEST1", "e"] {
        assert!(!valid_language(bad), "{bad}");
        assert_eq!(
            Speller::load(bad, &[fixtures()], None).err(),
            Some(SpellError::Language)
        );
    }
    let broken = temp("broken");
    fs::write(broken.join("yy.aff"), [0xff, 0xfe, 0x00]).expect("aff");
    fs::write(broken.join("yy.dic"), "1\nword\n").expect("dic");
    assert_eq!(
        Speller::load("yy", &[broken], None).err(),
        Some(SpellError::Invalid)
    );
}

#[test]
fn personal_words_are_kept_private() {
    let dir = temp("personal");
    let list = dir.join("dictionaries/personal.txt");
    let mut speller = Speller::load("xx_TEST", &[fixtures()], Some(&list)).expect("load");
    assert_eq!(speller.misspelled("geany").len(), 1);
    assert!(speller.add("geany"));
    assert!(!speller.add("two words") && !speller.add("../x") && !speller.add(&"a".repeat(65)));
    assert!(speller.misspelled("geany").is_empty());
    assert_eq!(fs::read_to_string(&list).expect("list"), "geany\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&list).expect("meta").permissions().mode() & 0o777,
            0o600
        );
    }
    let again = Speller::load("xx_TEST", &[fixtures()], Some(&list)).expect("reload");
    assert!(again.misspelled("geany").is_empty());
}

#[test]
fn checks_are_bounded() {
    let speller = Speller::load("xx_TEST", &[fixtures()], None).expect("load");
    let big = "zz ".repeat(400_000);
    assert!(speller.misspelled(&big).is_empty()); // over the size limit: not checked
    let many = "qq ".repeat(20_000);
    assert_eq!(
        speller.misspelled(&many).len(),
        ferny_core::spell::MAX_MISSPELLINGS
    );
    let long = "q".repeat(100);
    assert!(speller.misspelled(&long).is_empty());
}
