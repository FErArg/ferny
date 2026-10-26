//! Git read-only (H4.2d).

use ferny_core::git::{blame_args, parse_blame, parse_status, status_args, toplevel_args};

#[test]
fn args_neutralise_repository_config() {
    for args in [toplevel_args(), status_args(), blame_args("-x.c")] {
        let joined = args.join(" ");
        assert!(
            joined.starts_with("--no-optional-locks -c core.fsmonitor=false -c core.hooksPath=")
        );
        assert!(joined.contains("-c diff.external="));
    }
    let blame = blame_args("--output=/tmp/x");
    let dashdash = blame.iter().position(|a| a == "--").expect("--");
    assert_eq!(blame[dashdash + 1], "--output=/tmp/x");
    assert!(blame.contains(&"--no-textconv".to_owned()));
}

#[test]
fn status_is_parsed() {
    let output = b" M a.txt\0?? new/\0A  b.c\0R  c.c\0old.c\0UU d.c\0 D e.c\0";
    assert_eq!(
        parse_status(output),
        [
            ("a.txt".to_owned(), 'M'),
            ("new/".to_owned(), '?'),
            ("b.c".to_owned(), 'A'),
            ("c.c".to_owned(), 'R'),
            ("d.c".to_owned(), 'U'),
            ("e.c".to_owned(), 'D'),
        ]
    );
}

#[test]
fn blame_is_parsed_and_neutralised() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let output = format!(
        "{sha} 1 1 2\nauthor Ana\u{202E}gnipS\nauthor-mail <a@b.c>\nauthor-time 1800000000\n\
         author-tz +0000\nsummary First commit\nfilename a.c\n\tint a;\n{sha} 2 2\n\tint b;\n"
    );
    let lines = parse_blame(output.as_bytes());
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].line, 2);
    assert_eq!(lines[1].commit, "01234567");
    assert_eq!(lines[0].date, "2027-01-15");
    assert_eq!(lines[0].summary, "First commit");
    assert!(!lines[0].author.contains('\u{202E}'));
}

#[test]
fn parsers_are_bounded() {
    let many: Vec<u8> = (0..30_000)
        .flat_map(|i| format!(" M f{i}\0").into_bytes())
        .collect();
    assert_eq!(parse_status(&many).len(), ferny_core::git::MAX_STATUS);
    assert!(parse_status(b"\0\0garbage").is_empty());
    assert!(parse_blame(b"not a blame\n\tline\n").is_empty());
}
