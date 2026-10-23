//! Depth, attempt, parse and match deadlines and budgets.

use super::*;

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

/// The match deadline grows with the source: a large file (up to the 8 MiB
/// parse limit) gets one more parse budget per MiB, so it is not cut by the
/// budget a small file gets, and a small file keeps the shared budget.
#[test]
fn the_match_budget_scales_with_the_source_size() {
    const MIB: usize = 1 << 20;
    assert_eq!(match_budget(0), AST_EXECUTION_TIMEOUT);
    assert_eq!(match_budget(MIB - 1), AST_EXECUTION_TIMEOUT);
    assert_eq!(match_budget(3_600_000), AST_EXECUTION_TIMEOUT * 4);
    assert_eq!(
        match_budget(crate::signatures::MAX_PARSE_SIZE),
        AST_EXECUTION_TIMEOUT * 9
    );
}
