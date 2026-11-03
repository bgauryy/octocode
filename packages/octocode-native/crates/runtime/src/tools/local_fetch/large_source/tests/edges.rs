use super::*;

/// The scan-edge warnings `patterns` earns reading `text`.
fn edge_warnings(text: &str, patterns: &[&str], engine: &str) -> Vec<String> {
    let file = temp(text.as_bytes());
    let read = fetch(
        &file,
        &query(serde_json::json!({
            "path": file.path(),
            "matchString": patterns,
            "regexEngine": engine,
        })),
        None,
    );
    assert_eq!(read.error_code, None, "{read:?}");
    read.warnings
        .into_iter()
        .filter(|warning| warning.contains("scan edge"))
        .collect()
}

/// Short lines over several scan windows: every edge falls between lines.
fn short_lines() -> String {
    "fn name\nx\n".repeat(300_000)
}

/// One line far longer than the scan window: its edges fall inside it.
fn giant_line() -> String {
    format!("{}nee{}dle\n", "x".repeat(2_000_000), "e".repeat(1_000_000))
}

/// A pattern that stays on one line cannot be cut at an edge between lines.
#[test]
fn a_single_line_regex_over_short_lines_is_not_disclosed() {
    let warnings = edge_warnings(&short_lines(), &[r"fn \w+"], "rust");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A pattern that can span lines may be cut at an edge between lines.
#[test]
fn a_regex_that_spans_lines_is_disclosed_at_line_edges() {
    for pattern in [r"(?s)fn.*zzz", r"fn\s+zzz"] {
        let warnings = edge_warnings(&short_lines(), &[pattern], "rust");
        assert_eq!(warnings.len(), 1, "{pattern}: {warnings:?}");
        assert!(warnings[0].contains("can span lines"), "{warnings:?}");
    }
}

/// An unbounded regex may be cut at an edge inside a line longer than the
/// lookahead, whether or not it can span lines.
#[test]
fn an_unbounded_regex_on_a_giant_line_is_disclosed_at_mid_line_edges() {
    let warnings = edge_warnings(&giant_line(), &[r"nee+dle"], "rust");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("line longer"), "{warnings:?}");
    assert!(!warnings[0].contains("span lines"), "{warnings:?}");
}

/// A match no longer than the lookahead is never cut, and a literal's longest
/// match always fits it.
#[test]
fn bounded_regexes_and_literals_are_never_disclosed() {
    for source in [short_lines(), giant_line()] {
        for (patterns, engine) in [
            (vec![r"fn{1,3}ame"], "rust"),
            (vec![r"(?s)fn.{0,5}x"], "rust"),
            (vec!["nee"], "literal"),
            (vec!["fn name\nx"], "literal"),
        ] {
            let warnings = edge_warnings(&source, &patterns, engine);
            assert!(warnings.is_empty(), "{patterns:?}: {warnings:?}");
        }
    }
}
