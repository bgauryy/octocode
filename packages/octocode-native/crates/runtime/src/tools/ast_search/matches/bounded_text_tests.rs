use super::rows::bounded_match;

#[test]
fn exact_source_keeps_comments_line_breaks_and_indentation() {
    let go =
        "if err != nil { // The only error can be a bad pattern.\n\treturn fmt.Errorf(\"x\")\n}";
    assert_eq!(
        bounded_match(go, go.chars().count()),
        (go.to_owned(), false)
    );
    let rust = "builder\n    .worker_threads(1) // no timer!\n    .build()\n    .unwrap()";
    assert_eq!(
        bounded_match(rust, rust.chars().count()),
        (rust.to_owned(), false)
    );
    assert_eq!(bounded_match(rust, 12).0.chars().count(), 12);
}
