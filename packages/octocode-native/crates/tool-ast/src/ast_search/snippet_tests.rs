//! Inline source (`content` with `parser`) in place of `path`: the same rows a
//! file with that text gives, without a path or any fact of a walk.
use super::test_rows::*;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::test_support::{Fixture, checks};
use serde_json::{Value, json};

/// A credential-shaped token: what a read of a file redacts.
fn token() -> String {
    format!("ghp_{}", "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8")
}

/// `row` asked of `source` written as the file `a.ts`, and of the same text
/// sent as `content`.
fn both(source: &str, row: Value) -> (Value, Value) {
    let root = Fixture::new();
    let file = root.write("a.ts", source);
    let on = |changes: Value| {
        let mut query = row.clone();
        for (field, value) in changes.as_object().expect("fields") {
            query[field] = value.clone();
        }
        run(&root.0, query)
    };
    let of_file = on(json!({"path": file})).expect("file row");
    let of_code = on(json!({"content": source, "parser": "typescript"})).expect("snippet row");
    (of_file, of_code)
}

/// The facts of a walk and of a file: a snippet row states none.
fn assert_no_walk_facts(out: &Value) {
    for fact in [
        "path",
        "filesScanned",
        "skippedPaths",
        "skippedGrammars",
        "skippedFormats",
        "sourceEvidence",
    ] {
        assert!(out.get(fact).is_none(), "{fact}: {out}");
    }
    for lead in checks::leads(out) {
        assert!(
            lead.query.get("path").is_none(),
            "a snippet row leads to no file: {out}"
        );
    }
}

/// The match rows of a page's one source.
fn matches(out: &Value) -> &Value {
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    &files[0]["matches"]
}

#[test]
fn a_snippet_tree_is_the_tree_of_a_file_with_that_text() {
    for named_only in [true, false] {
        let (of_file, of_code) = both(
            "function f(a: number) {\n  return g(a, 1);\n}\n",
            json!({"operation":"syntaxTree","namedOnly":named_only}),
        );
        assert!(
            of_code["nodes"].as_array().is_some_and(|n| !n.is_empty()),
            "{of_code}"
        );
        assert_eq!(of_code["nodes"], of_file["nodes"]);
        assert_eq!(of_code["pagination"], of_file["pagination"]);
        assert_no_walk_facts(&of_code);
    }
}

#[test]
fn a_snippet_tree_starts_at_its_line_like_a_file_tree() {
    let (of_file, of_code) = both(
        "const a = 1;\nfoo(a);\n",
        json!({"operation":"syntaxTree","line":2}),
    );
    assert_eq!(of_code["nodes"], of_file["nodes"]);
    assert_ne!(of_code["nodes"][0], json!("0 program 1:1-3:1"), "{of_code}");
}

#[test]
fn a_snippet_match_is_the_match_of_a_file_with_that_text() {
    let source = "foo(1); bar(2)\nfoo(\n  3,\n);\n";
    for captures in [false, true] {
        let (of_file, of_code) = both(
            source,
            json!({"operation":"match","rule":{"pattern":"foo($A)"},"includeCaptures":captures}),
        );
        assert_eq!(matches(&of_code).as_array().map(Vec::len), Some(2));
        assert_eq!(matches(&of_code), matches(&of_file));
        assert_eq!(of_code["matchCount"], of_file["matchCount"]);
        // The one group names no file: its source is the request's `content`.
        let groups = of_code["files"].as_array().expect("files");
        assert_eq!(groups.len(), 1, "{of_code}");
        assert!(groups[0].get("path").is_none(), "{of_code}");
        assert_no_walk_facts(&of_code);
    }
    for view in ["files", "countMatches"] {
        let (of_file, of_code) = both(
            source,
            json!({"operation":"match","rule":{"pattern":"foo($A)"},"view":view}),
        );
        assert_eq!(of_code["matchCount"], of_file["matchCount"], "{of_code}");
        // No row names the snippet as a file.
        for group in of_code["files"].as_array().into_iter().flatten() {
            assert!(
                group.is_object() && group.get("path").is_none(),
                "{of_code}"
            );
        }
        assert_no_walk_facts(&of_code);
    }
}

#[test]
fn a_snippet_match_keeps_to_its_scope_like_a_file_match() {
    let (of_file, of_code) = both(
        "foo(1);\nfoo(2);\nfoo(3);\n",
        json!({
            "operation":"match",
            "rule":{"pattern":"foo($A)"},
            "scope":{"start":{"line":2,"column":1},"end":{"line":3,"column":1}}
        }),
    );
    assert_eq!(matches(&of_code).as_array().map(Vec::len), Some(1));
    assert_eq!(matches(&of_code), matches(&of_file));
}

/// A rule that fits nothing is explained on a snippet as on a file: the same
/// diagnostics and hints, with no file to lead to.
#[test]
fn an_empty_snippet_match_explains_itself_like_a_file_match() {
    let (of_file, of_code) = both(
        "foo(1); bar(2)\n",
        json!({"operation":"match","rule":{"pattern":"zzz($A)"}}),
    );
    assert_eq!(of_code["status"], "empty", "{of_code}");
    assert_eq!(of_code["files"], json!([]));
    let messages = |out: &Value| {
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|diagnostic| diagnostic["message"].clone())
            .collect::<Vec<_>>()
    };
    assert!(!messages(&of_code).is_empty(), "{of_code}");
    assert_eq!(messages(&of_code), messages(&of_file));
    assert_eq!(of_code["hints"], of_file["hints"]);
    assert_no_walk_facts(&of_code);
}

#[test]
fn an_invalid_rule_fails_on_a_snippet_as_on_a_file() {
    let root = Fixture::new();
    let source = "foo(1)\n";
    let file = root.write("a.ts", source);
    let rule = json!({"pattern":"foo($A"});
    let of_file = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":rule}),
    )
    .expect_err("file");
    let of_code = run(
        &root.0,
        json!({"operation":"match","content":source,"parser":"typescript","rule":rule}),
    )
    .expect_err("snippet");
    assert_eq!(of_code.code, ErrorCode::InvalidPattern);
    assert_eq!(of_code.code, of_file.code);
    assert_eq!(of_code.message, of_file.message);
    assert!(of_code.calls.is_none(), "a snippet has no file to lead to");
}

/// What a read of a file hides, a snippet's rows hide the same way, and
/// every position counts the shown text.
#[test]
fn a_secret_in_a_snippet_is_shown_as_a_read_of_a_file_shows_it() {
    let token = token();
    let source = format!("const key = \"{token}\";\nuse(key, \"{token}\");\n");
    let (tree_of_file, tree_of_code) = both(&source, json!({"operation":"syntaxTree"}));
    assert_eq!(tree_of_code["nodes"], tree_of_file["nodes"]);
    for rule in [
        json!({"pattern":"const key = $V"}),
        json!({"pattern":"use($$$ARGS)"}),
        json!({"kind":"string"}),
    ] {
        for captures in [false, true] {
            let (of_file, of_code) = both(
                &source,
                json!({"operation":"match","rule":rule,"includeCaptures":captures}),
            );
            assert!(
                matches(&of_code)
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty()),
                "{of_code}"
            );
            assert_eq!(matches(&of_code), matches(&of_file));
            assert!(!of_code.to_string().contains(&token), "{of_code}");
        }
    }
}

/// A snippet over one page is read through its pages like a file: each row
/// once, the same rows, the snippet carried by each page's own call.
#[test]
fn a_paged_snippet_lists_the_rows_of_the_paged_file() {
    let source = (0..120)
        .map(|index| format!("call{index}(alpha, beta);\n"))
        .collect::<String>();
    let root = Fixture::new();
    let file = root.write("a.ts", &source);
    let walk = |row: Value, rows_of: fn(&Value) -> Vec<String>| {
        let mut pages = Vec::new();
        checks::walker::walk("astSearch", json!({"queries":[row]}), |call| {
            let out = run_window(&root.0, call.clone(), 2_000).expect("page");
            let walked = checks::Page {
                output: octocode_runtime_base::test_support::published(ToolId::AstSearch, &out),
                rows: rows_of(&out),
            };
            pages.push(out);
            walked
        });
        pages.iter().flat_map(rows_of).collect::<Vec<_>>()
    };
    let nodes = |out: &Value| {
        out["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(Value::to_string)
            .collect::<Vec<_>>()
    };
    let match_rows = |out: &Value| {
        out["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
    };
    let snippet = json!({"content": source, "parser": "typescript"});
    let on = |target: &Value, mut row: Value| {
        for (field, value) in target.as_object().expect("fields") {
            row[field] = value.clone();
        }
        row
    };
    let tree = json!({"operation":"syntaxTree"});
    let of_file = walk(on(&json!({"path": file}), tree.clone()), nodes);
    let of_code = walk(on(&snippet, tree), nodes);
    assert!(of_code.len() > 500, "{}", of_code.len());
    assert_eq!(of_code, of_file);
    let rule = json!({"operation":"match","rule":{"pattern":"$F(alpha, beta)"}});
    let of_file = walk(on(&json!({"path": file}), rule.clone()), match_rows);
    let of_code = walk(on(&snippet, rule), match_rows);
    assert_eq!(of_code.len(), 120);
    assert_eq!(of_code, of_file);
}

/// Provenance states the bytes of a file on disk: the contract refuses a
/// snippet row asking for it, with what to send instead, before the row runs.
#[test]
fn a_snippet_with_provenance_is_refused_naming_the_fix() {
    for row in [
        json!({"operation":"syntaxTree"}),
        json!({"operation":"match","rule":{"pattern":"foo($A)"}}),
    ] {
        let mut row = row;
        row["content"] = json!("foo(1)");
        row["parser"] = json!("typescript");
        let validate = |row: &Value| {
            octocode_runtime_base::contracts::validate_built_query(
                ToolId::AstSearch.as_str(),
                row.clone(),
            )
        };
        validate(&row).expect("the snippet row alone is valid");
        row["includeProvenance"] = json!(true);
        let refusal = validate(&row).expect_err("refused").to_string();
        assert!(refusal.contains("drop includeProvenance"), "{refusal}");
    }
}

/// A file named like the snippet's group is never read for its rows.
#[test]
fn a_snippet_never_reads_a_file_named_like_its_group() {
    let root = Fixture::new();
    root.write(super::source::INLINE_NAME, "other(9); other(8)\n");
    let (paths, security) = octocode_runtime_base::test_support::simple_policy(&root.0);
    let _in_root = &paths;
    let out = execute_row(
        json!({
            "operation":"match",
            "content":"foo(1)",
            "parser":"typescript",
            "rule":{"pattern":"foo($A)"}
        }),
        &paths,
        &security,
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .expect("row");
    assert_eq!(matches(&out)[0]["value"], "foo(1)", "{out}");
}
