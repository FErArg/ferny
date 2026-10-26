//! File templates and snippets (H2.5b).

use std::fs;
use std::path::PathBuf;

use ferny_config::templates::{
    Comment, MAX_TEMPLATE_BYTES, Snippets, Values, expand, expand_wildcards, file_templates,
};

fn values() -> Values {
    Values {
        developer: "Ada".into(),
        mail: "ada@example.invalid".into(),
        year: "2026".into(),
        filename: "main.c".into(),
        company: "ACME".into(),
        ..Values::default()
    }
}

fn c_comment() -> Comment {
    Comment {
        open: "/*".into(),
        close: "*/".into(),
        single: "//".into(),
    }
}

fn temp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("ferny-templates-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("templates/files")).expect("mkdir");
    dir
}

#[test]
fn geany_templates_expand() {
    let all = file_templates(None);
    assert_eq!(all.len(), 14);
    let main = &all.iter().find(|(n, _)| n == "main.c").expect("main.c").1;
    let text = expand(main, &values(), &c_comment(), None);
    assert!(
        text.starts_with("/*\n * main.c\n *\n * Copyright 2026 Ada <ada@example.invalid>\n"),
        "{text}"
    );
    assert!(text.contains(" * This program is free software"), "{text}");
    assert!(text.contains(" */\n\n#include <stdio.h>"), "{text}");
    assert!(!text.contains("{file") && !text.contains("{gpl}"), "{text}");
    // Line comments when the filetype has no block comment.
    let hash = Comment {
        single: "#".into(),
        ..Comment::default()
    };
    let py = expand("{fileheader}\nprint()\n", &values(), &hash, None);
    assert!(py.starts_with("# main.c\n#\n# Copyright 2026 Ada"), "{py}");
}

#[test]
fn command_wildcard_is_not_run() {
    let text = expand_wildcards("{command:rm -rf ~} {nope} {developer} {ob}x{cb}", &values());
    assert_eq!(text, "{command:rm -rf ~} {nope} Ada {x}");
}

#[test]
fn user_templates_are_bounded() {
    let dir = temp("user");
    fs::write(dir.join("templates/files/main.c"), "mine {developer}").expect("write");
    fs::write(dir.join("templates/files/new.txt"), "new").expect("write");
    fs::write(
        dir.join("templates/files/huge.txt"),
        "x".repeat(MAX_TEMPLATE_BYTES as usize + 1),
    )
    .expect("write");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/hosts", dir.join("templates/files/link.txt")).expect("link");
    let all = file_templates(Some(&dir));
    let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"new.txt"));
    assert!(!names.contains(&"huge.txt") && !names.contains(&"link.txt"));
    assert_eq!(
        all.iter().find(|(n, _)| n == "main.c").expect("main").1,
        "mine {developer}"
    );
    // A user fileheader replaces Geany's.
    fs::write(dir.join("templates/fileheader"), "by {developer}").expect("write");
    let text = expand("{fileheader}", &values(), &Comment::default(), Some(&dir));
    assert_eq!(text, "by Ada");
}

#[test]
fn geany_snippets_expand() {
    let snippets = Snippets::load(None);
    let (text, cursor) = snippets
        .expand("C", "for", "    ", "    ", "\n", &values())
        .expect("for");
    assert_eq!(text, "for (i = 0; i < ; i++)\n    {\n        \n    }");
    assert_eq!(cursor, Some(16));
    // Default group and unknown keywords.
    assert!(
        snippets
            .expand("C", "nothing", "", "\t", "\n", &values())
            .is_none()
    );
    assert!(
        snippets
            .expand("Special", "block", "", "\t", "\n", &values())
            .is_none()
    );
}

#[test]
fn user_snippets_replace_geany() {
    let dir = temp("snippets");
    fs::write(
        dir.join("snippets.conf"),
        "[Default]\nsig=-- %cursor%{developer}\\s\n[C]\nhi=\\{command:id}\n",
    )
    .expect("write");
    let snippets = Snippets::load(Some(&dir));
    assert!(
        snippets
            .expand("C", "for", "", "\t", "\n", &values())
            .is_none()
    );
    let (text, cursor) = snippets
        .expand("Rust", "sig", "", "\t", "\n", &values())
        .expect("sig");
    assert_eq!((text.as_str(), cursor), ("-- Ada ", Some(3)));
    let (text, _) = snippets
        .expand("C", "hi", "", "\t", "\n", &values())
        .expect("hi");
    assert!(text.contains("{command:id}"), "{text}");
}
