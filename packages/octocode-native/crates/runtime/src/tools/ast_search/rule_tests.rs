//! astSearch patterns and rules: compile errors, exact patterns, rule
//! spellings, and syntax-tree error rows.
use super::test_rows::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, checks, simple_policy};
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn structural_zero_is_empty_with_actionable_pattern_guidance() {
    let root = Fixture::new();
    let source = root.0.join("source.ts");
    std::fs::write(&source, "const answer = 42;\n").expect("source");
    let (paths, security) = simple_policy(&root.0);

    let missing = execute_row(
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"let $A = $B"}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("structural zero");
    assert_eq!(missing["status"], "empty", "{missing}");
    assert!(
        !missing["isPartial"].as_bool().unwrap_or(false),
        "{missing}"
    );
    // The coverage is stated by fields; the node shape is the viewSyntax lead.
    assert!(missing.get("hints").is_none(), "{missing}");
    assert_eq!(
        missing["next"]["viewSyntax"]["query"]["queries"][0]["operation"], "syntaxTree",
        "{missing}"
    );

    // Trailing punctuation the pattern omits is not required (ast-grep
    // smart strictness).
    for pattern in ["const $A = $B;", "const $A = $B"] {
        let found = execute_row(
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":pattern}),
            &paths,
            &security,
            &NeverCancel,
        )
        .expect("structural match");
        assert_eq!(found["matchCount"], 1, "{found}");
        assert!(found.get("status").is_none(), "{found}");
    }
}

#[test]
fn an_unparseable_pattern_reports_untagged_text() {
    let root = Fixture::new();
    let source = root.0.join("a.rs");
    std::fs::write(&source, "fn a() {}\n").expect("a");
    for path in [source.clone(), root.0.clone()] {
        let error = run(
            &root.0,
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":path,"language":"rust","pattern":"neu("}),
        )
        .expect_err("invalid pattern");
        assert_eq!(error.code, ErrorCode::InvalidPattern);
        assert!(!error.message.starts_with('['), "{}", error.message);
    }
}

/// A syntax tree of a file with no registered grammar is a failed row
/// (`status: "error"`, a declared code, the engine's message), not a partial
/// payload that exits 0.
#[test]
fn syntax_tree_of_an_unsupported_file_is_an_error_row() {
    let root = Fixture::new();
    let file = root.0.join("data.json");
    std::fs::write(&file, "{\"a\":1}\n").expect("file");
    let out = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("row");
    assert_eq!(out["status"], "error", "{out}");
    assert_eq!(out["errorCode"], "languageUnsupported", "{out}");
    assert!(
        out["error"]
            .as_str()
            .is_some_and(|message| message.contains(".json")),
        "{out}"
    );
    assert!(out.get("isPartial").is_none(), "{out}");
}

/// `line` roots the tree at the smallest node covering that line: the
/// rows start at the statement, not at the file's program node.
#[test]
fn syntax_tree_starts_at_the_node_covering_a_line() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(&file, "const a = 1;\nfunction f() {\n  return a;\n}\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"syntaxTree","path":file,"line":3}),
    )
    .expect("row");
    let first = out["nodes"][0].as_str().expect("node row");
    assert!(first.contains("return_statement 3:"), "{out}");
    assert!(!out.to_string().contains("program"), "{out}");
}

/// A `line` past the last line is invalid input that names the line
/// count, with the whole-file tree as its repair; never a failed execution
/// that suggests broadening.
#[test]
fn syntax_tree_line_past_the_end_is_invalid_input_with_a_repair() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(&file, "const a = 1;\nfunction f() {\n  return a;\n}\n").expect("file");
    let error = run(
        &root.0,
        json!({"operation":"syntaxTree","path":file,"line":99}),
    )
    .expect_err("past the end");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert!(error.message.contains("(4 lines)"), "{error:?}");
    let repair = &error.next.as_deref().expect("repair")["repair"];
    let row = &repair["query"]["queries"][0];
    assert_eq!(row["operation"], "syntaxTree", "{repair}");
    assert!(row.get("line").is_none(), "{repair}");
    crate::contracts::validate_query("astSearch", row.clone()).expect("a valid repair row");
}

/// A blank `line` roots the tree at the next item and says so once, on the
/// first page only.
#[test]
fn syntax_tree_blank_line_snaps_to_the_next_item_and_notes_it_once() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(
        &file,
        "const a = 1;\n\nfunction f() {\n  const b = a;\n  const c = b;\n  return c;\n}\n",
    )
    .expect("file");
    let first = run(
        &root.0,
        json!({"operation":"syntaxTree","path":file,"line":2,"pageSize":3}),
    )
    .expect("row");
    let node = first["nodes"][0].as_str().expect("node row");
    assert!(node.contains("function_declaration 3:"), "{first}");
    assert_eq!(
        first["diagnostics"][0]["code"], "syntaxTree.options.lineBlank",
        "{first}"
    );
    let mut next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    assert_eq!(next["line"], 2, "{first}");
    next["mainGoal"] = json!("test");
    let second = run(&root.0, next).expect("page 2");
    assert!(
        second["nodes"].as_array().is_some_and(|n| !n.is_empty()),
        "{second}"
    );
    assert!(second.get("diagnostics").is_none(), "{second}");
}

/// A failed inspection states its error once: the row carries no empty
/// page, no snapshot, no single-page block and no diagnostics that repeat
/// `error`, and no recovery text about paging.
#[test]
fn syntax_tree_error_row_states_the_error_once() {
    let root = Fixture::new();
    let file = root.0.join("data.json");
    std::fs::write(&file, "{\"a\":1}\n").expect("file");
    let out = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("row");
    let mut keys: Vec<&str> = out
        .as_object()
        .expect("row")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["error", "errorCode", "operation", "path", "status"],
        "{out}"
    );
    checks::assert_absent(&out, "next page");
}

#[test]
fn directory_pattern_that_compiles_in_no_file_is_an_error_not_empty() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs"] {
        std::fs::write(root.0.join(name), "fn main() {}\n").expect("file");
    }
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"fn ???"}),
    )
    .expect_err("uncompilable pattern must fail loudly");
    assert_eq!(error.code, ErrorCode::InvalidPattern);
}

#[test]
fn patterns_without_metavariables_offer_no_capture_expansion() {
    let root = Fixture::new();
    let source = root.0.join("calls.ts");
    std::fs::write(&source, "oldCall(one);\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"oldCall(one)"}),
    )
    .expect("match");
    assert_eq!(
        out["files"][0]["matches"][0],
        json!({"line":1,"column":1,"value":"oldCall(one)"}),
        "{out}"
    );
    assert!(
        out.get("next")
            .is_none_or(|next| next.get("expandCaptures").is_none()),
        "{out}"
    );
}

/// A pattern is the answer as written: one that omits a part (Rust
/// visibility, a return type) matches only items spelled without it, with
/// no silent broader rule, and equals its `rule: {pattern}` form.
#[test]
fn a_pattern_matches_exactly_as_written() {
    let root = Fixture::new();
    let rust = root.0.join("items.rs");
    std::fs::write(
        &rust,
        "pub fn a() -> u8 { 1 }\npub(crate) fn b() {}\nfn c() {}\n",
    )
    .expect("rust");
    let typed = root.0.join("typed.ts");
    std::fs::write(
        &typed,
        "export function typed(a: number): number { return a; }\nexport function untyped(b) { return b; }\n",
    )
    .expect("typed");
    let matched = |path: &PathBuf, query: serde_json::Value| {
        let mut query = query;
        query["operation"] = json!("match");
        query["path"] = json!(path);
        run(&root.0, query).expect("match")
    };
    for (path, pattern, count) in [
        (&rust, "fn $N() { $$$B }", 1),
        (&rust, "pub fn $N() -> $R { $$$B }", 1),
        (&typed, "export function $N($$$A) { $$$B }", 1),
    ] {
        let out = matched(path, json!({"pattern": pattern}));
        assert_eq!(out["matchCount"], count, "{pattern}: {out}");
        assert!(
            out["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|d| d["code"] != "structural.pattern.relaxed"),
            "{pattern}: {out}"
        );
        let rule = matched(path, json!({"rule": {"pattern": pattern}}));
        assert_eq!(out["files"], rule["files"], "{pattern}: {out}");
    }
}

/// An exact declaration pattern that matches nothing while the scope holds
/// declarations of its kind gets a diagnostic counting them and a rule lead
/// that lists them (named as the pattern names it); an expression pattern,
/// or a scope without the kind, gets neither.
#[test]
fn an_exact_declaration_miss_counts_the_kind_and_leads_to_its_rule() {
    let root = Fixture::new();
    let dir = root.0.join("src");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("items.rs"),
        "pub(crate) fn a() -> u8 { 1 }\npub(crate) fn b() -> u8 { 2 }\nstruct S;\n",
    )
    .expect("rust");
    std::fs::write(dir.join("more.rs"), "pub fn run() -> u8 { 3 }\n").expect("rust");
    let matched = |path: &PathBuf, pattern: &str| {
        run(
            &root.0,
            json!({"operation":"match","path":path,"pattern":pattern}),
        )
        .expect("match")
    };
    let miss = matched(&dir, "fn $NAME($$$ARGS) -> $R { $$$BODY }");
    assert_eq!(miss["matchCount"], 0, "{miss}");
    let note = miss["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["code"] == "structural.pattern.exact")
        .unwrap_or_else(|| panic!("exact-miss diagnostic: {miss}"));
    assert!(
        note["message"]
            .as_str()
            .is_some_and(|m| m.contains("3 function_item node(s) in 2 file(s)")),
        "{note}"
    );
    let lead = &miss["next"]["broadenSearch"]["query"]["queries"][0];
    assert_eq!(lead["rule"], json!({"kind":"function_item"}), "{miss}");
    assert!(lead.get("pattern").is_none(), "{lead}");
    crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid lead row");
    let mut follow = lead.clone();
    follow["mainGoal"] = json!("test");
    let listed = run(&root.0, follow).expect("lead runs");
    assert_eq!(listed["matchCount"], 3, "{listed}");
    // A named pattern lists only its name.
    let named = matched(&dir.join("more.rs"), "fn run() -> u8 { 3 }");
    let lead = &named["next"]["broadenSearch"]["query"]["queries"][0];
    assert_eq!(
        lead["rule"],
        json!({"kind":"function_item","has":{"kind":"identifier","regex":"^run$"}}),
        "{named}"
    );
    // An expression pattern is no declaration: no count, no lead.
    let call = matched(&dir, "$X.unwrap()");
    assert!(call["next"].get("broadenSearch").is_none(), "{call}");
    // A kind the scope lacks: nothing to list.
    let none = matched(&dir, "trait $T { $$$B }");
    assert!(none["next"].get("broadenSearch").is_none(), "{none}");
}

/// A rule object that does not decode as a rule (a bad regex, a relation
/// key without a matcher) is
/// `invalidPattern` with no lead: the message names the key, and no syntax
/// tree shows a rule's keys. A rule naming a kind this grammar lacks, or a
/// pattern that does not compile, still leads to the file's syntax tree,
/// where its node shapes are read.
#[test]
fn rule_parse_errors_carry_no_tree_lead() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(&source, "fn a() {}\n").expect("source");
    for (rule, message) in [
        (json!({"regex":"("}), "invalid rule regex"),
        (json!({"has":{"field":"name"}}), "at least one matcher"),
    ] {
        for path in [&source, &root.0] {
            let error = run(
                &root.0,
                json!({"operation":"match","mainGoal":"test","reasoning":"test","path":path,
                    "language":"rust","rule":rule}),
            )
            .expect_err("invalid rule");
            assert_eq!(error.code, ErrorCode::InvalidPattern, "{rule}: {error:?}");
            assert!(error.message.contains(message), "{error:?}");
            assert!(!error.message.starts_with('['), "{error:?}");
            assert!(error.hints.is_empty(), "{error:?}");
            assert!(error.next.is_none(), "{rule}: {error:?}");
        }
    }
    // Without language, a rule no grammar parses is the rule's error, never
    // a choice between the directory's grammars.
    root.write("b.py", "def b(): pass\n");
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,
            "rule":{"regex":"("}}),
    )
    .expect_err("invalid rule");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    assert!(error.message.contains("invalid rule regex"), "{error:?}");
    assert!(error.next.is_none(), "{error:?}");
    for query in [
        json!({"rule":{"kind":"not_a_real_kind"}}),
        json!({"pattern":"fn ???"}),
    ] {
        let mut row =
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source});
        row.as_object_mut()
            .expect("object")
            .extend(query.as_object().expect("object").clone());
        let error = run(&root.0, row).expect_err("does not compile");
        assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
        let next = error.next.as_ref().expect("viewSyntax");
        assert_eq!(
            next["viewSyntax"]["query"]["queries"][0]["path"],
            json!(source.canonicalize().expect("source").to_string_lossy()),
            "{next}"
        );
    }
}

/// A pattern the grammar cannot parse fails before any prefilter: a
/// directory where no file holds the anchor must not report `complete` and
/// empty for `foo(`.
#[test]
fn invalid_pattern_is_an_error_even_when_every_file_is_prefiltered() {
    let root = Fixture::new();
    root.write("a.ts", "const value = 1;\n");
    let error = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript","pattern":"foo("}),
    )
    .expect_err("an unparseable pattern must fail loudly");
    // The response stage attaches the repair hint for this code.
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
}

/// A continuation copies an object rule exactly as written, and it parses
/// as a typed row.
#[test]
fn a_rule_object_continues_as_written() {
    let root = Fixture::new();
    root.write("lib.rs", "fn a() { unsafe { Pin::new_unchecked(&mut x); } }\nfn b() { Pin::new_unchecked(&mut y); }\n");
    let rule = json!({"pattern":"Pin::new_unchecked($A)","not":{"inside":{"kind":"unsafe_block","stopBy":"end"}}});
    // A short value cut hides the capture, so the page offers expandCaptures.
    let object = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","matchContentLength":12,"rule":rule}),
    )
    .expect("object rule");
    let continuation = object["next"]["expandCaptures"]["query"]["queries"][0].clone();
    assert_eq!(continuation["rule"], rule, "{object}");
    assert!(
        serde_json::from_value::<super::AstSearchQuery>(continuation).is_ok(),
        "the continuation parses as a typed row"
    );
}

/// Matches inside a `macro_rules!` template body are listed (no
/// silent omission behind `complete:true`).
#[test]
fn macro_rules_template_bodies_are_listed() {
    let root = Fixture::new();
    root.write("m.rs", "macro_rules! get {\n    ($e:expr) => {\n        config.value.unwrap()\n    };\n}\nfn main() {\n    other.unwrap();\n}\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"$X.unwrap()"}),
    )
    .expect("match");
    let lines = out["files"][0]["matches"]
        .as_array()
        .map(|rows| rows.iter().map(|row| lean_row(row).0).collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(lines, [3, 7], "{out}");
}

/// A rule is the object alone: the object runs, and neither its text nor a
/// rule-file wrapper (`{rule: …}`) is a rule.
#[test]
fn a_rule_is_the_object_alone() {
    let root = Fixture::new();
    root.write("lib.rs", "fn a() { unsafe { Pin::new_unchecked(&mut x); } }\nfn b() { Pin::new_unchecked(&mut y); }\n");
    let bare = json!({"pattern":"Pin::new_unchecked($A)","not":{"inside":{"kind":"unsafe_block","stopBy":"end"}}});
    let row = |rule: serde_json::Value| json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","rule":rule});
    let expected = run(&root.0, row(bare.clone())).expect("the rule runs");
    assert_eq!(
        expected["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{expected}"
    );
    assert_eq!(
        lean_row(&expected["files"][0]["matches"][0]).0,
        2,
        "{expected}"
    );
    crate::contracts::validate_query("astSearch", row(bare.clone()))
        .expect("the rule object is in contract");
    for (spelling, rule) in [
        ("rule text", json!(bare.to_string())),
        ("rule-file object", json!({ "rule": bare })),
        ("rule-file text", json!(json!({ "rule": bare }).to_string())),
    ] {
        assert!(
            crate::contracts::validate_query("astSearch", row(rule)).is_err(),
            "{spelling} is not a rule"
        );
    }
}

/// An empty match counts the files it parsed and leads to one bounded page
/// of the first one's syntax tree, at the line that spells the pattern's
/// anchor, where the pattern's node shape can be compared.
#[test]
fn empty_match_leads_to_syntax_tree() {
    let root = Fixture::new();
    root.write("a.rs", "fn gamma() {}\n\nfn alpha() {\n    beta(1);\n}\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"beta($A, $B)"}),
    )
    .expect("match");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(out["filesScanned"], 1, "{out}");
    let lead = &out["next"]["viewSyntax"];
    assert_eq!(lead["tool"], "astSearch", "{out}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["operation"], "syntaxTree", "{out}");
    assert!(
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs")),
        "{out}"
    );
    assert_eq!(row["line"], 4, "{out}");
    assert_eq!(row["pageSize"], 40, "{out}");
    crate::contracts::validate_query("astSearch", row.clone()).expect("a valid syntaxTree row");
    let tree = run(&root.0, row.clone()).expect("syntax tree");
    assert!(
        tree["nodes"][0]
            .as_str()
            .is_some_and(|node| node.contains("expression_statement 4:")),
        "{tree}"
    );
}

/// Repairable failures carry their one lead and no advice text: a rule
/// that does not compile against a file leads to its syntax tree; a
/// language no file under a directory has leads to the query in the grammar
/// the directory does hold (a repair never removes a field the run needs).
#[test]
fn repairable_failures_carry_one_lead() {
    let root = Fixture::new();
    let file = root.0.join("a.rs");
    std::fs::write(&file, "fn a() {}\n").expect("a");
    let error = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":{"kind":"not_a_real_kind"}}),
    )
    .expect_err("unknown kind");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    assert!(error.hints.is_empty(), "{error:?}");
    let next = error.next.as_ref().expect("lead");
    assert_eq!(
        next["viewSyntax"]["query"]["queries"][0]["operation"], "syntaxTree",
        "{next}"
    );

    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"python","pattern":"print($A)"}),
    )
    .expect("no python file");
    let repair = &out["next"]["repair"]["query"]["queries"][0];
    assert_eq!(repair["language"], "rust", "{out}");
    assert_eq!(repair["pattern"], "print($A)", "{out}");
    assert_eq!(
        out["hints"],
        json!(["No python file under this path was parsed."]),
        "{out}"
    );
}

/// A syntax tree whose parse recovered from errors is the whole recovered
/// tree: no page reaches more, so the row ends at a terminal limit that
/// names its cut.
#[test]
fn a_recovered_syntax_tree_names_its_terminal_cut() {
    let root = Fixture::new();
    let file = root.write("broken.ts", "function f( {\n");
    let out = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("tree");
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(out["terminalLimit"], true, "{out}");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
}

#[test]
fn owned_core_context_and_field_rules_run_as_objects() {
    let root = Fixture::new();
    let file = root.write(
        "fields.ts",
        "class Actual { value = 42; other = 7; }\nfunction name() { value(); }\n",
    );
    let contextual = json!({"pattern":{"context":"class WrapperOnly { $K = $V; }","selector":"public_field_definition"}});
    let field = json!({"kind":"function_declaration","has":{"kind":"identifier","regex":"^name$","field":"name"}});
    for (rule, expected) in [(contextual, 2), (field, 1)] {
        for path in [&file, &root.0] {
            let out = run(
                &root.0,
                json!({"path":path,"operation":"match","rule":rule,"captureText":true}),
            )
            .expect("owned rule");
            assert_eq!(out["matchCount"], expected, "{out}");
        }
    }
}

#[test]
fn owned_core_context_continuations_keep_the_entire_query() {
    let root = Fixture::new();
    let file = root.write("fields.ts", "class Actual { value = 42; other = 7; }\n");
    let rule = json!({"pattern":{"context":"class WrapperOnly { $K = $V; }","selector":"public_field_definition"}});
    let first = run(
        &root.0,
        json!({"path":file,"operation":"match","rule":rule,"matchPageSize":1,"captureText":true}),
    )
    .expect("first page");
    assert_eq!(first["matchCount"], 2, "{first}");
    let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    assert_eq!(next["rule"], rule, "{first}");
    crate::contracts::validate_query("astSearch", next.clone())
        .expect("continuation is in contract");
    let second = run(&root.0, next).expect("second page");
    assert_eq!(
        second["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{second}"
    );
    assert_ne!(
        first["files"][0]["matches"][0],
        second["files"][0]["matches"][0]
    );
    assert!(second["next"].get("nextMatchPage").is_none(), "{second}");
}

#[test]
fn owned_core_sibling_stops_are_applied_by_the_runtime() {
    let root = Fixture::new();
    let file = root.write("calls.ts", "first(); middle(); last();\n");
    let relation = json!({"kind":"expression_statement","regex":"^last","stopBy":"end"});
    let rule = json!({"pattern":{"context":"first();","selector":"expression_statement"},"precedes":relation});
    let out = run(
        &root.0,
        json!({"path":file,"operation":"match","rule":rule}),
    )
    .expect("sibling match");
    assert_eq!(out["matchCount"], 1, "{out}");
    let mut stopped = rule;
    stopped["precedes"]["stopBy"] = json!({"regex":"^middle"});
    let out = run(
        &root.0,
        json!({"path":file,"operation":"match","rule":stopped}),
    )
    .expect("bounded relation");
    assert_eq!(out["matchCount"], 0, "{out}");
}
