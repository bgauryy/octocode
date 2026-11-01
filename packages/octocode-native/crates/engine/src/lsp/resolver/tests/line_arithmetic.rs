//! Line hints and signed line distances never wrap.
use crate::lsp::resolver::{hinted_line, line_delta};

#[test]
fn hinted_lines_are_zero_based_and_floor_at_zero() {
    assert_eq!(hinted_line(0), 0);
    assert_eq!(hinted_line(1), 0);
    assert_eq!(hinted_line(7), 6);
    assert_eq!(
        hinted_line(u32::MAX),
        usize::try_from(u32::MAX - 1).unwrap()
    );
}

#[test]
fn line_deltas_are_signed_and_saturate() {
    assert_eq!(line_delta(4, 9), 5);
    assert_eq!(line_delta(9, 4), -5);
    assert_eq!(line_delta(3, 3), 0);
    assert_eq!(line_delta(0, usize::MAX), i32::MAX);
    assert_eq!(line_delta(usize::MAX, 0), i32::MIN);
}

/// A hint past `i32::MAX` once wrapped negative; it now reads as out of range.
#[test]
fn a_huge_line_hint_is_out_of_range() {
    let error = super::super::line_scan::scan_near_line(&["fn a() {}"], "a", u32::MAX, 0)
        .expect_err("out of range");
    assert!(error.to_string().contains("out of range"), "{error}");
}
