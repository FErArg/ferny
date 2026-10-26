//! Command-line locations, single-instance requests and the session file
//! (threats T8 and T10).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::TempDir;
use ferny_core::document::Documents;
#[cfg(unix)]
use ferny_core::ipc::EndpointState;
use ferny_core::ipc::{self, IpcError, MAX_REQUEST_BYTES, MAX_REQUEST_FILES};
use ferny_core::location::{Location, parse_arguments};
use ferny_core::session::{self, MAX_RECENT_FILES, RecentList, Session, SessionError};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn absolute(name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:\\work\\{name}"))
    } else {
        PathBuf::from(format!("/work/{name}"))
    }
}

#[test]
fn cli_location_forms() {
    let dir = TempDir::new("cli-forms");
    let cwd = &dir.0;
    let parsed = parse_arguments(
        &args(&["a.c", "b.c:12", "c.c:3:7", "+40", "d.c", "e.c:0", "f.c:x"]),
        cwd,
    );
    let expect = |name: &str, line, column| Location {
        path: cwd.join(name),
        line,
        column,
    };
    assert_eq!(
        parsed,
        [
            expect("a.c", None, None),
            expect("b.c", Some(12), None),
            expect("c.c", Some(3), Some(7)),
            expect("d.c", Some(40), None),
            expect("e.c:0", None, None),
            expect("f.c:x", None, None),
        ]
    );
}

#[test]
fn cli_location_prefers_existing_literal_name() {
    let dir = TempDir::new("cli-literal");
    dir.write("odd:12", b"x");
    let parsed = parse_arguments(&args(&["odd:12"]), &dir.0);
    assert_eq!(parsed, [Location::new(dir.0.join("odd:12"))]);
}

#[test]
fn cli_location_rejects_out_of_range_numbers() {
    let dir = TempDir::new("cli-range");
    let parsed = parse_arguments(&args(&["a.c:99999999999", "+99999999999", "b.c"]), &dir.0);
    assert_eq!(parsed[0], Location::new(dir.0.join("a.c:99999999999")));
    assert_eq!(parsed[1], Location::new(dir.0.join("+99999999999")));
}

#[test]
fn ipc_request_round_trip() {
    let locations = vec![
        Location {
            path: absolute("a b.rs"),
            line: Some(3),
            column: Some(9),
        },
        Location::new(absolute("ñandú.txt")),
    ];
    let bytes = ipc::encode_request(&locations).expect("encode");
    assert_eq!(ipc::decode_request(&bytes), Ok(locations));
    assert_eq!(
        ipc::decode_request(&ipc::encode_request(&[]).expect("empty")),
        Ok(vec![])
    );
}

#[test]
fn ipc_request_round_trip_normalizes_leading_zero_positions() {
    let path = absolute("leading-zero.txt");
    let request = format!("ferny-open 1\n0\t02\t{}\n", path.display());
    let decoded = ipc::decode_request(request.as_bytes()).expect("valid request");
    assert_eq!(
        decoded,
        [Location {
            path: path.clone(),
            line: None,
            column: Some(2),
        }]
    );

    let encoded = ipc::encode_request(&decoded).expect("encode decoded request");
    assert_eq!(
        encoded,
        format!("ferny-open 1\n0\t2\t{}\n", path.display()).as_bytes()
    );
    assert_eq!(ipc::decode_request(&encoded), Ok(decoded));
}

#[test]
fn rejects_oversized_request() {
    let mut bytes = b"ferny-open 1\n".to_vec();
    bytes.resize(MAX_REQUEST_BYTES + 1, b'a');
    assert_eq!(ipc::decode_request(&bytes), Err(IpcError::TooLarge));
}

#[test]
fn rejects_too_many_files() {
    let many: Vec<Location> = (0..=MAX_REQUEST_FILES)
        .map(|i| Location::new(absolute(&format!("{i}.txt"))))
        .collect();
    assert_eq!(ipc::encode_request(&many), Err(IpcError::TooManyFiles));
    let mut text = String::from("ferny-open 1\n");
    for location in &many {
        text.push_str(&format!("0\t0\t{}\n", location.path.display()));
    }
    assert_eq!(
        ipc::decode_request(text.as_bytes()),
        Err(IpcError::TooManyFiles)
    );
}

#[test]
fn rejects_unknown_kind() {
    for request in [
        "ferny-open 2\n",
        "ferny-exec 1\n0\t0\t/bin/sh\n",
        "",
        "ferny-open 1",
        "\u{feff}ferny-open 1\n",
    ] {
        assert!(
            matches!(
                ipc::decode_request(request.as_bytes()),
                Err(IpcError::UnknownRequest | IpcError::Malformed)
            ),
            "{request:?}"
        );
    }
}

#[test]
fn rejects_relative_path() {
    assert_eq!(
        ipc::decode_request(b"ferny-open 1\n0\t0\trelative/a.txt\n"),
        Err(IpcError::InvalidPath)
    );
    assert_eq!(
        ipc::encode_request(&[Location::new("relative.txt")]),
        Err(IpcError::InvalidPath)
    );
}

#[test]
fn rejects_nul_and_control() {
    let abs = absolute("a.txt");
    let base = abs.to_str().expect("utf8");
    for bad in [
        format!("{base}\u{0}x"),
        format!("{base}\u{1b}[2J"),
        format!("{base}\u{7}"),
    ] {
        let request = format!("ferny-open 1\n0\t0\t{bad}\n");
        assert_eq!(
            ipc::decode_request(request.as_bytes()),
            Err(IpcError::InvalidPath)
        );
    }
    for bad in [
        "x\t0\t/a",
        "1\t\t/a",
        "1\t2",
        "-1\t0\t/a",
        "99999999999\t0\t/a",
        "1 \t0\t/a",
    ] {
        let request = format!("ferny-open 1\n{bad}\n");
        assert!(ipc::decode_request(request.as_bytes()).is_err(), "{bad:?}");
    }
    assert_eq!(
        ipc::decode_request(b"ferny-open 1\n\xff\n"),
        Err(IpcError::Malformed)
    );
}

#[test]
fn socket_path_length_is_checked() {
    assert!(ipc::socket_path_fits(Path::new(
        "/run/user/1000/ferny/ferny.sock"
    )));
    let long = format!("/{}/ferny.sock", "d".repeat(100));
    assert!(!ipc::socket_path_fits(Path::new(&long)));
}

#[test]
fn errors_carry_no_path() {
    let err = ipc::decode_request(b"ferny-open 1\n0\t0\tsecret-name.txt\n").expect_err("relative");
    assert!(!err.to_string().contains("secret"));
}

#[cfg(unix)]
mod unix_endpoint {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::os::unix::net::UnixListener;

    /// Socket paths must be short (about 104 bytes on macOS), so these
    /// tests use a directory directly under /tmp, created exclusively
    /// (`create_dir` fails if anything exists there, symlinks included)
    /// and private, removed on drop.
    struct ShortDir(PathBuf);

    impl ShortDir {
        fn new(name: &str) -> Self {
            let dir = PathBuf::from(format!("/tmp/fy-{}-{name}", std::process::id()));
            fs::create_dir(&dir).expect("create short dir");
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("chmod");
            Self(dir)
        }
    }

    impl Drop for ShortDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn socket_dir_is_created_private() {
        let base = TempDir::new("ipc-dir");
        let dir = base.0.join("ferny");
        ipc::prepare_directory(&dir).expect("prepare");
        assert_eq!(fs::metadata(&dir).expect("meta").mode() & 0o777, 0o700);
        // An existing directory open to others is tightened.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod");
        ipc::prepare_directory(&dir).expect("prepare again");
        assert_eq!(fs::metadata(&dir).expect("meta").mode() & 0o777, 0o700);
    }

    #[test]
    fn socket_dir_symlink_is_refused() {
        let base = TempDir::new("ipc-dir-link");
        let real = base.0.join("real");
        fs::create_dir(&real).expect("mkdir");
        let link = base.0.join("link");
        symlink(&real, &link).expect("symlink");
        assert_eq!(
            ipc::prepare_directory(&link),
            Err(IpcError::UnsafeDirectory)
        );
    }

    #[test]
    fn stale_socket_is_reported_as_ours() {
        let base = ShortDir::new("stale");
        let socket = base.0.join("ferny.sock");
        assert_eq!(ipc::endpoint_state(&socket), EndpointState::Absent);
        drop(UnixListener::bind(&socket).expect("bind"));
        // The listener is gone but the socket file stays: stale, but ours.
        assert_eq!(ipc::endpoint_state(&socket), EndpointState::Ours);
    }

    #[test]
    fn foreign_or_unexpected_socket_is_refused() {
        let base = ShortDir::new("foreign");
        let regular = base.0.join("ferny.sock");
        fs::write(&regular, b"not a socket").expect("write");
        assert_eq!(ipc::endpoint_state(&regular), EndpointState::Foreign);
        let elsewhere = ShortDir::new("target");
        let target = elsewhere.0.join("real.sock");
        drop(UnixListener::bind(&target).expect("bind"));
        let link = base.0.join("link.sock");
        symlink(&target, &link).expect("symlink");
        assert_eq!(ipc::endpoint_state(&link), EndpointState::Foreign);
    }
}

#[test]
fn session_round_trip() {
    let dir = TempDir::new("session-rt");
    let path = dir.0.join("config").join("session.toml");
    let mut recent = RecentList::new(MAX_RECENT_FILES);
    recent.push(absolute("old.txt"));
    recent.push(absolute("new.txt"));
    let session = Session {
        files: vec![
            Location {
                path: absolute("a.rs"),
                line: Some(10),
                column: Some(4),
            },
            Location::new(absolute("b.rs")),
        ],
        active: 1,
        geometry: vec![1, 2, 255],
        layout: vec![0, 9],
        recent,
        scribble: "notes \"quoted\"\n\ttab \u{202E} ñ\u{0}".to_owned(),
        project: Some(absolute("project")),
    };
    session::save(&path, &session).expect("save");
    assert_eq!(session::load(&path), Ok(session));
}

#[test]
fn scribble_is_bounded() {
    use ferny_core::session::{MAX_SCRIBBLE_BYTES, truncate_scribble};
    // A multi-byte character straddling the limit is dropped whole.
    let text = format!("{}ñ", "a".repeat(MAX_SCRIBBLE_BYTES - 1));
    let cut = truncate_scribble(text);
    assert_eq!(cut.len(), MAX_SCRIBBLE_BYTES - 1);
    // Saved and loaded at the limit, even when every character is escaped.
    let dir = TempDir::new("session-scribble");
    let path = dir.0.join("session.toml");
    let session = Session {
        scribble: "\u{1}".repeat(MAX_SCRIBBLE_BYTES + 10),
        ..Session::default()
    };
    session::save(&path, &session).expect("save");
    let loaded = session::load(&path).expect("load");
    assert_eq!(loaded.scribble.len(), MAX_SCRIBBLE_BYTES);
}

#[cfg(unix)]
#[test]
fn session_file_is_owner_only() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new("session-mode");
    let path = dir.0.join("cfg").join("session.toml");
    session::save(&path, &Session::default()).expect("save");
    assert_eq!(fs::metadata(&path).expect("meta").mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().expect("parent"))
            .expect("meta")
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn session_missing_and_invalid() {
    let dir = TempDir::new("session-invalid");
    assert_eq!(
        session::load(&dir.0.join("none.toml")),
        Err(SessionError::NotFound)
    );
    for (name, text) in [
        ("garbage.toml", "this is = = not toml"),
        ("version.toml", "version = 2\n"),
        ("types.toml", "version = 1\nfiles = \"nope\"\n"),
    ] {
        let path = dir.write(name, text.as_bytes());
        assert_eq!(session::load(&path), Err(SessionError::Invalid), "{name}");
    }
    let huge = dir.write("huge.toml", &vec![b'#'; 1024 * 1024 + 1]);
    assert_eq!(session::load(&huge), Err(SessionError::Invalid));
}

#[test]
fn session_drops_invalid_entries_and_clamps() {
    let dir = TempDir::new("session-sanitise");
    let abs = absolute("ok.txt");
    let mut text =
        String::from("version = 1\nactive = 999\ngeometry = \"zz\"\nunknown_key = true\n");
    text.push_str(&format!(
        "[[files]]\npath = {:?}\nline = 5\n",
        abs.to_str().expect("utf8")
    ));
    text.push_str("[[files]]\npath = \"relative.txt\"\n");
    text.push_str("[[files]]\npath = \"/bad\\u0000name\"\n");
    text.push_str("recent = []\n");
    let path = dir.write("session.toml", text.as_bytes());
    let session = session::load(&path).expect("load");
    assert_eq!(session.files.len(), 1);
    assert_eq!(session.files[0].line, Some(5));
    assert_eq!(session.active, 0);
    assert!(session.geometry.is_empty());

    let mut many = String::from("version = 1\n");
    for i in 0..300 {
        many.push_str(&format!(
            "[[files]]\npath = {:?}\n",
            absolute(&format!("{i}")).to_str().expect("utf8")
        ));
    }
    let path = dir.write("many.toml", many.as_bytes());
    assert_eq!(
        session::load(&path).expect("load").files.len(),
        session::MAX_SESSION_FILES
    );
}

#[test]
fn recent_list_is_bounded_and_deduplicated() {
    let mut list = RecentList::new(3);
    for name in ["a", "b", "c", "a", "d"] {
        list.push(name.to_owned());
    }
    let items: Vec<&str> = list.iter().map(String::as_str).collect();
    assert_eq!(items, ["d", "a", "c"]);
    assert_eq!(list.pop().as_deref(), Some("d"));
    assert_eq!(list.len(), 2);
}

#[test]
fn find_open_document_by_canonical_path() {
    let dir = TempDir::new("find-doc");
    let path = dir.write("a.txt", b"x");
    fs::create_dir(dir.0.join("sub")).expect("mkdir");
    let mut docs = Documents::new();
    let (id, _) = docs.open(&path, None).expect("open");
    let roundabout = dir.0.join("sub").join("..").join("a.txt");
    assert_eq!(docs.find_by_path(&roundabout), Some(id));
    assert_eq!(docs.find_by_path(Path::new(&dir.0.join("other.txt"))), None);
}
