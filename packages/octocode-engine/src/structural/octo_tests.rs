use super::*;
use crate::signatures::languages;

#[test]
fn ast_audit_special_pattern_preserves_repeated_capture_equality() {
    let source = "left: right\nsame: same\n";
    let direct = run_pattern(source, "yml", "$X: $X");
    let rule = run_rule(source, "yml", "pattern: \"$X: $X\"");
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].text, "same: same");
    assert_eq!(direct[0].metavars, rule[0].metavars);
    let direct_range = &direct[0].metavar_ranges["X"][0];
    let rule_range = &rule[0].metavar_ranges["X"][0];
    assert_eq!(direct_range.line, rule_range.line);
    assert_eq!(direct_range.column, rule_range.column);
}

#[test]
fn ast_audit_html_open_tag_range_respects_quoted_greater_than() {
    for source in ["<div title=\"a > b\">text</div>", "<input title='a > b' />"] {
        let matches = run_pattern(source, "html", "<$TAG>");
        assert_eq!(matches.len(), 1);
        let expected_end = source.rfind('>').expect("closing delimiter");
        let expected = if source.starts_with("<div") {
            "<div title=\"a > b\">"
        } else {
            &source[..=expected_end]
        };
        assert_eq!(matches[0].text, expected);
        assert_eq!(matches[0].end_col as usize, expected.len());
    }
}

#[test]
fn ast_audit_html_tags_cover_script_style_without_matching_raw_text() {
    let source = r#"<main><script title="a > b">const tpl = "<b>fake</b>";</script><style data-note='a > b'>.x::before{content:"<i>fake</i>"}</style><input /></main>"#;
    let expected = [
        "<main>",
        r#"<script title="a > b">"#,
        "<style data-note='a > b'>",
        "<input />",
    ];
    for matches in [
        run_pattern(source, "html", "<$TAG>"),
        run_rule(source, "html", "pattern: '<$TAG>'"),
    ] {
        assert_eq!(
            matches
                .iter()
                .map(|matched| matched.text.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for (matched, tag) in matches.iter().zip(["main", "script", "style", "input"]) {
            assert_eq!(matched.metavars["TAG"], [tag]);
            assert_eq!(
                matched.end_col - matched.start_col,
                matched.text.len() as u32
            );
        }
    }
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
fn shared_pattern_context_accepts_bare_css_declarations() {
    let source = ".demo { color: value; background: other; }";
    assert_fragment_context("css", source, "color: $VALUE", "color: value;");
    assert!(run_pattern(source, "css", "width: $VALUE").is_empty());
    assert_eq!(
        run_pattern(source, "css", ".demo { color: $VALUE; background: other; }").len(),
        1
    );
}

#[cfg(feature = "tree-sitter-extended")]
#[test]
fn shared_pattern_context_accepts_bare_scss_declarations() {
    let source = "$theme: blue; .demo { color: value; background: $theme; }";
    assert_fragment_context("scss", source, "color: $VALUE", "color: value;");
    assert!(run_pattern(source, "scss", "width: $VALUE").is_empty());
    assert_eq!(
        run_pattern(
            source,
            "scss",
            ".demo { color: $VALUE; background: $theme; }"
        )
        .len(),
        1
    );
}

#[test]
fn point_column_uses_utf16_code_units_not_code_points() {
    // "🌍" is one Unicode scalar value but TWO UTF-16 code units (surrogate
    // pair) and FOUR UTF-8 bytes. Columns must agree with the resolver /
    // signatures layers, which count UTF-16 code units.
    let content = "const 🌍x = 1;";
    let index = LineIndex::new(content);
    // "const " = 6 bytes, "🌍" = 4 bytes → byte column of `x` is 10.
    // UTF-16: 6 (ascii) + 2 (emoji) = 8.
    assert_eq!(index.point_column_to_char_column(0, 10), 8);
    // Pure-ASCII prefix is unchanged (byte == utf-16).
    assert_eq!(index.point_column_to_char_column(0, 6), 6);
}

fn run_pattern(src: &str, ext: &str, pattern: &str) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        StructuralQuery::new(Some(pattern), None).expect("query"),
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
        StructuralQuery::new(None, Some(rule)).expect("query"),
    )
    .expect("compile rule");
    matcher(src)
        .expect("complete execution")
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

#[cfg(feature = "tree-sitter-extended")]
#[test]
fn expression_patterns_match_inside_sql_documents() {
    let cases = [(
        "sql",
        "SELECT target FROM users;\n",
        "SELECT $COL FROM users",
        "COL",
        "target",
    )];
    let mut failures = Vec::new();
    for (ext, source, pattern, capture, expected) in cases {
        let language = lang(ext);
        let pattern_source = language.preprocess_pattern(pattern);
        let pattern_tree = parse_tree(&language.tree_sitter_language(), &pattern_source).unwrap();
        let source_tree = parse_tree(&language.tree_sitter_language(), source).unwrap();
        let matches = run_pattern(source, ext, pattern);
        if matches.len() != 1
            || matches[0].metavars.get(capture) != Some(&vec![expected.to_owned()])
        {
            failures.push(format!(
                ".{ext}: {} matches\npattern: {}\nsource: {}",
                matches.len(),
                pattern_tree.root_node().to_sexp(),
                source_tree.root_node().to_sexp()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
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

#[cfg(feature = "tree-sitter-extended")]
#[test]
fn multi_capture_body_matches_in_zig_despite_missing_sibling() {
    let matches = run_pattern(
        "pub fn foo(x: i32) i32 {\n    return x;\n}\n",
        "zig",
        "pub fn $NAME($$$ARGS) i32 { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "Zig matched {} times, expected 1",
        matches.len()
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

#[cfg(feature = "tree-sitter-extended")]
#[test]
fn lua_pattern_matches_a_real_function_call() {
    let matches = run_pattern("local x = 1\nprint(x)\n", "lua", "print(x)");
    assert_eq!(
        matches.len(),
        1,
        "Lua matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(matches[0].start_line, 2);
}

#[test]
fn php_pattern_matches_a_real_assignment() {
    // Before the `<?php` auto-wrap, a bare pattern with no PHP tag parsed as
    // one opaque `text` node (PHP's grammar treats un-tagged input as host
    // HTML/text, not code) — `text` never appears as a candidate when
    // walking a real (tagged) document, so this always matched nothing.
    let matches = run_pattern("<?php\n$x = 5;\n", "php", "$x = 5;");
    assert_eq!(
        matches.len(),
        1,
        "PHP matched {} times, expected 1",
        matches.len()
    );

    // A call-argument pattern also exercises the wrap since `$ARG` metavars
    // (PHP keeps `$` as its own expando, for real `$var` patterns) sit in a
    // valid expression position here, unlike a bare-word position such as a
    // function name.
    let call_matches = run_pattern("<?php\nfindMe($a, $b);\n", "php", "findMe($ARG1, $ARG2)");
    assert_eq!(
        call_matches.len(),
        1,
        "PHP call pattern matched {} times, expected 1",
        call_matches.len()
    );
}

#[cfg(feature = "tree-sitter-extended")]
#[test]
fn zig_pattern_with_metavars_matches_a_real_function() {
    let matches = run_pattern(
        "pub fn findMe(x: i32) i32 {\n    return x;\n}\n",
        "zig",
        "pub fn $NAME($$$ARGS) i32 { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "Zig matched {} times, expected 1",
        matches.len()
    );
}

// PHP variables keep `$`, while function names require a plain identifier.
// Verify both positions after substituting `_` only for function names.

#[test]
fn php_pattern_matches_a_function_with_bare_word_name_position() {
    let matches = run_pattern(
        "<?php\nfunction findMe($a, $b) {\n    return $a + $b;\n}\n",
        "php",
        "function $NAME($a, $b) { $$$BODY }",
    );
    assert_eq!(
        matches.len(),
        1,
        "PHP matched {} times, expected 1",
        matches.len()
    );
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["findMe".to_string()][..])
    );
    assert_eq!(
        matches[0].metavars.get("BODY").map(Vec::as_slice),
        Some(&["return $a + $b;".to_string()][..])
    );

    // $ARG-position (call argument) metavars are unaffected by the
    // bare-word-position special case — same expando ($) either way.
    let call_matches = run_pattern("<?php\nfindMe($a, $b);\n", "php", "findMe($ARG1, $ARG2)");
    assert_eq!(
        call_matches.len(),
        1,
        "PHP call matched {} times, expected 1",
        call_matches.len()
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
