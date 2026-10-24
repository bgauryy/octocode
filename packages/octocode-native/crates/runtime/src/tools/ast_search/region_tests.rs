fn run(
    root: &std::path::Path,
    row: serde_json::Value,
) -> Result<serde_json::Value, crate::response::result::ToolError> {
    super::test_rows::run_window(root, row, 1)
}
use crate::tools::test_support::Fixture;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

#[test]
fn scoped_matches_keep_parent_context_and_page_all_counts_with_the_same_scope() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(
        &file,
        "function chosen() {\n  foo(a);\n  foo(b);\n}\nfoo(c);\n",
    )
    .expect("source");
    let scope = json!({"start":{"line":2,"column":1},"end":{"line":4,"column":1}});
    let query = json!({"path":file,"operation":"match","scope":scope,"includeCaptures":true,"rule":{"pattern":"foo($X)","inside":{"kind":"function_declaration","stopBy":"end"}}});
    let mut result = run(&root.0, query).expect("first page");
    let mut seen = Vec::new();
    let mut next_query = Value::Null;
    for _ in 0..5 {
        for file in result["files"].as_array().expect("files") {
            for matched in file["matches"].as_array().expect("matches") {
                seen.push(matched["line"].as_u64().expect("line"));
            }
        }
        let Some(next) = result["next"]["nextMatchPage"]["input"].as_object() else {
            break;
        };
        next_query = Value::Object(next.clone());
        assert_eq!(next_query["queries"][0]["scope"]["start"], scope["start"]);
        result = run(&root.0, next_query.clone()).expect("next page");
    }
    assert_eq!(seen, [2, 3]);
    assert!(!next_query.is_null());
    next_query["queries"][0]["scope"]["end"]["line"] = json!(5);
    let stale = run(&root.0, next_query).expect("typed stale-snapshot response");
    assert_eq!(stale["status"], "error");
    assert_eq!(stale["errorCode"], "staleSnapshot");
    for view in ["content", "countMatches"] {
        let found = run(&root.0, json!({"path":file,"operation":"match","rule":{"pattern":"foo($X)"},"scope":scope,"view":view})).expect("scoped view");
        assert_eq!(found["matchCount"], 2, "{found}");
    }
}

#[test]
fn overlap_returns_complete_matching_nodes_and_contained_excludes_crossing_nodes() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    let source = "function f() {\n  foo(a);\n}\n";
    std::fs::write(&file, source).expect("source");
    for (mode, count) in [("contained", 0), ("overlap", 1)] {
        let found = run(&root.0, json!({"path":file,"operation":"match","rule":{"kind":"function_declaration"},"includeCaptures":true,"scope":{"start":{"line":2,"column":3},"end":{"line":2,"column":6},"mode":mode}})).expect("scope");
        assert_eq!(found["matchCount"], count, "{found}");
        if count == 1 {
            assert_eq!(found["files"][0]["matches"][0]["value"], source.trim_end());
        }
    }
}

#[test]
fn scope_rejects_directories_out_of_bounds_and_mid_surrogate_positions() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(&file, "\"😀\"; foo(a);\r\n").expect("source");
    for (path, start, end) in [
        (
            root.0.clone(),
            json!({"line":1,"column":1}),
            json!({"line":1,"column":2}),
        ),
        (
            file.clone(),
            json!({"line":1,"column":3}),
            json!({"line":1,"column":13}),
        ),
        (
            file.clone(),
            json!({"line":1,"column":1}),
            json!({"line":20,"column":1}),
        ),
        (
            file.clone(),
            json!({"line":1,"column":1}),
            json!({"line":1,"column":200}),
        ),
    ] {
        let error = run(&root.0, json!({"path":path,"operation":"match","rule":{"pattern":"foo($X)"},"scope":{"start":start,"end":end}})).expect_err("invalid scope");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    let found = run(&root.0, json!({"path":file,"operation":"match","rule":{"pattern":"foo($X)"},"scope":{"start":{"line":1,"column":7},"end":{"line":1,"column":13}}})).expect("UTF-16 region");
    assert_eq!(found["matchCount"], 1);
    assert_eq!(found["files"][0]["matches"][0]["column"], 7);
}

#[test]
fn generated_rule_types_execute_all_ordinal_spellings_and_exact_range() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(&file, "foo(a, b, c);\n").expect("source");
    for position in [json!(2), json!("2n"), json!({"position":2})] {
        let result = run(&root.0, json!({"path":file,"operation":"match","rule":{"kind":"identifier","inside":{"kind":"arguments"},"nthChild":position}})).expect("generated ordinal input");
        assert_eq!(result["matchCount"], 1);
        assert_eq!(result["files"][0]["matches"][0]["value"], "b");
    }
}
