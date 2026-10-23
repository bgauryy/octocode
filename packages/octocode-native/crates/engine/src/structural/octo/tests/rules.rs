//! Rule decoding, composition, candidate plans and relational rules.

use super::*;

#[test]
fn ast_audit_rule_rejects_unknown_kinds_at_every_level() {
    for rule in [
        json!({"kind":"not_a_real_node_kind_astro_999"}),
        json!({"kind":"function_declaration","has":{"kind":"not_a_real_node_kind_astro_999"}}),
        json!({"any":[{"kind":"identifier"},{"not":{"kind":"not_a_real_node_kind_astro_999"}}]}),
        // A bad kind beside a pattern that fails to parse is still named.
        json!({"pattern":"const = ;","inside":{"kind":"not_a_real_node_kind_astro_999"}}),
        json!({"all":[{"pattern":"const = ;"},{"has":{"kind":"not_a_real_node_kind_astro_999"}}]}),
    ] {
        let error = CompiledRule::new(&lang("ts"), &rule)
            .err()
            .expect("unknown kinds must fail during rule compilation")
            .to_string();
        assert!(error.contains("unknown node kind"), "{error}");
    }
    assert!(CompiledRule::new(&lang("ts"), &json!({"kind":"ERROR"})).is_ok());
    assert!(run_rule("const x = 1;", "ts", json!({"kind":"function_declaration"})).is_empty());
}

#[test]
fn rule_kind_rejects_supertypes_and_error_prefixes() {
    // Supertypes (`expression`) and ERROR prefixes name no concrete node, so a
    // rule using them would silently match nothing; they must fail compile.
    for kind in ["expression", "ERR", "E", "'('"] {
        let error = CompiledRule::new(&lang("ts"), &json!({"kind": kind}))
            .err()
            .unwrap_or_else(|| panic!("kind {kind} must be rejected"))
            .to_string();
        assert!(error.contains("unknown node kind"), "{error}");
    }
}

#[test]
fn kind_rule_matches_call_expressions() {
    let matches = run_rule(
        "foo(a);\nbar(b);\n",
        "ts",
        json!({"kind":"call_expression"}),
    );
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].text, "foo(a)");
    assert_eq!(matches[1].text, "bar(b)");
}

#[test]
fn inside_rule_walks_ancestors_with_stop_by_end() {
    let src =
        "async function f() {\n  for (const x of xs) {\n    await g(x);\n  }\n  await h();\n}\n";
    let rule = json!({"pattern":"await $C","inside":{"kind":"for_in_statement","stopBy":"end"}});
    let matches = run_rule(src, "ts", rule);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start_line, 3);
}

#[test]
fn all_any_not_rule_composition_works() {
    let src = "foo(a);\nbar(b);\neval(c);\n";
    let any = json!({"any":[{"pattern":"foo($X)"},{"pattern":"bar($X)"}]});
    assert_eq!(run_rule(src, "ts", any).len(), 2);

    let not = json!({"kind":"call_expression","not":{"pattern":"eval($X)"}});
    let matches = run_rule(src, "ts", not);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].text, "foo(a)");
    assert_eq!(matches[1].text, "bar(b)");
}

#[test]
fn pattern_candidate_plan_uses_effective_root_kind() {
    let pattern = CompiledPattern::new(&lang("ts"), "foo($X)").expect("pattern compiles");

    assert!(pattern.candidate_plan().matches_kind("call_expression"));
    assert!(!pattern.candidate_plan().matches_kind("identifier"));
}

#[test]
fn rule_candidate_plan_intersects_all_and_unions_any() {
    let all = CompiledRule::new(
        &lang("ts"),
        &json!({"all":[{"kind":"call_expression"},{"pattern":"foo($X)"}]}),
    )
    .expect("all rule compiles");
    assert!(all.candidate_plan.matches_kind("call_expression"));
    assert!(!all.candidate_plan.matches_kind("identifier"));

    let any = CompiledRule::new(
        &lang("ts"),
        &json!({"any":[{"kind":"call_expression"},{"kind":"identifier"}]}),
    )
    .expect("any rule compiles");
    assert!(any.candidate_plan.matches_kind("call_expression"));
    assert!(any.candidate_plan.matches_kind("identifier"));
    assert!(!any.candidate_plan.matches_kind("string"));
}

#[test]
fn simple_kind_rule_uses_direct_fast_path_shape() {
    let rule = CompiledRule::new(&lang("ts"), &json!({"kind":"call_expression"}))
        .expect("kind rule compiles");

    assert_eq!(rule.simple_kind(), Some("call_expression"));
}

#[test]
fn impossible_candidate_plan_returns_no_matches() {
    let rule = json!({"kind":"identifier","pattern":"foo($X)"});
    let matches = run_rule("foo(a);\nconst b = a;\n", "ts", rule);

    assert!(matches.is_empty());
}

#[test]
fn inside_with_nested_has_does_not_collide_on_secondary_capture() {
    // Both relational walks record the related node under the internal
    // "secondary" capture. An `inside` whose sub-rule contains `has` used
    // to collide on it (different node texts) and reject valid matches.
    let src = "mod tests { fn t() { let v = w.unwrap(); } }\n\
                   mod other { fn o() { let x = y.unwrap(); } }\n";
    let rule = json!({"pattern":"$X.unwrap()","inside":{"kind":"mod_item","stopBy":"end","has":{"kind":"identifier","regex":"^tests$"}}});
    let matches = run_rule(src, "rs", rule);

    assert_eq!(matches.len(), 1, "only the unwrap inside `mod tests`");
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["w".to_string()][..])
    );
    assert!(
        !matches[0].metavars.contains_key(SECONDARY_CAPTURE),
        "internal bookkeeping capture must not leak into output metavars"
    );
    assert!(
        !matches[0].metavar_ranges.contains_key(SECONDARY_CAPTURE),
        "internal bookkeeping capture must not leak into output metavar ranges"
    );
}

#[test]
fn the_rule_is_the_object_itself_without_a_document_wrapper() {
    // The rule object is the rule body: a top-level `rule` key is not part of it.
    let src = "mod tests { fn t() { let v = w.unwrap(); } }\n";
    let bare = json!({"pattern":"$X.unwrap()","inside":{"kind":"mod_item","stopBy":"end"}});
    let wrapped = json!({"rule": bare});

    let bare_matches = run_rule(src, "rs", &bare);
    assert_eq!(bare_matches.len(), 1, "the rule object must match");
    assert_eq!(
        bare_matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["w".to_string()][..])
    );
    let error = compile_matcher(
        &lang("rs"),
        &StructuralQuery::new(None, Some(&wrapped)).unwrap(),
    )
    .err()
    .expect("a wrapped rule is not a rule object")
    .to_string();
    assert!(error.starts_with("[structural.rule.invalid] "), "{error}");
    assert!(error.contains("unknown field `rule`"), "{error}");
}

#[test]
fn one_request_decodes_its_rule_once_across_planning_and_languages() {
    RULE_DECODE_COUNT.with(|count| count.set(0));
    let rule = json!({"all":[{"pattern":"foo($X)"},{"not":{"pattern":"foo(absent)"}}]});
    let query = StructuralQuery::new(None, Some(&rule)).unwrap();
    query.prefilter();
    query.explanation();
    for extension in ["ts", "js"] {
        let run = compile_matcher(&lang(extension), &query).unwrap();
        let matches = all_matches(&run, "foo(value);").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].matched.metavars["X"], ["value"]);
        assert!(all_matches(&run, "foo(absent);").unwrap().is_empty());
    }
    assert_eq!(RULE_DECODE_COUNT.with(std::cell::Cell::get), 1);
}

#[test]
fn ast_grep_rule_file_metadata_is_not_part_of_a_rule_object() {
    for (key, value) in [
        ("id", json!("no-unwrap")),
        ("language", json!("rust")),
        ("severity", json!("warning")),
        ("message", json!("avoid")),
        ("note", json!("why")),
        ("url", json!("https://x")),
        ("metadata", json!({"a": 1})),
        ("constraints", json!({})),
    ] {
        let mut rule = json!({"pattern": "$A.unwrap()"});
        rule[key] = value;
        let query = StructuralQuery::new(None, Some(&rule)).unwrap();
        let error = compile_matcher(&lang("rs"), &query)
            .err()
            .expect("metadata keys are not rule keys")
            .to_string();
        assert!(error.contains(&format!("unknown field `{key}`")), "{error}");
    }
}

/// An unknown rule key names the keys the rule takes and shows one rule that
/// parses and relates two nodes.
#[test]
fn an_unknown_rule_key_comes_with_a_working_example() {
    let rule = json!({"kind": "class_declaration", "has": {"constraints": {}}});
    let query = StructuralQuery::new(None, Some(&rule)).unwrap();
    let error = compile_matcher(&lang("ts"), &query)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("unknown field `constraints`"), "{error}");
    let (_, example) = error.split_once("Example rule: ").expect("an example");
    let example: Value = serde_json::from_str(example).expect("the example is JSON");
    let example = StructuralQuery::new(None, Some(&example)).unwrap();
    assert!(compile_matcher(&lang("ts"), &example).is_ok());
}

/// A rule key from another matcher (`field`) is refused with the keys the
/// rule takes and a working rule to copy.
#[test]
fn an_unknown_rule_key_is_refused_with_an_example_rule() {
    let rule = json!({"kind": "class_declaration", "has": {"constraints": {}}});
    let query = StructuralQuery::new(None, Some(&rule)).unwrap();
    let error = compile_matcher(&lang("ts"), &query)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("unknown field `constraints`"), "{error}");
    assert!(error.contains("expected one of `kind`"), "{error}");
    let example = error.split("Example rule: ").nth(1).expect("example rule");
    let example: Value = serde_json::from_str(example.trim()).expect("the example is JSON");
    let query = StructuralQuery::new(None, Some(&example)).unwrap();
    assert!(
        compile_matcher(&lang("ts"), &query).is_ok(),
        "the example compiles"
    );
}

#[test]
fn an_invalid_rule_is_decoded_once_without_changing_compile_errors() {
    RULE_DECODE_COUNT.with(|count| count.set(0));
    let rule = json!({"pattern": ["["]});
    let query = StructuralQuery::new(None, Some(&rule)).unwrap();
    query.prefilter();
    query.explanation();
    let mut errors = Vec::new();
    for extension in ["ts", "js"] {
        errors.push(
            compile_matcher(&lang(extension), &query)
                .err()
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(errors[0], errors[1]);
    assert!(errors[0].starts_with("[structural.rule.invalid] invalid rule:"));
    assert_eq!(RULE_DECODE_COUNT.with(std::cell::Cell::get), 1);
}

#[test]
fn inside_stop_by_end_is_linear_in_depth_on_deep_nesting() {
    // `inside` + `stopBy: end` must not call `Node::parent()` per ancestor,
    // which re-searches from the root each call (O(depth²) per candidate).
    // 2,000 nested blocks must finish well inside the deadline with every match.
    let depth = 2_000;
    let mut src = String::from("function f() {\n");
    for level in 0..depth {
        src.push_str(&format!("{{ id{level};\n"));
    }
    src.push_str(&"}".repeat(depth));
    src.push_str("\n}\n");
    let rule = json!({"kind":"identifier","inside":{"kind":"function_declaration","stopBy":"end"}});
    let started = Instant::now();
    let matches = run_rule(&src, "ts", rule);
    let elapsed = started.elapsed();
    // `f` itself plus one identifier per nesting level.
    assert_eq!(matches.len(), depth + 1);
    assert!(
        elapsed < std::time::Duration::from_millis(1_500),
        "deep inside took {elapsed:?}"
    );
}

#[test]
fn inside_nested_under_has_resolves_ancestors_without_a_cursor_path() {
    // A `has` sub-rule evaluates descendants without a cursor path, so its own
    // `inside` recomputes the ancestor chain from the document root.
    let src = "function outer() { if (x) { call(a); } }\nfunction other() { call(b); }\n";
    let rule = json!({"kind":"function_declaration","has":{"kind":"call_expression","stopBy":"end","inside":{"kind":"if_statement","stopBy":"end"}}});
    let matches = run_rule(src, "ts", rule);
    assert_eq!(matches.len(), 1);
    assert!(matches[0].text.starts_with("function outer"));
}
