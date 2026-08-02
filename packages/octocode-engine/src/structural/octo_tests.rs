use super::*;
use crate::signatures::languages;

fn lang(ext: &str) -> AgLanguage {
    AgLanguage::new(
        ext,
        languages::find_entry(ext).expect("test language should exist"),
    )
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
    matcher(src).into_iter().map(|m| m.matched).collect()
}

fn run_rule(src: &str, ext: &str, rule: &str) -> Vec<StructuralMatch> {
    let matcher = compile_matcher(
        &lang(ext),
        StructuralQuery::new(None, Some(rule)).expect("query"),
    )
    .expect("compile rule");
    matcher(src).into_iter().map(|m| m.matched).collect()
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
    assert_eq!(matches.len(), 1, "C matched {} times, expected 1", matches.len());
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["foo".to_string()][..])
    );
}

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
    assert_eq!(matches.len(), 1, "C# matched {} times, expected 1", matches.len());
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["Foo".to_string()][..])
    );
}

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
    assert_eq!(matches.len(), 1, "C# matched {} times, expected 1", matches.len());
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["Box".to_string()][..])
    );
}

#[test]
fn multi_capture_body_matches_in_zig_despite_missing_sibling() {
    let matches = run_pattern(
        "pub fn foo(x: i32) i32 {\n    return x;\n}\n",
        "zig",
        "pub fn $NAME($$$ARGS) i32 { $$$BODY }",
    );
    assert_eq!(matches.len(), 1, "Zig matched {} times, expected 1", matches.len());
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
fn lua_pattern_matches_a_real_function_call() {
    let matches = run_pattern("local x = 1\nprint(x)\n", "lua", "print(x)");
    assert_eq!(matches.len(), 1, "Lua matched {} times, expected 1", matches.len());
    assert_eq!(matches[0].start_line, 2);
}

#[test]
fn php_pattern_matches_a_real_assignment() {
    // Before the `<?php` auto-wrap, a bare pattern with no PHP tag parsed as
    // one opaque `text` node (PHP's grammar treats un-tagged input as host
    // HTML/text, not code) — `text` never appears as a candidate when
    // walking a real (tagged) document, so this always matched nothing.
    let matches = run_pattern("<?php\n$x = 5;\n", "php", "$x = 5;");
    assert_eq!(matches.len(), 1, "PHP matched {} times, expected 1", matches.len());

    // A call-argument pattern also exercises the wrap since `$ARG` metavars
    // (PHP keeps `$` as its own expando, for real `$var` patterns) sit in a
    // valid expression position here, unlike a bare-word position such as a
    // function name.
    let call_matches = run_pattern(
        "<?php\nfindMe($a, $b);\n",
        "php",
        "findMe($ARG1, $ARG2)",
    );
    assert_eq!(
        call_matches.len(),
        1,
        "PHP call pattern matched {} times, expected 1",
        call_matches.len()
    );
}

#[test]
fn erlang_pattern_with_metavars_matches_a_real_function() {
    let matches = run_pattern(
        "-module(m).\nfindMe(A, B) -> A + B.\n",
        "erl",
        "$NAME($$$ARGS) -> $$$BODY.",
    );
    assert_eq!(matches.len(), 1, "Erlang matched {} times, expected 1", matches.len());
}

#[test]
fn elixir_pattern_with_metavars_matches_a_real_def() {
    let matches = run_pattern(
        "defmodule M do\n  def findMe(a, b) do\n    a + b\n  end\nend\n",
        "ex",
        "def $NAME($$$ARGS) do $$$BODY end",
    );
    assert_eq!(matches.len(), 1, "Elixir matched {} times, expected 1", matches.len());
}

#[test]
fn zig_pattern_with_metavars_matches_a_real_function() {
    let matches = run_pattern(
        "pub fn findMe(x: i32) i32 {\n    return x;\n}\n",
        "zig",
        "pub fn $NAME($$$ARGS) i32 { $$$BODY }",
    );
    assert_eq!(matches.len(), 1, "Zig matched {} times, expected 1", matches.len());
}

// PHP and Bash both keep `$` as `Expando::primary` (so `$ARG`-shaped
// argument/variable patterns stay valid PHP/Bash syntax), but neither
// language accepts a `$`-prefixed token at a *bare-word* position — a
// function name is a plain identifier in both. `Expando::bare_word` (`_`,
// valid in both) is substituted there instead; `meta_from_node` recognizes
// either character (see its doc comment). One test per language, each
// checking both the previously-broken bare-word position AND that the
// existing `$ARG`-position behavior didn't regress.

#[test]
fn php_pattern_matches_a_function_with_bare_word_name_position() {
    let matches = run_pattern(
        "<?php\nfunction findMe($a, $b) {\n    return $a + $b;\n}\n",
        "php",
        "function $NAME($a, $b) { $$$BODY }",
    );
    assert_eq!(matches.len(), 1, "PHP matched {} times, expected 1", matches.len());
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
    let call_matches = run_pattern(
        "<?php\nfindMe($a, $b);\n",
        "php",
        "findMe($ARG1, $ARG2)",
    );
    assert_eq!(
        call_matches.len(),
        1,
        "PHP call matched {} times, expected 1",
        call_matches.len()
    );
}

#[test]
fn bash_pattern_matches_a_function_with_bare_word_name_position() {
    let matches = run_pattern(
        "findMe() {\n  echo hi\n}\n",
        "sh",
        "$NAME() {\n  $$$BODY\n}",
    );
    assert_eq!(matches.len(), 1, "Bash matched {} times, expected 1", matches.len());
    assert_eq!(
        matches[0].metavars.get("NAME").map(Vec::as_slice),
        Some(&["findMe".to_string()][..])
    );
    assert_eq!(
        matches[0].metavars.get("BODY").map(Vec::as_slice),
        Some(&["echo hi".to_string()][..])
    );
}
