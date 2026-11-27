use super::*;
use octocode_test_support::json_fixture;

#[test]
fn rows_keep_line_order_with_numeric_lines() {
    let keys = vec![(12, "a.b".to_owned()), (100, "c".to_owned())];
    assert_eq!(
        serde_json::to_value(MatchKeyRows(&keys)).expect("serializable"),
        json_fixture(
            r#"[
                {"line": 12, "keyPath": "a.b"},
                {"line": 100, "keyPath": "c"}
            ]"#
        )
    );
}

#[test]
fn only_json_and_toml_extensions_count() {
    for (path, keyed) in [
        ("package.json", true),
        ("tsconfig.JSONC", true),
        ("a.json5", true),
        ("Cargo.toml", true),
        ("pyproject.TOML", true),
        ("a.jsonl", false),
        ("a.js", false),
        ("Cargo.lock", false),
    ] {
        assert_eq!(is_keyed_path(Path::new(path)), keyed, "{path}");
    }
}

#[test]
fn other_files_have_no_key_paths() {
    assert!(line_key_paths(Path::new("a.rs"), "a = 1\n", &[1]).is_empty());
    assert_eq!(
        line_key_paths(Path::new("a.toml"), "[t]\na = 1\n", &[2]),
        vec![(2, "t.a".to_owned())]
    );
}
