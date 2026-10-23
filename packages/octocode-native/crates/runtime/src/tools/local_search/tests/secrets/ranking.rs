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
fn a_clipped_file_shows_its_deciding_hits_first_and_lists_the_rest() {
    // Comment and call hits come first by line; the assignment and the
    // branch that decide behaviour sit past the per-file cap.
    let mut file = String::new();
    for _ in 0..4 {
        file.push_str(" * maximumSize doc\n");
    }
    for _ in 0..3 {
        file.push_str("check(maximumSize);\n");
    }
    file.push_str("this.maximumSize = maximumSize;\n"); // line 8
    file.push_str("if (maximumSize > 0) {\n"); // line 9
    let request = |match_page| {
        ls_query(
            serde_json::json!({"matchString": "maximumSize", "matchPageSize": 3, "matchPage": match_page, "contextLines": 0}),
            None,
        )
    };
    let lines = |body: &serde_json::Value| -> Vec<u64> {
        body["files"][0]["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .filter_map(|m| m["line"].as_u64())
            .collect()
    };
    let first = search_fixture(&[("CacheBuilder.java", &file)], request(1));
    // Page 1 holds the best-ranked rows, shown in source order.
    assert_eq!(lines(&first), [5, 8, 9], "{first}");
    // The rows still unseen are named, cheap to read at their lines.
    assert_eq!(
        first["files"][0]["pagination"]["moreLines"],
        serde_json::json!([1, 2, 3, 4, 6, 7]),
        "{first}"
    );
    let second = search_fixture(&[("CacheBuilder.java", &file)], request(2));
    assert_eq!(lines(&second), [1, 6, 7], "{second}");
    assert_eq!(
        second["files"][0]["pagination"]["moreLines"],
        serde_json::json!([2, 3, 4])
    );
    let last = search_fixture(&[("CacheBuilder.java", &file)], request(3));
    assert_eq!(lines(&last), [2, 3, 4], "{last}");
    assert!(last["files"][0].get("pagination").is_none(), "{last}");
}

#[test]
fn reverse_applies_to_default_relevance_order() {
    let files = [("a.txt", "foo\nfoo\nfoo\n"), ("b.txt", "foo\n")];
    let order = |reverse| {
        let body = search_fixture(
            &files,
            ls_query(
                serde_json::json!({"matchString": "foo".to_string(), "reverse": reverse}),
                None,
            ),
        );
        body["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(None), ["a.txt", "b.txt"]);
    assert_eq!(order(Some(true)), ["b.txt", "a.txt"]);
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
    for view in ["paginated", "countMatches", "countLines", "matchOnly"] {
        assert_eq!(
            ranked_paths(
                &files,
                serde_json::json!({"matchString": "parse_config", "resultView": view}),
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
    let ranked = ranked_paths(&files, serde_json::json!({"matchString": "parse_config"}));
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
        serde_json::json!({"matchString": "parse_config"}),
    );
    assert_eq!(ranked, ["a/tests/config.rs", "z/src/config.rs"]);
}

#[test]
fn relevance_on_path_list_views_demotes_test_paths_and_path_sort_does_not() {
    let hit = "parse_config();\n";
    let files = [("a/tests/config.rs", hit), ("z/src/config.rs", hit)];
    let relevance = ranked_paths(
        &files,
        serde_json::json!({"matchString": "parse_config", "resultView": "files"}),
    );
    assert_eq!(relevance, ["z/src/config.rs", "a/tests/config.rs"]);
    let by_path = ranked_paths(
        &files,
        serde_json::json!({"matchString": "parse_config", "resultView": "files", "sort": "path"}),
    );
    assert_eq!(by_path, ["a/tests/config.rs", "z/src/config.rs"]);
    let without = ranked_paths(
        &[("a/tests/other.rs", "x\n"), ("z/src/other.rs", "x\n")],
        serde_json::json!({"matchString": "parse_config", "resultView": "filesWithout"}),
    );
    assert_eq!(without, ["z/src/other.rs", "a/tests/other.rs"]);
}
