//! Rule objects: parse errors, continuations as written, and the owned
//! core's context, field and sibling-stop rules.

use super::*;

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
        serde_json::from_value::<crate::tools::ast_search::AstSearchQuery>(continuation).is_ok(),
        "the continuation parses as a typed row"
    );
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
