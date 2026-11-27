use super::number_content;

/// `content` has one shape on every page: a walk rebuilds the whole answer.
#[test]
fn a_row_has_no_final_line_ending() {
    assert_eq!(number_content("a\n", 3), "3\ta");
    assert_eq!(number_content("a", 3), "3\ta");
    assert_eq!(number_content("a\nb\n", 3), "3\ta\n4\tb");
}
