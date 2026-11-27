use super::LineScope;

/// A line is kept exactly when a span holds it, edges included.
#[test]
fn a_line_is_kept_inside_its_spans_only() {
    let spans = [[2, 4], [7, 7], [10, 12]];
    for line in 0..15 {
        let inside = spans
            .iter()
            .any(|[start, end]| (*start..=*end).contains(&line));
        assert_eq!(LineScope::keeps(&spans, line), inside, "line {line}");
    }
    assert!(!LineScope::keeps(&[], 1));
}
