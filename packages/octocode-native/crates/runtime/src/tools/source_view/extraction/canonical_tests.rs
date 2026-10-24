use super::*;
use serde_json::json;

fn query(fields: serde_json::Value) -> LocalFetchQuery {
    let mut row = json!({"path":"file.txt"});
    row.as_object_mut()
        .expect("row")
        .extend(fields.as_object().expect("fields").clone());
    serde_json::from_value(row).expect("canonical reader")
}

#[test]
fn matching_has_zero_default_context_and_keeps_both_explicit_sides() {
    let source = "before\nneedle\nafter\nlast\noutside\n";
    let plain = extract(&query(json!({"matchString":["needle"]})), source).expect("match");
    assert_eq!(plain.text, "needle\n");
    let contextual = extract(
        &query(json!({
            "matchString":["needle"], "contextLines":{"before":1,"after":2}
        })),
        source,
    )
    .expect("asymmetric match");
    assert_eq!(contextual.text, "before\nneedle\nafter\nlast\n");
    assert_eq!(contextual.source_lines, Some(vec![1, 2, 3, 4]));
}

#[test]
fn typed_ranges_constrain_both_hits_and_their_context() {
    let source = "needle\none\nneedle\nthree\nneedle\n";
    let result = extract(
        &query(json!({
            "matchString":["needle"], "contextLines":{"before":2,"after":2},
            "ranges":[{"startLine":2,"endLine":4}]
        })),
        source,
    )
    .expect("range-scoped match");
    assert_eq!(result.count, Some(1));
    assert_eq!(result.text, "one\nneedle\nthree\n");
    assert_eq!(result.source_lines, Some(vec![2, 3, 4]));
}

#[test]
fn matched_long_lines_keep_all_bytes_for_the_reader_pager() {
    let source = format!("{}needle{}\n", "a".repeat(3000), "z".repeat(3000));
    let result = extract(&query(json!({"matchString":["needle"]})), &source).expect("long line");
    assert_eq!(result.text, source);
    assert_eq!(result.count, Some(1));
}
