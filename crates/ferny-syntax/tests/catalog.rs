//! The embedded catalog: every file parses, detection follows Geany, and
//! styles resolve with Geany's semantics.

use std::collections::{BTreeMap, HashMap};

use ferny_syntax::{Catalog, DEFAULT_SCHEME, Style, glob_match, parse_color, parse_style};

#[test]
fn every_embedded_file_parses() {
    let catalog = Catalog::builtin();
    let (filetypes, schemes) = Catalog::embedded_counts();
    assert_eq!(catalog.filetypes().len(), filetypes);
    assert_eq!(catalog.schemes().len(), schemes);
    assert!(filetypes >= 90, "{filetypes} filetypes");
    assert!(schemes >= 46, "{schemes} schemes");
    assert!(catalog.scheme(DEFAULT_SCHEME).is_some());
}

#[test]
fn every_scheme_has_a_known_licence() {
    for scheme in Catalog::builtin().schemes() {
        assert_ne!(scheme.license, "unknown", "{}", scheme.id);
    }
}

#[test]
fn every_named_style_reference_resolves() {
    let catalog = Catalog::builtin();
    for scheme in catalog.schemes() {
        let named = catalog.named_styles(&scheme.id);
        assert!(named.contains_key("default"), "{}", scheme.id);
        for ft in catalog.filetypes() {
            let (_, styles) = catalog.resolve_styles(ft, &scheme.id);
            assert_eq!(styles.len(), ft.styles.len());
        }
    }
}

#[test]
fn detection_by_name() {
    let catalog = Catalog::builtin();
    let id = |name: &str| catalog.detect(name, "").map(|ft| ft.id.as_str());
    assert_eq!(id("main.c"), Some("c"));
    assert_eq!(id("/src/widget.cpp"), Some("cpp"));
    assert_eq!(id("lib.rs"), Some("rust"));
    assert_eq!(id("setup.py"), Some("python"));
    assert_eq!(id("Makefile"), Some("makefile"));
    assert_eq!(id("CMakeLists.txt"), Some("cmake"));
    assert_eq!(id("sketch.ino"), Some("arduino"));
    assert_eq!(id("notes.unknownext"), None);
}

#[test]
fn detection_by_shebang() {
    let catalog = Catalog::builtin();
    let id = |line: &str| catalog.detect("script", line).map(|ft| ft.id.as_str());
    assert_eq!(id("#!/usr/bin/env python3"), Some("python"));
    assert_eq!(id("#!/bin/bash"), Some("sh"));
    assert_eq!(id("#!/usr/bin/perl -w"), Some("perl"));
    assert_eq!(id("#!/bin/busybox ash"), Some("sh"));
    assert_eq!(id("# not a shebang"), None);
    // As in Geany, the shebang wins over the file name.
    assert_eq!(
        catalog.detect("x.rb", "#!/bin/sh").map(|ft| ft.id.as_str()),
        Some("sh")
    );
}

#[test]
fn detection_by_modeline_as_in_geany() {
    let catalog = Catalog::builtin();
    let id = |name: &str, header: &str| catalog.detect(name, header).map(|ft| ft.id.as_str());
    assert_eq!(id("notes", "# -*- Python -*-"), Some("python"));
    assert_eq!(id("x.txt", "first line\n/* -*- C++ -*- */"), Some("cpp"));
    // Only the first two lines, exact names, Geany's single-word form.
    assert_eq!(id("notes", "a\nb\n# -*- Python -*-"), None);
    assert_eq!(id("notes", "# -*- python -*-"), None);
    assert_eq!(id("notes", "# -*- mode: python -*-"), None);
    // The shebang is checked before the modeline.
    assert_eq!(id("notes", "#!/bin/sh\n# -*- Python -*-"), Some("sh"));
}

#[test]
fn c_keywords_and_styles_resolve_in_the_default_scheme() {
    let catalog = Catalog::builtin();
    let c = catalog.filetype("c").expect("c");
    assert_eq!(c.lexer, "cpp");
    assert!(c.keywords[0].words.split(' ').any(|w| w == "while"));
    assert_eq!(
        c.settings.get("comment_single").map(String::as_str),
        Some("//")
    );
    let (default, styles) = catalog.resolve_styles(c, DEFAULT_SCHEME);
    assert_eq!((default.fore, default.back), (0x000000, 0xffffff));
    // SCE_C_COMMENT = 1 uses the named style "comment" (0xd00000).
    let comment = styles.iter().find(|s| s.style == 1).expect("comment");
    assert_eq!(comment.look.fore, 0xd00000);
}

#[test]
fn dark_scheme_uses_its_named_colours() {
    let catalog = Catalog::builtin();
    let named = catalog.named_styles("darcula");
    let default = named.get("default").expect("default");
    assert_eq!((default.fore, default.back), (0xb6c3cf, 0x3b3b3b));
}

#[test]
fn style_values() {
    let base = Style {
        fore: 1,
        back: 2,
        bold: false,
        italic: false,
    };
    let mut named = HashMap::new();
    named.insert(
        "comment".to_owned(),
        Style {
            fore: 0xaa0000,
            back: 2,
            bold: false,
            italic: false,
        },
    );
    let mut colors = BTreeMap::new();
    colors.insert("bg".to_owned(), "#123".to_owned());
    let s = |v: &str| parse_style(v, base, &named, &colors);
    assert_eq!(s("comment").fore, 0xaa0000);
    assert!(s("comment,bold").bold);
    assert!(s("comment,italic").italic);
    assert_eq!(s("unknown,bold"), base);
    assert_eq!(s(""), base);
    assert_eq!(
        s("0x00ff00;bg;true;false"),
        Style {
            fore: 0x00ff00,
            back: 0x112233,
            bold: true,
            italic: false
        }
    );
    assert_eq!(
        s(";;;true"),
        Style {
            italic: true,
            ..base
        }
    );
    assert_eq!(s("nonsense;#zzz"), base);
}

#[test]
fn colours() {
    let none = BTreeMap::new();
    assert_eq!(parse_color("#0099cc", &none), Some(0x0099cc));
    assert_eq!(parse_color("#09c", &none), Some(0x0099cc));
    assert_eq!(parse_color("0x0099CC", &none), Some(0x0099cc));
    for bad in ["", "red", "#12", "#1234567", "0x", "#gggggg"] {
        assert_eq!(parse_color(bad, &none), None, "{bad:?}");
    }
}

#[test]
fn globs() {
    assert!(glob_match("*.c", "main.c"));
    assert!(glob_match("Makefile*", "Makefile.am"));
    assert!(glob_match("?.h", "a.h"));
    assert!(!glob_match("*.c", "main.cc"));
    assert!(glob_match("*", ""));
    // Pathological pattern stays fast (no exponential backtracking).
    let text = "a".repeat(10_000);
    assert!(!glob_match("*a*a*a*a*a*b", &text));
}
