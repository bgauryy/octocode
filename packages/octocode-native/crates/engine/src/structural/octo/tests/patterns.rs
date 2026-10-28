//! Pattern matching: captures, probes, immunity and binding rollback.

use super::*;

#[test]
fn ast_audit_special_pattern_preserves_repeated_capture_equality() {
    let source = "pair(left, right);\npair(same, same);\n";
    let direct = run_pattern(source, "ts", "pair($X, $X)");
    let rule = run_rule(source, "ts", json!({"pattern":"pair($X, $X)"}));
    assert_eq!(direct.len(), 1);
    assert_eq!(direct[0].text, "pair(same, same)");
    assert_eq!(direct[0].metavars, rule[0].metavars);
    let direct_range = &direct[0].metavar_ranges["X"][0];
    let rule_range = &rule[0].metavar_ranges["X"][0];
    assert_eq!(direct_range.line, rule_range.line);
    assert_eq!(direct_range.column, rule_range.column);
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
fn capture_env_rollback_restores_inserts_and_replacements() {
    let mut env = CaptureEnv::default();
    assert!(env.capture_one("A", "a", (0, 0, 0, 1, 0, 0, 0)));
    env.capture_replace(SECONDARY_CAPTURE, "first".to_owned(), (0, 0, 0, 5, 0, 0, 0));
    let checkpoint = env.checkpoint();
    assert!(env.capture_one("B", "b", (1, 0, 1, 1, 0, 0, 0)));
    assert!(env.capture_many("C", ["c1", "c2"].into_iter(), || vec![
        (2, 0, 2, 1, 0, 0, 0);
        2
    ]));
    env.capture_replace(
        SECONDARY_CAPTURE,
        "second".to_owned(),
        (3, 0, 3, 6, 0, 0, 0),
    );
    // A backreference check that fails binds nothing.
    assert!(!env.capture_one("A", "other", (4, 0, 4, 5, 0, 0, 0)));
    assert_eq!(env.undo_len(), 5);
    env.rollback(checkpoint);
    assert_eq!(env.undo_len(), 2);
    assert!(env.capture_one("A", "a", (0, 0, 0, 1, 0, 0, 0)), "A kept");
    let (values, ranges) = env.into_maps();
    assert_eq!(values.keys().collect::<Vec<_>>(), ["A"]);
    assert_eq!(ranges["A"], vec![(0, 0, 0, 1, 0, 0, 0)]);
}

#[test]
fn failed_rule_branches_leave_no_bindings_behind() {
    // The first `any` alternative binds X before failing on `bar`; its binding
    // must be rolled back, not leak into the match from the second one.
    let matches = run_rule(
        "foo(a, b);\n",
        "ts",
        json!({"any":[{"pattern":"foo($X, bar)"},{"pattern":"foo($Y, $Z)"}]}),
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
        json!({"all":[{"pattern":"foo($X, $W)"},{"not":{"pattern":"foo($Q, c)"}}]}),
    );
    assert_eq!(matches.len(), 1);
    assert!(!matches[0].metavars.contains_key("Q"));
    assert!(
        run_rule(
            "foo(a, b);\n",
            "ts",
            json!({"all":[{"pattern":"foo($X, $W)"},{"pattern":"foo($W, $X)"}]}),
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

/// A pattern whose root does not parse is rejected, naming the token the
/// parser could not place with its `$` sigil as written.
#[test]
fn unparsed_pattern_names_the_unplaced_token() {
    for (ext, pattern) in [
        ("rs", "fn $NAME() $$$BODY"),
        ("py", "def $F($X) $$$BODY"),
        ("js", "function $F() $$$BODY"),
    ] {
        let query = StructuralQuery::new(Some(pattern), None).expect("query");
        let error = compile_matcher(&lang(ext), &query)
            .err()
            .expect("a function without its body does not parse")
            .to_string();
        assert!(
            error.contains("pattern parsed with syntax errors near `$$$BODY`"),
            "{ext}: {error}"
        );
    }
}

#[test]
fn special_patterns_share_their_source_template() {
    let source = "const tree = <main><Box /><Box /><Box /></main>;";
    let found = run_pattern(source, "tsx", "<$TAG>");
    assert!(found.len() >= 3);
    for matched in &found[1..] {
        assert!(std::sync::Arc::ptr_eq(
            &found[0].source_templates,
            &matched.source_templates
        ));
    }
}
