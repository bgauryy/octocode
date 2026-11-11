//! What one judgment of a source-code text costs: each of its lexical gates
//! is read once however many passes ask, and a text no pattern is left to
//! judge is not lexed at all.

use super::*;

/// The comment-gate scans one `judge` makes on this thread.
fn gate_scans(judge: impl FnOnce()) -> usize {
    let before = code_syntax::gate_scans();
    judge();
    code_syntax::gate_scans() - before
}

/// A comment that holds a credential, which the patterns that read comments
/// find only through the text's syntax: the gate that says a comment may
/// hold one is read when the patterns are picked and again by no pass.
#[test]
fn a_judgment_lexes_its_source_for_the_comment_gate_once() {
    let content = concat!(
        "fn f() {\n",
        "    let a = 1; // password: Zq7xK2mPlanted9\n",
        "    // DB_PASSWORD=Zq7xK2mPlanted9xx\n",
        "    let b = \"token: q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc\";\n",
        "}\n",
    );
    let mut result = None;
    let scans = gate_scans(|| result = redact(content, Scope::File("a.rs")));
    let result = result.expect("the comment's credential is hidden");
    assert!(!result.sanitized.contains("Zq7xK2mPlanted9"));
    assert_eq!(scans, 1, "one lexical scan per text");
}

/// A text with a quoted credential line and no comment that may hold one:
/// the comment gate says no once, and the passes that ask it again (the
/// comment lines of each pattern that reads them) are answered from that.
#[test]
fn a_judgment_that_finds_no_comment_gate_asks_it_once() {
    let content = concat!(
        "fn f() {\n",
        "    let b = \"token: q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc\";\n",
        "    let c = 2;\n",
        "}\n",
    );
    let grammar = code_syntax::grammar_of("a.rs").expect("a Rust grammar");
    assert!(!code_syntax::has_credential_assignment(content, grammar));
    assert!(code_syntax::has_quoted_credential_assignment(content));
    let mut result = None;
    let scans = gate_scans(|| result = redact(content, Scope::File("a.rs")));
    assert!(
        result.is_some_and(|result| !result.sanitized.contains("q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc")),
        "the literal's credential is hidden"
    );
    assert_eq!(scans, 1, "one lexical scan per text");
}

/// A leaf cut from a source judged whole at its own path has no pattern
/// left to apply: nothing lexes it, whatever its text holds, and nothing is
/// replaced, as before.
#[test]
fn a_judged_leaf_with_no_pattern_left_is_not_lexed() {
    let leaf = concat!(
        "    let a = 1; // password: Zq7xK2mPlanted9\n",
        "    let b = \"token: q8Zr4LmT2vXw9KpN7sYd3HfJ6gBc\";\n",
    );
    let scope = Scope::Judged {
        leaf: Some("src/a.rs"),
        source: "src/a.rs",
    };
    assert!(
        applicable_patterns(leaf, scope).is_empty(),
        "the whole pass applied every pattern the leaf net applies"
    );
    let mut result = None;
    let scans = gate_scans(|| result = redact(leaf, scope).map(|result| result.sanitized));
    assert_eq!(result, None);
    assert_eq!(scans, 0, "no pattern asks the gate");
    // The same leaf with no judgment behind it is lexed and judged.
    let alone = gate_scans(|| {
        assert!(redact(leaf, Scope::Leaf("src/a.rs")).is_some());
    });
    assert_eq!(alone, 1);
}

/// A text none of whose applicable patterns reads comments is not lexed for
/// a gate no pattern asks.
#[test]
fn a_text_no_comment_reading_pattern_applies_to_is_not_lexed() {
    let content = format!("fn f() {{ let t = \"{FAKE_GH_TOKEN}\"; }}\n");
    let scope = Scope::File("a.rs");
    assert!(
        !applicable_patterns(&content, scope)
            .iter()
            .any(|idx| READS_COMMENT_SYNTAX[idx]),
        "no pattern that reads comments applies"
    );
    let mut result = None;
    let scans = gate_scans(|| result = redact(&content, scope));
    assert!(result.is_some_and(|result| !result.sanitized.contains(FAKE_GH_TOKEN)));
    assert_eq!(scans, 0);
}
