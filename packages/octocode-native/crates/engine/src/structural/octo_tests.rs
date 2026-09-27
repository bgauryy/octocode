use super::*;
use crate::signatures::languages;

#[test]
fn ast_audit_special_pattern_preserves_repeated_capture_equality() {
    let source = "pair(left, right);\npair(same, same);\n";
    let direct = run_pattern(source, "ts", "pair($X, $X)");
    let rule = run_rule(source, "ts", "pattern: 'pair($X, $X)'");
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].text, "pair(same, same)");
    assert_eq!(direct[0].metavars, rule[0].metavars);
    let direct_range = &direct[0].metavar_ranges["X"][0];
    let rule_range = &rule[0].metavar_ranges["X"][0];
    assert_eq!(direct_range.line, rule_range.line);
    assert_eq!(direct_range.column, rule_range.column);
}

#[test]
fn ast_audit_rule_rejects_unknown_kinds_at_every_level() {
    for rule in [
        "kind: not_a_real_node_kind_astro_999",
        "kind: function_declaration\nhas:\n  kind: not_a_real_node_kind_astro_999",
        "any:\n  - kind: identifier\n  - not:\n      kind: not_a_real_node_kind_astro_999",
    ] {
        let error = CompiledRule::new(&lang("ts"), rule)
            .err()
            .expect("unknown kinds must fail during rule compilation");
        assert!(error.contains("unknown node kind"), "{error}");
    }
    assert!(CompiledRule::new(&lang("ts"), "kind: ERROR").is_ok());
    assert!(run_rule("const x = 1;", "ts", "kind: function_declaration").is_empty());
}

#[test]
fn rule_kind_rejects_supertypes_and_error_prefixes() {
    // Supertypes (`expression`) and ERROR prefixes name no concrete node, so a
    // rule using them would silently match nothing; they must fail compile.
    for kind in ["expression", "ERR", "E", "'('"] {
        let error = CompiledRule::new(&lang("ts"), &format!("kind: {kind}"))
            .err()
            .unwrap_or_else(|| panic!("kind {kind} must be rejected"));
        assert!(error.contains("unknown node kind"), "{error}");
    }
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn ast_audit_cpp_multi_capture_body_matches_statements() {
    let pattern = "int $NAME($$$ARGS) { $$$BODY }";
    for ext in ["cpp", "hpp", "cc", "cxx", "hh", "hxx"] {
        for (source, body) in [
            ("int demo(int x) {}", Vec::<&str>::new()),
            ("int demo(int x) { return x; }", vec!["return x;"]),
            (
                "int demo(int x) { int y = x; return y; }",
                vec!["int y = x;", "return y;"],
            ),
        ] {
            for matches in [
                run_pattern(source, ext, pattern),
                run_rule(source, ext, &format!("pattern: '{pattern}'")),
            ] {
                assert_eq!(matches.len(), 1, "{ext}: {source}");
                assert_eq!(matches[0].metavars["NAME"], ["demo"]);
                assert_eq!(matches[0].metavars["BODY"], body);
            }
        }
    }
    let matches = run_pattern("int x{1};", "cpp", "int x{$VALUE};");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].metavars["VALUE"], ["1"]);
}

fn lang(ext: &str) -> AgLanguage {
    AgLanguage::new(
        ext,
        languages::find_entry(ext).expect("test language should exist"),
    )
}

fn assert_fragment_context(ext: &str, source: &str, bare: &str, expected: &str) {
    for capture in ["VALUE", "X"] {
        let bare = bare.replace("$VALUE", &format!("${capture}"));
        let terminated = format!("{bare};");
        for pattern in [bare.as_str(), terminated.as_str()] {
            let rule = format!("all:\n  - pattern: '{pattern}'\n  - regex: '.'");
            for matches in [
                run_pattern(source, ext, pattern),
                run_rule(source, ext, &rule),
            ] {
                assert_eq!(matches.len(), 1, "{ext}: {pattern}");
                assert_eq!(matches[0].text, expected, "{ext}: {pattern}");
                assert_eq!(matches[0].metavars[capture], ["value"]);
                let range = &matches[0].metavar_ranges[capture][0];
                assert_eq!(range.text, "value");
                assert_eq!(range.end_column - range.column, 5);
            }
        }
    }
}

#[test]
fn shared_pattern_context_accepts_bare_java_calls() {
    let source = "class Demo { void run() { target(value); other(value); } }";
    assert_fragment_context("java", source, "target($VALUE)", "target(value)");
    assert!(run_pattern(source, "java", "absent($VALUE)").is_empty());
    assert_eq!(
        run_pattern(source, "java", "class $NAME { $$$BODY }").len(),
        1
    );
}

#[test]
fn shared_pattern_context_accepts_bare_calls_in_c_family_rust_and_go() {
    let cases = [
        ("rs", "fn run() { target(value); other(value); }"),
        ("c", "void run(void) { target(value); other(value); }"),
        ("h", "void run(void) { target(value); other(value); }"),
        ("cpp", "void run() { target(value); other(value); }"),
        (
            "go",
            "package main\nfunc run() { target(value); other(value) }",
        ),
    ];
    for (ext, source) in cases {
        assert_fragment_context(ext, source, "target($VALUE)", "target(value)");
        assert!(
            run_pattern(source, ext, "absent($VALUE)").is_empty(),
            "{ext}"
        );
    }
}

fn run_pattern(src: &str, ext: &str, pattern: &str) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(Some(pattern), None).expect("query"),
    )
    .expect("compile pattern");
    matcher(src)
        .expect("complete execution")
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

fn run_rule(src: &str, ext: &str, rule: &str) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(None, Some(rule)).expect("query"),
    )
    .expect("compile rule");
    matcher(src)
        .expect("complete execution")
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

#[test]
fn document_probe_returns_root() {
    let matches = run_pattern("foo(a)\nbar(b)\n", "ts", "$$$");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start_line, 1);
    assert_eq!(matches[0].text, "foo(a)\nbar(b)\n");
}

#[test]
fn simple_call_pattern_captures_single_metavar() {
    let matches = run_pattern(
        "const a = foo(bar);\nconst b = foo(baz);\n",
        "ts",
        "foo($X)",
    );
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["bar".to_string()][..])
    );
    assert_eq!(
        matches[1].metavars.get("X").map(Vec::as_slice),
        Some(&["baz".to_string()][..])
    );
}

#[test]
fn bare_metavar_does_not_match_a_missing_named_node() {
    // "${}" is an empty template-literal interpolation — tree-sitter's error
    // recovery inserts a MISSING (zero-width) `identifier` node in place of
    // the missing expression, rather than an ERROR node (verified via a
    // direct parse-tree dump). A bare `$X` metavar pattern matches every
    // named node (`CandidatePlan::Any`), so without an explicit exclusion it
    // would report a phantom identifier match with empty captured text at
    // that position.
    let src = "x = `${}`;\nconst real = 1;\n";
    let matches = run_pattern(src, "ts", "$X");
    let empty_text_matches: Vec<_> = matches.iter().filter(|m| m.text.is_empty()).collect();
    assert!(
        empty_text_matches.is_empty(),
        "bare metavar matched {} MISSING/empty-text node(s), first at line {}",
        empty_text_matches.len(),
        empty_text_matches
            .first()
            .map(|m| m.start_line)
            .unwrap_or(0)
    );
    // Sanity: the exclusion doesn't over-reject — real identifiers elsewhere
    // in the same file still match.
    assert!(
        matches.iter().any(|m| m.text == "real"),
        "expected a real identifier match to survive the exclusion"
    );
}

// Some grammars parse a bare `$$$BODY` expando identifier at statement
// position ambiguously: an unrecognized identifier looks like the start of a
// declaration, and tree-sitter's error recovery inserts a zero-width MISSING
// `;` node as its sibling. The compiled pattern's root can still be a
// legitimate, non-`is_error()` node (so `CompiledPattern::new` accepts it) —
// but without filtering, that spurious MISSING sibling could never match any
// real candidate child, silently breaking every `{ $$$BODY }`-shaped pattern
// (0 matches, no error). One test per affected grammar, split so one
// grammar's regression doesn't hide another's.

#[test]
fn multi_capture_body_matches_in_c_despite_missing_sibling() {
    let matches = run_pattern(
        "int foo(int x) {\n  return x;\n}\n",
        "c",
        "int $NAME($$$ARGS) { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "C matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["foo".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-c-sharp")]
#[test]
fn multi_capture_body_matches_in_csharp_despite_missing_sibling() {
    // C# has no top-level member syntax at all — `public int Foo(...) {...}`
    // parsed standalone doesn't just leave a MISSING sibling (the C/C++
    // shape); the whole body ends up wrapped in an ERROR node, because
    // `public` isn't a valid modifier outside a class/struct/interface body.
    // `preprocess_pattern` wraps every C# pattern in a synthetic
    // `class __OctoWrap { ... }` (see `AgLanguage::class_wrap`) so the parser
    // has real member context, and `effective_pattern_root` unwraps through
    // that specific synthetic class by name (see `CSHARP_WRAP_MARKER`) to
    // reach the real member. `meta_from_node` is purely text-based, so the
    // leftover ERROR wrapper around `$$$BODY` doesn't block recognizing it
    // as a multi-capture once the root kind is right.
    let matches = run_pattern(
        "class Box {\n  public int Foo(int x) {\n    return x;\n  }\n}\n",
        "cs",
        "public int $NAME($$$ARGS) { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "C# matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["Foo".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-c-sharp")]
#[test]
fn class_shaped_pattern_still_matches_in_csharp_despite_synthetic_wrap() {
    // The synthetic wrapper class must unwrap ONLY itself (matched by its
    // exact literal name, `__OctoWrap`) — a genuine `class $NAME { ... }`
    // pattern, once wrapped as `class __OctoWrap { class µNAME { ... } }`,
    // must still resolve its effective root to the INNER class, not get
    // stuck comparing the synthetic outer class's own (non-metavar) name
    // against real candidates.
    let matches = run_pattern(
        "class Box {\n  public int Foo(int x) {\n    return x;\n  }\n}\n",
        "cs",
        "class $NAME { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "C# matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["Box".to_string()][..])
    );
}

#[test]
fn comments_and_strings_do_not_match_call_pattern() {
    let src = "// eval(evil)\nconst s = \"eval(evil)\";\neval(real);\n";
    let matches = run_pattern(src, "js", "eval($X)");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start_line, 3);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["real".to_string()][..])
    );
}

#[test]
fn multi_capture_preserves_argument_separators() {
    let matches = run_pattern("log(1, 2, 3);\n", "js", "log($$$ARGS)");
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].metavars.get("ARGS").map(Vec::as_slice),
        Some(
            &[
                "1".to_string(),
                ",".to_string(),
                "2".to_string(),
                ",".to_string(),
                "3".to_string()
            ][..]
        )
    );
}

#[test]
fn kind_rule_matches_call_expressions() {
    let matches = run_rule(
        "foo(a);\nbar(b);\n",
        "ts",
        "rule:\n  kind: call_expression\n",
    );
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].text, "foo(a)");
    assert_eq!(matches[1].text, "bar(b)");
}

#[test]
fn inside_rule_walks_ancestors_with_stop_by_end() {
    let src =
        "async function f() {\n  for (const x of xs) {\n    await g(x);\n  }\n  await h();\n}\n";
    let rule =
        "rule:\n  pattern: await $C\n  inside:\n    kind: for_in_statement\n    stopBy: end\n";
    let matches = run_rule(src, "ts", rule);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start_line, 3);
}

#[test]
fn all_any_not_rule_composition_works() {
    let src = "foo(a);\nbar(b);\neval(c);\n";
    let any = "rule:\n  any:\n    - pattern: foo($X)\n    - pattern: bar($X)\n";
    assert_eq!(run_rule(src, "ts", any).len(), 2);

    let not = "rule:\n  kind: call_expression\n  not:\n    pattern: eval($X)\n";
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
        "rule:\n  all:\n    - kind: call_expression\n    - pattern: foo($X)\n",
    )
    .expect("all rule compiles");
    assert!(all.candidate_plan.matches_kind("call_expression"));
    assert!(!all.candidate_plan.matches_kind("identifier"));

    let any = CompiledRule::new(
        &lang("ts"),
        "rule:\n  any:\n    - kind: call_expression\n    - kind: identifier\n",
    )
    .expect("any rule compiles");
    assert!(any.candidate_plan.matches_kind("call_expression"));
    assert!(any.candidate_plan.matches_kind("identifier"));
    assert!(!any.candidate_plan.matches_kind("string"));
}

#[test]
fn simple_kind_rule_uses_direct_fast_path_shape() {
    let rule = CompiledRule::new(&lang("ts"), "rule:\n  kind: call_expression\n")
        .expect("kind rule compiles");

    assert_eq!(rule.simple_kind(), Some("call_expression"));
}

#[test]
fn impossible_candidate_plan_returns_no_matches() {
    let rule = "rule:\n  kind: identifier\n  pattern: foo($X)\n";
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
    let rule = "rule:\n  pattern: $X.unwrap()\n  inside:\n    kind: mod_item\n    stopBy: end\n    has:\n      kind: identifier\n      regex: ^tests$\n";
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
fn bare_rule_without_document_wrapper_is_accepted() {
    // Agents write the rule body directly; the engine must accept it
    // without a top-level `rule:` key.
    let src = "mod tests { fn t() { let v = w.unwrap(); } }\n";
    let bare = "pattern: $X.unwrap()\ninside:\n  kind: mod_item\n  stopBy: end\n";
    let wrapped = "rule:\n  pattern: $X.unwrap()\n  inside:\n    kind: mod_item\n    stopBy: end\n";

    let bare_matches = run_rule(src, "rs", bare);
    let wrapped_matches = run_rule(src, "rs", wrapped);

    assert_eq!(bare_matches.len(), 1, "bare rule form must match");
    assert_eq!(
        bare_matches.len(),
        wrapped_matches.len(),
        "bare and wrapped forms must behave identically"
    );
    assert_eq!(
        bare_matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["w".to_string()][..])
    );
}

#[test]
fn deeply_nested_input_does_not_stack_overflow() {
    // A ~200 KB run of nested `[` produces a tree far deeper than a test
    // thread's 2 MB stack can survive with a naive recursive walker. The
    // depth guard must let the (unmatched) search return without crashing.
    let depth = 100_000;
    let src = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let matches = run_pattern(&src, "js", "foo($X)");
    assert!(
        matches.is_empty(),
        "no call expression exists in a nested-array blob"
    );
}

#[test]
fn multiple_multi_captures_terminate_within_attempt_budget() {
    // Three `$$$` around literal separators against a wide argument list is a
    // combinatorial split space. None of the args are the literal `x`/`y`
    // the pattern demands, so it can never match — the point is that the
    // attempts budget makes it bail quickly instead of exploring every split.
    let args: Vec<String> = (0..40).map(|i| i.to_string()).collect();
    let src = format!("f({});\n", args.join(", "));
    let start = std::time::Instant::now();
    let matches = run_pattern(&src, "js", "f($$$A, x, $$$B, y, $$$C)");
    assert!(
        start.elapsed().as_secs() < 5,
        "bounded backtracking must terminate promptly"
    );
    assert!(
        matches.is_empty(),
        "no `x`/`y` separators exist in the args"
    );
}

#[test]
fn structural_review_parser_interruption_resets_cached_parser() {
    let language = lang("ts").tree_sitter_language();
    let source = "const x = 1;\n".repeat(10_000);
    let error =
        parse_tree_with_deadline(&language, &source, Instant::now()).expect_err("cancel parse");
    assert_eq!(error.code, "structural.parse.interrupted");
    let next = parse_tree(&language, "probe();").expect("next independent document parses");
    assert_eq!(next.root_node().end_byte(), 8);
}

#[test]
fn structural_review_expired_match_deadline_is_explicit() {
    let language = lang("ts").tree_sitter_language();
    let tree = parse_tree(&language, "probe();").expect("tree");
    let error = visit_named(tree.root_node(), Instant::now(), &mut |_| Ok(()))
        .expect_err("expired deadline");
    assert_eq!(error.code, "structural.match.deadline");
}

#[test]
fn expired_parser_deadline_is_honored_for_tiny_inputs() {
    let language = lang("ts").tree_sitter_language();
    let error = parse_tree_with_deadline(&language, "x", Instant::now())
        .expect_err("even a parse shorter than the progress callback interval must expire");
    assert_eq!(error.code, "structural.parse.interrupted");
}

#[test]
fn one_request_parses_yaml_once_across_planning_and_languages() {
    RULE_PARSE_COUNT.with(|count| count.set(0));
    let query = StructuralQuery::new(
        None,
        Some("all: [{pattern: foo($X)}, {not: {pattern: foo(absent)}}]"),
    )
    .unwrap();
    query.prefilter();
    query.explanation();
    for extension in ["ts", "js"] {
        let run = compile_matcher(&lang(extension), &query).unwrap();
        let matches = run("foo(value);").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].matched.metavars["X"], ["value"]);
        assert!(run("foo(absent);").unwrap().is_empty());
    }
    assert_eq!(RULE_PARSE_COUNT.with(std::cell::Cell::get), 1);
}

#[test]
fn csharp_statement_and_expression_patterns_match_inside_methods() {
    let source = "class W {\n  int Area() { return Helper(LIMIT); }\n  static int Run() { var s = \"😀\"; Log(s); return Helper(s.Length); }\n}\n";
    let calls = run_pattern(source, "cs", "Helper($A)");
    assert_eq!(calls.len(), 2, "expression pattern");
    assert_eq!(calls[1].metavars["A"], ["s.Length"]);
    assert_eq!(
        run_pattern(source, "cs", "Log($A);").len(),
        1,
        "statement pattern"
    );
    assert_eq!(
        run_pattern(source, "cs", "Helper(LIMIT)").len(),
        1,
        "literal pattern"
    );
}

#[test]
fn pasted_ast_grep_rule_file_metadata_is_tolerated() {
    let rule_file = "id: no-unwrap\nlanguage: rust\nseverity: warning\nmessage: avoid\nnote: why\nurl: https://x\nmetadata: {a: 1}\nrule:\n  pattern: $A.unwrap()\n";
    let matches = run_rule("fn f() { let v = x.unwrap(); }", "rs", rule_file);
    assert_eq!(matches.len(), 1, "metadata keys do not affect matching");
    let unsupported =
        StructuralQuery::new(None, Some("id: x\nrule:\n  pattern: $A\nconstraints: {}\n")).unwrap();
    let error = compile_matcher(&lang("rs"), &unsupported).err().unwrap();
    assert!(error.contains("unknown field `constraints`"), "{error}");
}

#[test]
fn malformed_yaml_is_parsed_once_without_changing_compile_errors() {
    RULE_PARSE_COUNT.with(|count| count.set(0));
    let query = StructuralQuery::new(None, Some("pattern: [")).unwrap();
    query.prefilter();
    query.explanation();
    let mut errors = Vec::new();
    for extension in ["ts", "js"] {
        errors.push(compile_matcher(&lang(extension), &query).err().unwrap());
    }
    assert_eq!(errors[0], errors[1]);
    assert!(errors[0].starts_with("[structural.query.compileFailed] invalid rule YAML:"));
    assert_eq!(RULE_PARSE_COUNT.with(std::cell::Cell::get), 1);
}

#[test]
fn rust_raw_identifier_borrow_parses_without_recovery_and_matches() {
    let source = "fn main() { let raw = 1; inspect(&raw); }";
    let language = lang("rs").tree_sitter_language();
    let tree = parse_tree(&language, source).expect("valid Rust parses");
    assert!(
        !tree.root_node().has_error(),
        "borrowing an identifier named raw is valid Rust: {}",
        tree.root_node().to_sexp()
    );
    let matches = run_pattern(source, "rs", "inspect($X)");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].metavars["X"], vec!["&raw"]);
}

#[test]
fn capture_env_rollback_restores_inserts_and_replacements() {
    let mut env = CaptureEnv::default();
    assert!(env.capture_one("A", "a", (0, 0, 0, 1)));
    env.capture_replace(SECONDARY_CAPTURE, "first".to_owned(), (0, 0, 0, 5));
    let checkpoint = env.checkpoint();
    assert!(env.capture_one("B", "b", (1, 0, 1, 1)));
    assert!(env.capture_many("C", ["c1", "c2"].into_iter(), || vec![(2, 0, 2, 1); 2]));
    env.capture_replace(SECONDARY_CAPTURE, "second".to_owned(), (3, 0, 3, 6));
    // A backreference check that fails binds nothing.
    assert!(!env.capture_one("A", "other", (4, 0, 4, 5)));
    assert_eq!(env.undo_len(), 5);
    env.rollback(checkpoint);
    assert_eq!(env.undo_len(), 2);
    assert!(env.capture_one("A", "a", (0, 0, 0, 1)), "A kept");
    let (values, ranges) = env.into_maps();
    assert_eq!(values.keys().collect::<Vec<_>>(), ["A"]);
    assert_eq!(ranges["A"], vec![(0, 0, 0, 1)]);
}

#[test]
fn failed_rule_branches_leave_no_bindings_behind() {
    // The first `any` alternative binds X before failing on `bar`; its binding
    // must be rolled back, not leak into the match from the second one.
    let matches = run_rule(
        "foo(a, b);\n",
        "ts",
        "any:\n  - pattern: foo($X, bar)\n  - pattern: foo($Y, $Z)",
    );
    assert_eq!(matches.len(), 1);
    let mut names = matches[0].metavars.keys().cloned().collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["Y", "Z"]);

    // `not` never contributes bindings, and a failed `all` member rolls back
    // everything its siblings bound for that candidate.
    let matches = run_rule(
        "foo(a, b);\n",
        "ts",
        "all:\n  - pattern: foo($X, $W)\n  - not: {pattern: 'foo($Q, c)'}",
    );
    assert_eq!(matches.len(), 1);
    assert!(!matches[0].metavars.contains_key("Q"));
    assert!(
        run_rule(
            "foo(a, b);\n",
            "ts",
            "all:\n  - pattern: foo($X, $W)\n  - pattern: foo($W, $X)",
        )
        .is_empty(),
        "backreferences across all members still apply"
    );

    // A `$$$` split that binds then fails downstream is retried cleanly.
    let matches = run_pattern("f(a, b, c);\n", "ts", "f($$$ARGS, $LAST)");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].metavars["ARGS"], ["a", ",", "b"]);
    assert_eq!(matches[0].metavars["LAST"], ["c"]);
}

#[test]
fn jsx_tag_pattern_matches_elements_only_in_jsx_grammars() {
    let source = "const view = <div className=\"x\"><Item id={1} /></div>;\n";
    for ext in ["tsx", "jsx", "js"] {
        let matches = run_pattern(source, ext, "<$T>");
        let tags = matches
            .iter()
            .map(|m| m.metavars["T"][0].as_str())
            .collect::<Vec<_>>();
        assert_eq!(tags, ["div", "Item"], "{ext}");
        assert_eq!(matches[0].text, "<div className=\"x\">", "{ext}");
        assert_eq!(matches[1].text, "<Item id={1} />", "{ext}");
    }
    // Rules share the same special matcher and backreference semantics.
    assert_eq!(run_rule(source, "tsx", "pattern: <$T>").len(), 2);

    // Grammars without JSX compile `<$T>` as an ordinary pattern: it never
    // takes the tag path (and so never matches JSX-looking text).
    for ext in ["ts", "py", "rs"] {
        if let Ok(matcher) = compile_matcher(
            &lang(ext),
            &StructuralQuery::new(Some("<$T>"), None).expect("query"),
        ) {
            assert!(
                matcher("let x = 1;\n")
                    .map(|m| m.is_empty())
                    .unwrap_or(true),
                "{ext}"
            );
        }
    }
}

#[test]
fn key_value_pattern_is_gated_on_grammars_with_pairs() {
    let matches = run_pattern("const o = { a: 1, b: two };\n", "ts", "$K: $V");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[1].metavars["K"], ["b"]);
    assert_eq!(matches[1].metavars["V"], ["two"]);
    let py = run_pattern("d = {'a': 1}\n", "py", "$K: $V");
    assert_eq!(py.len(), 1);
    assert_eq!(py[0].metavars["K"], ["'a'"]);
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
    let rule =
        "rule:\n  kind: identifier\n  inside:\n    kind: function_declaration\n    stopBy: end\n";
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
    let rule = "rule:\n  kind: function_declaration\n  has:\n    kind: call_expression\n    stopBy: end\n    inside:\n      kind: if_statement\n      stopBy: end\n";
    let matches = run_rule(src, "ts", rule);
    assert_eq!(matches.len(), 1);
    assert!(matches[0].text.starts_with("function outer"));
}
