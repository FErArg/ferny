//! Symbols with tree-sitter (H2.2, threats T6, T14).

use ferny_syntax::symbols::{MAX_SOURCE_BYTES, SymbolError, extract, supported};

fn names(filetype: &str, source: &str) -> Vec<(String, String, u32)> {
    extract(filetype, source)
        .expect("symbols")
        .into_iter()
        .map(|s| (s.kind, s.name, s.line))
        .collect()
}

fn own(v: &[(&str, &str, u32)]) -> Vec<(String, String, u32)> {
    v.iter()
        .map(|(k, n, l)| ((*k).to_owned(), (*n).to_owned(), *l))
        .collect()
}

#[test]
fn five_languages() {
    for ft in ["c", "cpp", "rust", "python", "javascript"] {
        assert!(supported(ft), "{ft}");
    }
    assert!(!supported("make"));
    assert_eq!(extract("make", "all:"), Err(SymbolError::Unsupported));

    assert_eq!(
        names(
            "c",
            "struct point { int x; };\nint main(void) {\n  return 0;\n}\n"
        ),
        own(&[("class", "point", 1), ("function", "main", 2)])
    );
    let rust = "struct S;\nimpl S {\n    fn new() -> Self { S }\n}\nfn main() {}\n";
    assert_eq!(
        names("rust", rust),
        own(&[
            ("class", "S", 1),
            ("method", "new", 3),
            ("function", "main", 5)
        ])
    );
    assert_eq!(
        names(
            "python",
            "class A:\n    def m(self):\n        pass\n\ndef f():\n    pass\n"
        ),
        own(&[
            ("class", "A", 1),
            ("function", "m", 2),
            ("function", "f", 5)
        ])
    );
    let js = names("javascript", "function f() {}\nclass K { m() {} }\n");
    assert!(js.contains(&("function".into(), "f".into(), 1)), "{js:?}");
    assert!(js.contains(&("class".into(), "K".into(), 2)), "{js:?}");
    let cpp = names(
        "cpp",
        "namespace n {\nclass C {};\nint g() { return 1; }\n}\n",
    );
    assert!(cpp.iter().any(|(_, n, l)| n == "C" && *l == 2), "{cpp:?}");
    assert!(cpp.iter().any(|(_, n, l)| n == "g" && *l == 3), "{cpp:?}");
}

#[test]
fn broken_and_hostile_input_is_bounded() {
    // Invalid code still yields what can be recovered, never panics.
    assert!(extract("c", "int f( { ;;; }}}} \u{202E} \0").is_ok());
    // Pathological nesting: either parsed or cut at the time budget, never
    // much later.
    let start = std::time::Instant::now();
    let result = extract("python", &"(".repeat(100_000));
    assert!(
        matches!(result, Ok(_) | Err(SymbolError::Timeout)),
        "{result:?}"
    );
    assert!(start.elapsed() < ferny_syntax::symbols::TIME_BUDGET * 3);
    let big = "x".repeat(MAX_SOURCE_BYTES + 1);
    assert_eq!(extract("rust", &big), Err(SymbolError::TooLarge));
}

#[test]
fn many_definitions_are_capped() {
    let source: String = (0..12_000).map(|i| format!("fn f{i}() {{}}\n")).collect();
    let symbols = extract("rust", &source).expect("symbols");
    assert_eq!(symbols.len(), ferny_syntax::symbols::MAX_SYMBOLS);
}
