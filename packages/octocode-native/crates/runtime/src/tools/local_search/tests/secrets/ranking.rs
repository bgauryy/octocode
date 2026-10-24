//! Relevance order: deciding hits first, declarations and source files
//! above comments, tests and generated files.

use super::*;

fn ranked_paths(files: &[(&str, &str)], fields: serde_json::Value) -> Vec<String> {
    let body = search_fixture(files, ls_query(fields, None));
    body["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn relevance_ranks_a_declaration_hit_above_a_comment_or_string_hit_at_equal_count() {
    // Path order alone would put the comment and string hits first.
    let files = [
        ("a_comment.rs", "// parse_config reads the file\n"),
        ("b_string.rs", "let label = \"parse_config\";\n"),
        ("c_call.rs", "let cfg = parse_config(path);\n"),
        ("d_decl.rs", "pub fn parse_config(path: &str) -> Config {\n"),
    ];
    for view in ["content", "countMatches", "countLines", "matchOnly"] {
        assert_eq!(
            ranked_paths(
                &files,
                serde_json::json!({"matchString": ["parse_config"], "view": view}),
            ),
            ["d_decl.rs", "c_call.rs", "a_comment.rs", "b_string.rs"],
            "view {view}"
        );
    }
}

#[test]
fn relevance_ranks_a_source_file_above_test_and_generated_files_at_equal_count() {
    let hit = "let cfg = parse_config(path);\n";
    let files = [
        ("a/tests/config.rs", hit),
        ("b/config_test.go", hit),
        ("c/config.test.ts", hit),
        ("d/generated/config.rs", hit),
        ("e/vendor/config.go", hit),
        ("z/src/config.rs", hit),
    ];
    let ranked = ranked_paths(&files, serde_json::json!({"matchString": ["parse_config"]}));
    assert_eq!(ranked[0], "z/src/config.rs", "{ranked:?}");
    // The demoted rest keeps a total order: path breaks the tie, and a
    // generated file follows every hand-written one.
    assert_eq!(
        &ranked[1..],
        [
            "a/tests/config.rs",
            "b/config_test.go",
            "c/config.test.ts",
            "e/vendor/config.go",
            "d/generated/config.rs",
        ]
    );
    // Match count still dominates: two test hits beat one source hit.
    let ranked = ranked_paths(
        &[
            ("a/tests/config.rs", "parse_config();\nparse_config();\n"),
            ("z/src/config.rs", hit),
        ],
        serde_json::json!({"matchString": ["parse_config"]}),
    );
    assert_eq!(ranked, ["a/tests/config.rs", "z/src/config.rs"]);
}

#[test]
fn relevance_on_path_list_views_demotes_test_paths_and_path_sort_does_not() {
    let hit = "parse_config();\n";
    let files = [("a/tests/config.rs", hit), ("z/src/config.rs", hit)];
    let relevance = ranked_paths(
        &files,
        serde_json::json!({"matchString": ["parse_config"], "view": "files"}),
    );
    assert_eq!(relevance, ["z/src/config.rs", "a/tests/config.rs"]);
    let by_path = ranked_paths(
        &files,
        serde_json::json!({"matchString": ["parse_config"], "view": "files", "sort":{"by":"path","order":"asc"}}),
    );
    assert_eq!(by_path, ["a/tests/config.rs", "z/src/config.rs"]);
    let without = ranked_paths(
        &[("a/tests/other.rs", "x\n"), ("z/src/other.rs", "x\n")],
        serde_json::json!({"matchString": ["parse_config"], "view": "filesWithout"}),
    );
    assert_eq!(without, ["z/src/other.rs", "a/tests/other.rs"]);
}
