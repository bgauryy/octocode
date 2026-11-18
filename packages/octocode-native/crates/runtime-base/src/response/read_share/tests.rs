use super::*;
#[test]
fn every_row_receives_a_positive_share_and_numbering_counts_utf16() {
    for rows in [1, 3, 20] {
        for budget in [1, 500, 20000] {
            assert!(row_window(ToolId::LocalFetch, budget, rows) > 0);
        }
    }
    assert_eq!(
        text_chars("😀\n", 1),
        octocode_engine::json_size::string_body_units("😀\n") + 3
    );
}
