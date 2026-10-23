//! Recovered syntax that could hide a match.

use super::*;

/// A macro-attribute line (`JSON_HEDLEY_NON_NULL(2)`) makes tree-sitter read
/// the method after it as part of one garbled declaration: the call inside
/// its body is not a `call_expression`, so a call pattern misses it.
const HIDDEN_CALL: &str = "class lexer {\n    JSON_HEDLEY_NON_NULL(2)\n    token_type scan_literal(const char_type* literal_text, const std::size_t length,\n                            token_type return_type)\n    {\n        JSON_ASSERT(char_traits<char_type>::to_char_type(current) == literal_text[0]);\n        return return_type;\n    }\n};\nint clean() { JSON_ASSERT(1); return 2; }\n";

/// The same macro-attribute garble, but the body after it parses cleanly.
const LOCAL_ERROR: &str = "struct S {\n    JSON_HEDLEY_NON_NULL(2)\n    int scan(const char* a, int b)\n    {\n        JSON_ASSERT(a != nullptr);\n        return b;\n    }\n};\n";

/// Whether the matcher of `pattern` reports `src` as parsed with recovered
/// syntax that could hide one of its matches, and how many it found.
fn recovery_of(src: &str, ext: &str, query: (Option<&str>, Option<&Value>)) -> (bool, usize) {
    let matcher = compile_matcher(
        &lang(ext),
        &StructuralQuery::new(query.0, query.1).expect("query"),
    )
    .expect("compile");
    let mut found = Found::new(crate::structural::types::StructuralDemand::Matches);
    matcher(src, &mut found).expect("complete execution");
    (found.recovered(), found.into_matches().len())
}

/// A recovered parse is reported when recovered syntax spans the text the
/// pattern needs: the call a pattern would match can sit in a garbled
/// declaration, and the matcher cannot find what the tree does not hold.
#[test]
fn a_match_names_recovered_syntax_that_could_hide_a_match() {
    let (recovered, found) = recovery_of(HIDDEN_CALL, "cpp", (Some("JSON_ASSERT($$$A)"), None));
    assert_eq!((recovered, found), (true, 1), "the hidden call is the miss");
    let rule = json!({"pattern":"JSON_ASSERT($$$A)"});
    assert!(recovery_of(HIDDEN_CALL, "cpp", (None, Some(&rule))).0);
}

/// Recovered syntax elsewhere, a clean parse around the anchor, an anchor in
/// a comment or a string, and a clean file are not a gap.
#[test]
fn recovered_syntax_away_from_the_anchor_is_no_gap() {
    let pattern = (Some("JSON_ASSERT($$$A)"), None);
    assert_eq!(recovery_of(LOCAL_ERROR, "cpp", pattern), (false, 1));
    let elsewhere = format!(
        "{LOCAL_ERROR}// JSON_ASSERT(x) is documented here\nconst char* s = \"JSON_ASSERT(y)\";\n"
    );
    assert_eq!(recovery_of(&elsewhere, "cpp", pattern), (false, 1));
    assert_eq!(
        recovery_of(
            "int clean() { JSON_ASSERT(1); return 2; }\n",
            "cpp",
            pattern
        ),
        (false, 1)
    );
}

/// A query with no literal anchor (a kind, a bare call shape) cannot say
/// where a match would be, so any recovered syntax in the file could hide it.
#[test]
fn an_anchorless_query_reports_every_recovered_parse() {
    assert!(recovery_of(LOCAL_ERROR, "cpp", (Some("$F($$$A)"), None)).0);
    assert!(
        recovery_of(
            LOCAL_ERROR,
            "cpp",
            (None, Some(&json!({"kind":"call_expression"})))
        )
        .0
    );
    assert!(
        !recovery_of(
            "int f() { return g(1); }\n",
            "cpp",
            (None, Some(&json!({"kind":"call_expression"})))
        )
        .0
    );
}
