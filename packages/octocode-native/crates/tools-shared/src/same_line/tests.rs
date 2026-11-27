use super::{mark, repeated, take_mark};
use octocode_test_support::json_fixture;

/// `(line, text)` rows of one file in source order.
fn key<'a>(row: &'a (u64, &'a str)) -> (u64, &'a str) {
    (row.0, row.1)
}

#[test]
fn a_row_repeats_when_another_row_anywhere_in_the_result_shares_its_place_and_text() {
    let rows = [(1, "f()"), (2, "f()"), (2, "g()"), (2, "f()"), (3, "f()")];
    let whole = repeated(&rows, 0..rows.len(), key);
    assert_eq!(whole, [false, true, false, true, false]);
    // A page that holds one of the pair still reads its twin beyond it.
    assert_eq!(repeated(&rows, 0..2, key), [false, true]);
    assert_eq!(repeated(&rows, 3..5, key), [true, false]);
    assert_eq!(repeated(&rows, 2..3, key), [false]);
    assert!(repeated(&rows, 5..9, key).is_empty());
}

#[test]
fn a_mark_is_taken_once() {
    let mut row = json_fixture(r#"{"line": 2, "column": 7}"#);
    mark(&mut row);
    let fields = row.as_object_mut().expect("row");
    assert!(take_mark(fields));
    assert!(!take_mark(fields));
    assert_eq!(row, json_fixture(r#"{"line": 2, "column": 7}"#));
}
