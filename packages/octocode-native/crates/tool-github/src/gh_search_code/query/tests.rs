use super::*;
use octocode_test_support::json_fixture;
use octocode_tools_shared::gh_shared::query::parse;

/// Rendered qualifiers keep paths as path prefixes and quote what the
/// search syntax would otherwise read as operators or separators.
#[test]
fn rendered_qualifiers_keep_paths_and_quote_reserved_text() {
    // (case, row, present, absent, prefix)
    type Case = (
        &'static str,
        serde_json::Value,
        &'static [&'static str],
        &'static [&'static str],
        &'static str,
    );
    let cases: [Case; 3] = [
        (
            "dotted directory path stays a path prefix",
            json_fixture(r#"{"keywords": ["merge"], "path": "types/lodash.merge"}"#),
            &["path:\"types/lodash.merge\""],
            &["filename:"],
            "",
        ),
        (
            "qualifier values with whitespace are quoted",
            json_fixture(
                r#"{
                        "keywords": ["x"],
                        "path": "my dir/src",
                        "nameContains": "my file.txt"
                    }"#,
            ),
            &["path:\"my dir/src\"", "filename:\"my file.txt\""],
            &[],
            "",
        ),
        (
            "reserved boolean keywords are quoted",
            json_fixture(r#"{"keywords": ["foo", "OR", "bar", "NOT", "and"]}"#),
            &[],
            &[],
            "foo \"OR\" bar \"NOT\" \"and\"",
        ),
    ];
    for (case, mut row, present, absent, prefix) in cases {
        row["owner"] = "o".into();
        let q = code(&parse(row), None, PathAs::Folder);
        for text in present {
            assert!(q.contains(text), "{case}: {q}");
        }
        for text in absent {
            assert!(!q.contains(text), "{case}: {q}");
        }
        assert!(q.starts_with(prefix), "{case}: {q}");
    }
}

/// A file `path` is the full path as `filename:` under its parent
/// directory, the root as `path:/`; GitHub answered both forms on
/// 2026-11-12 (vitejs/vite, rust-lang/rustlings) and neither as `path:`.
#[test]
fn a_file_path_searches_its_name_under_its_parent() {
    let file = |path: &str| {
        code(
            &parse(serde_json::json!({"owner":"o","repo":"r","keywords":["x"],"path":path})),
            None,
            PathAs::File,
        )
    };
    assert_eq!(
        file("packages/vite/src/node/server/index.ts"),
        r#"x filename:"packages/vite/src/node/server/index.ts" path:"packages/vite/src/node/server" repo:o/r in:file"#
    );
    assert_eq!(
        file("Cargo.toml"),
        "x filename:Cargo.toml path:/ repo:o/r in:file"
    );
    let query = parse(json_fixture(r#"{"owner": "o", "path": "src/Main.rs"}"#));
    assert!(PathAs::File.keeps(&query, "src/main.rs"));
    assert!(!PathAs::File.keeps(&query, "x/src/main.rs"));
    assert!(PathAs::Folder.keeps(&query, "x/src/main.rs"));
}

#[test]
fn leading_quote_keyword_cannot_negate_the_repo_scope() {
    // A raw `"hello" NOT` would bind NOT to the repo: qualifier.
    let q = code(
        &parse(json_fixture(
            r#"{
                    "owner": "octocat",
                    "repo": "Hello-World",
                    "keywords": ["\"hello\" NOT", "x\" OR repo:evil/x"]
                }"#,
        )),
        None,
        PathAs::Folder,
    );
    assert_eq!(
        q, r#""\"hello\" NOT" "x\" OR repo:evil/x" repo:octocat/Hello-World in:file"#,
        "{q}"
    );
}

/// A keyword that spells JSON quotes keeps them, escaped, beside the
/// path and extension qualifiers: GitHub answers this form (8
/// advisories for `ws` on 2026-11-06) and none with the quotes dropped.
#[test]
fn a_quoted_json_keyword_keeps_its_quotes_beside_path_and_extension() {
    let q = code(
        &parse(json_fixture(
            r#"{
                    "owner": "github",
                    "repo": "advisory-database",
                    "path": "advisories/github-reviewed",
                    "extensions": ["json"],
                    "keywords": ["\"name\": \"ws\""]
                }"#,
        )),
        None,
        PathAs::Folder,
    );
    assert_eq!(
        q,
        r#""\"name\": \"ws\"" extension:json path:"advisories/github-reviewed" repo:github/advisory-database in:file"#
    );
}

/// The contract rejects a search with nothing to bound it (owner or
/// owner/repo alone, blank keywords); native trusts admission.
#[test]
fn the_contract_rejects_empty_and_unreachable_searches() {
    let admitted = |row: &str| {
        let row: serde_json::Value = serde_json::from_str(row).expect("row");
        octocode_runtime_base::contracts::validate(
            "ghSearchCode",
            serde_json::json!({"queries":[row]}),
        )
        .is_ok()
    };
    for raw in [
        r#"{"owner":"o"}"#,
        r#"{"owner":"o","keywords":[]}"#,
        r#"{"owner":"o","keywords":["   "]}"#,
        r#"{"owner":"o","repo":"r"}"#,
    ] {
        assert!(!admitted(raw), "{raw}");
    }
    for raw in [
        r#"{"owner":"o","keywords":["needle"]}"#,
        r#"{"owner":"o","path":"src"}"#,
        r#"{"owner":"o","extensions":["rs"]}"#,
        r#"{"owner":"o","nameContains":"Cargo.toml"}"#,
    ] {
        assert!(admitted(raw), "{raw}");
    }
}
