//! LSP client core (H3.1a, threats T1, T2, T6, T7, T16).

#[cfg(unix)]
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use ferny_lsp::config::{self, for_filetype};
use ferny_lsp::{
    FrameError, MAX_DIAGNOSTICS, MAX_HEADER_BYTES, MAX_MESSAGE_BYTES, byte_offset, is_secret_file,
    parse_diagnostics, read_message, write_message,
};
#[cfg(unix)]
use ferny_lsp::{path_to_uri, server_environment, uri_to_path};
use serde_json::json;

#[test]
fn framing_round_trip_and_bounds() {
    let mut out = Vec::new();
    write_message(&mut out, b"{\"a\":1}").expect("write");
    write_message(&mut out, b"{}").expect("write");
    let mut reader = Cursor::new(out);
    assert_eq!(read_message(&mut reader).expect("one"), b"{\"a\":1}");
    assert_eq!(read_message(&mut reader).expect("two"), b"{}");
    assert_eq!(read_message(&mut reader), Err(FrameError::Closed));
}

#[test]
fn framing_is_bounded() {
    let cases: Vec<Vec<u8>> = vec![
        b"Content-Type: x\r\n\r\n{}".to_vec(),
        format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1).into_bytes(),
        b"Content-Length: -1\r\n\r\n".to_vec(),
        b"no colon here\r\n\r\n".to_vec(),
        format!("X: {}\r\n\r\n", "a".repeat(MAX_HEADER_BYTES)).into_bytes(),
        vec![0xff, b':', b'\n', b'\n'],
    ];
    for case in cases {
        assert_eq!(
            read_message(&mut Cursor::new(case)),
            Err(FrameError::Invalid)
        );
    }
    // A length longer than the stream: closed, not a hang.
    let short = b"Content-Length: 10\r\n\r\n{}".to_vec();
    assert_eq!(
        read_message(&mut Cursor::new(short)),
        Err(FrameError::Closed)
    );
}

#[test]
fn secret_files_are_not_synced() {
    for name in [
        ".env",
        ".env.local",
        "server.pem",
        "x.KEY",
        "id_rsa",
        "id_ed25519.pub",
        "credentials.json",
        ".netrc",
        ".npmrc",
        "keys.kdbx",
        "cert.p12",
    ] {
        assert!(is_secret_file(Path::new(name)), "{name}");
    }
    for name in ["main.rs", "env.rs", "keyboard.c", "README.md", "Cargo.toml"] {
        assert!(!is_secret_file(Path::new(name)), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn environment_is_reduced() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ferny-lsp-env");
    std::fs::create_dir_all(dir.join("bin")).expect("mkdir");
    let path = format!("/usr/bin:.::relative:{}:/bin", dir.join("bin").display());
    let current = vec![
        ("PATH", path.as_str()),
        ("HOME", "/home/x"),
        ("LANG", "es_AR.UTF-8"),
        ("LC_ALL", "C"),
        ("AWS_SECRET_ACCESS_KEY", "synthetic"),
        ("GITHUB_TOKEN", "synthetic"),
        ("CPATH", "/opt/include"),
    ]
    .into_iter()
    .map(|(k, v)| (OsString::from(k), OsString::from(v)));
    let env = server_environment(current, &["CPATH".to_owned()], &dir);
    let get = |k: &str| {
        env.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.to_string_lossy().into_owned())
    };
    assert_eq!(get("PATH").as_deref(), Some("/usr/bin:/bin"));
    assert_eq!(get("HOME").as_deref(), Some("/home/x"));
    assert_eq!(get("LC_ALL").as_deref(), Some("C"));
    assert_eq!(get("CPATH").as_deref(), Some("/opt/include"));
    assert!(get("AWS_SECRET_ACCESS_KEY").is_none() && get("GITHUB_TOKEN").is_none());
}

#[cfg(unix)]
#[test]
fn uris_round_trip_and_reject_junk() {
    let path = Path::new("/w/a b/ñ#%?.rs");
    let uri = path_to_uri(path);
    assert_eq!(uri, "file:///w/a%20b/%C3%B1%23%25%3F.rs");
    assert_eq!(uri_to_path(&uri).as_deref(), Some(path));
    for bad in [
        "http://x/y",
        "file://relative",
        "file:///a%2",
        "file:///a%0Ab",
        "file:///%ff",
    ] {
        assert!(uri_to_path(bad).is_none(), "{bad}");
    }
}

#[test]
fn diagnostics_are_parsed_and_bounded() {
    let many: Vec<_> = (0..MAX_DIAGNOSTICS + 5)
        .map(|i| {
            json!({"range": {"start": {"line": i, "character": 0},
                                  "end": {"line": i, "character": 3}},
                        "severity": 2, "message": "w"})
        })
        .collect();
    let (uri, items) =
        parse_diagnostics(&json!({"uri": "file:///x.c", "diagnostics": many})).expect("parse");
    assert_eq!(uri, "file:///x.c");
    assert_eq!(items.len(), MAX_DIAGNOSTICS);
    // Bad entries are skipped, severity defaults to error, text neutralised.
    let (_, items) = parse_diagnostics(&json!({"uri": "u", "diagnostics": [
        {"range": {"start": {"line": -1, "character": 0}, "end": {"line": 0, "character": 0}}, "message": "x"},
        {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
         "severity": 9, "message": "evil \u{202E} \u{1b}[31mred"},
    ]})).expect("parse");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].severity, 1);
    assert_eq!(items[0].message, "evil \\u{202E} red");
}

#[test]
fn positions_are_clamped() {
    let text = "añb\n😀x\nlast";
    assert_eq!(byte_offset(text, (0, 2), true), 3); // after ñ
    assert_eq!(byte_offset(text, (1, 2), true), 9); // after 😀 (2 UTF-16 units)
    assert_eq!(byte_offset(text, (1, 99), true), 10); // end of line
    assert_eq!(byte_offset(text, (9, 0), true), text.len());
    assert_eq!(byte_offset(text, (0, 2), false), 1); // inside ñ: back to a boundary
}

#[test]
fn configuration_layers() {
    let (servers, warnings) = config::load(None);
    assert!(warnings.is_empty());
    assert_eq!(
        for_filetype(&servers, "cpp").map(|s| s.name.as_str()),
        Some("clangd")
    );
    assert!(for_filetype(&servers, "make").is_none());

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ferny-lsp-config");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("lsp.toml");
    std::fs::write(
        &path,
        "[servers.clangd]\nenabled = false\n[servers.zls]\ncommand = [\"zls\"]\nfiletypes = [\"zig\"]\n\
         [servers.pylsp]\ncommand = [\"pylsp\", \"-v\"]\nenv = [\"VIRTUAL_ENV\"]\n\
         [servers.bad]\ncommand = []\nfiletypes = [\"x\"]\n[servers.half]\ncommand = [\"x\"]\n",
    )
    .expect("write");
    let (servers, warnings) = config::load(Some(&path));
    assert!(for_filetype(&servers, "c").is_none());
    assert_eq!(
        for_filetype(&servers, "zig").map(|s| s.name.as_str()),
        Some("zls")
    );
    let pylsp = for_filetype(&servers, "python").expect("pylsp");
    assert_eq!((pylsp.command.len(), pylsp.env.len()), (2, 1));
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    std::fs::write(&path, "servers = 1").expect("write");
    assert_eq!(config::load(Some(&path)).1.len(), 1);
}

#[cfg(unix)]
#[test]
fn lifecycle_with_a_scripted_server() {
    use std::sync::mpsc;
    use std::time::Duration;

    use ferny_lsp::client::{Event, Server};

    // A fake server: answers initialize, publishes one diagnostic, then
    // waits for stdin to close.
    let body1 =
        r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"positionEncoding":"utf-8"}}}"#;
    let body2 = r#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///x.c","diagnostics":[{"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":2}},"severity":1,"message":"boom"}]}}"#;
    let script = format!(
        "printf 'Content-Length: {}\\r\\n\\r\\n%s' '{}'; printf 'Content-Length: {}\\r\\n\\r\\n%s' '{}'; cat > /dev/null",
        body1.len(),
        body1,
        body2.len(),
        body2
    );
    let (tx, rx) = mpsc::channel();
    let server = Server::spawn(
        Path::new("/bin/sh"),
        &["-c".to_owned(), script],
        Path::new("/"),
        Vec::new(),
        move |event| {
            let _ = tx.send(event);
        },
    )
    .expect("spawn");
    let wait = || rx.recv_timeout(Duration::from_secs(5)).expect("event");
    assert_eq!(wait(), Event::Ready);
    assert!(server.is_ready() && server.utf8_positions());
    match wait() {
        Event::Diagnostics(uri, items) => {
            assert_eq!(uri, "file:///x.c");
            assert_eq!(items[0].message, "boom");
        }
        other => panic!("{other:?}"),
    }
    server.did_open("file:///x.c", "c", 1, "int");
    server.shutdown();
    assert_eq!(wait(), Event::Exited(false));
}

#[cfg(unix)]
#[test]
fn lifecycle_server_that_dies() {
    use std::sync::mpsc;
    use std::time::Duration;

    use ferny_lsp::client::{Event, Server};
    let (tx, rx) = mpsc::channel();
    let _server = Server::spawn(
        Path::new("/bin/sh"),
        &["-c".to_owned(), "exit 3".to_owned()],
        Path::new("/"),
        Vec::new(),
        move |event| {
            let _ = tx.send(event);
        },
    )
    .expect("spawn");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).expect("event"),
        Event::Exited(true)
    );
}

#[test]
fn positions_round_trip() {
    use ferny_lsp::lsp_position;
    let text = "añb\n😀x\nlast";
    for utf16 in [true, false] {
        // Character boundaries only (8 is inside the emoji).
        for offset in [0, 1, 3, 4, 5, 9, 10, 11, text.len()] {
            let position = lsp_position(text, offset, utf16);
            assert_eq!(
                byte_offset(text, position, utf16),
                offset,
                "{offset} {utf16}"
            );
        }
    }
    assert_eq!(lsp_position(text, 9, true), (1, 2));
    assert_eq!(lsp_position(text, 9, false), (1, 4));
    assert_eq!(lsp_position(text, 999, true), (2, 4));
}

#[test]
fn answers_are_parsed_and_bounded() {
    use ferny_lsp::{MAX_ITEMS, parse_completion, parse_hover, parse_locations, parse_signature};
    // Completion: list or array; newText, insertText, label; no newlines.
    let list = json!({"isIncomplete": false, "items": [
        {"label": "helper(int)", "insertText": "helper"},
        {"label": "x", "textEdit": {"newText": "xyz", "range": {}}},
        {"label": "only"},
        {"label": "multi", "insertText": "a\nb"},
        {"insertText": 3}
    ]});
    assert_eq!(parse_completion(&list), ["helper", "xyz", "only"]);
    let many: Vec<_> = (0..MAX_ITEMS + 9)
        .map(|i| json!({"label": format!("w{i}")}))
        .collect();
    assert_eq!(parse_completion(&json!(many)).len(), MAX_ITEMS);
    assert!(parse_completion(&json!(null)).is_empty());

    // Hover: markup, marked strings, arrays; neutralised.
    assert_eq!(
        parse_hover(&json!({"contents": {"kind": "markdown", "value": "int f()"}})),
        "int f()"
    );
    assert_eq!(
        parse_hover(&json!({"contents": ["a", {"language": "c", "value": "b"}]})),
        "a\nb"
    );
    assert_eq!(
        parse_hover(&json!({"contents": "x \u{1b}[31m\u{202E}y"})),
        "x \\u{202E}y"
    );
    assert_eq!(parse_hover(&json!(null)), "");

    // Signature: string and offset parameter labels.
    let (label, range) = parse_signature(&json!({"signatures": [{"label": "int f(int a, char b)",
        "parameters": [{"label": "int a"}, {"label": [13, 19]}]}], "activeParameter": 1}))
    .expect("signature");
    assert_eq!(label, "int f(int a, char b)");
    assert_eq!(range, Some((13, 19)));
    assert!(parse_signature(&json!({"signatures": []})).is_none());
    assert!(parse_signature(&json!({"signatures": [{"label": "a\nb"}]})).is_none());

    // Locations: single, array, links.
    let range = json!({"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 9}});
    assert_eq!(
        parse_locations(&json!({"uri": "file:///a.c", "range": range})),
        [("file:///a.c".to_owned(), (3, 4))]
    );
    assert_eq!(
        parse_locations(
            &json!([{"targetUri": "file:///b.h", "targetSelectionRange": range,
        "targetRange": range}])
        ),
        [("file:///b.h".to_owned(), (3, 4))]
    );
    assert!(parse_locations(&json!(null)).is_empty());
}

#[test]
fn edits_are_parsed_bounded_and_applied() {
    use ferny_lsp::{MAX_EDITS, apply_edits, parse_text_edits, parse_workspace_edit};
    let edit = |l: u32, c: u32, l2: u32, c2: u32, t: &str| {
        json!({"range":
        {"start": {"line": l, "character": c}, "end": {"line": l2, "character": c2}}, "newText": t})
    };
    // Rename: `changes` and `documentChanges`; file operations refused.
    let changes = json!({"changes": {"file:///a.c": [edit(0, 4, 0, 10, "assist")]}});
    let files = parse_workspace_edit(&changes).expect("changes");
    assert_eq!(files.len(), 1);
    let doc = json!({"documentChanges": [{"textDocument": {"uri": "file:///b.h", "version": 1},
        "edits": [edit(1, 0, 1, 1, "x")]}]});
    assert_eq!(parse_workspace_edit(&doc).expect("doc")[0].0, "file:///b.h");
    assert!(
        parse_workspace_edit(&json!({"documentChanges": [{"kind": "delete", "uri": "file:///x"}]}))
            .is_none()
    );
    let many: Vec<_> = (0..MAX_EDITS + 1)
        .map(|i| edit(i as u32, 0, i as u32, 0, "y"))
        .collect();
    assert!(parse_workspace_edit(&json!({"changes": {"file:///a": many}})).is_none());
    // Formatting.
    assert_eq!(parse_text_edits(&json!(null)), Some(Vec::new()));
    let edits =
        parse_text_edits(&json!([edit(0, 0, 0, 3, "int"), edit(1, 0, 1, 1, "  ")])).expect("edits");
    // Applied from the end; UTF-16 positions after a non-BMP character.
    assert_eq!(
        apply_edits("chr x;\n\tz", &edits, true).as_deref(),
        Some("int x;\n  z")
    );
    let emoji = parse_text_edits(&json!([edit(0, 2, 0, 3, "B")])).expect("emoji");
    assert_eq!(apply_edits("😀b", &emoji, true).as_deref(), Some("😀B"));
    // Overlapping edits are refused.
    let overlap =
        parse_text_edits(&json!([edit(0, 0, 0, 3, "a"), edit(0, 2, 0, 4, "b")])).expect("o");
    assert!(apply_edits("abcdef", &overlap, false).is_none());
}

#[test]
fn symbols_are_parsed_flattened_and_bounded() {
    use ferny_lsp::{parse_symbols, symbol_kind_name};
    let nested = serde_json::json!([{
        "name": "Widget\u{202E}", "kind": 5,
        "range": {"start": {"line": 1, "character": 0}, "end": {"line": 9, "character": 1}},
        "selectionRange": {"start": {"line": 1, "character": 6}, "end": {"line": 1, "character": 12}},
        "children": [{"name": "draw", "kind": 6,
            "range": {"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 20}},
            "selectionRange": {"start": {"line": 3, "character": 9}, "end": {"line": 3, "character": 13}}}]
    }]);
    let symbols = parse_symbols(&nested);
    assert_eq!(symbols.len(), 2);
    assert!(!symbols[0].0.contains('\u{202E}'));
    assert_eq!(symbols[1], ("draw".to_owned(), 6, (String::new(), (3, 9))));
    let flat = serde_json::json!([{"name": "main", "kind": 12,
        "location": {"uri": "file:///w/a.c", "range": {"start": {"line": 4, "character": 0},
                                                        "end": {"line": 4, "character": 4}}}}]);
    assert_eq!(
        parse_symbols(&flat)[0].2,
        ("file:///w/a.c".to_owned(), (4, 0))
    );
    let many: Vec<_> = (0..2000)
        .map(|i| {
            serde_json::json!({"name": format!("s{i}"), "kind": 12,
        "location": {"uri": "file:///w/a.c"}})
        })
        .collect();
    assert_eq!(
        parse_symbols(&serde_json::Value::Array(many)).len(),
        ferny_lsp::MAX_ITEMS
    );
    assert!(parse_symbols(&serde_json::json!(null)).is_empty());
    assert_eq!(symbol_kind_name(12), Some("function"));
    assert_eq!(symbol_kind_name(13), None);
}
