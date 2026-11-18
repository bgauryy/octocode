use super::*;

/// `secret` at bytes 10..20 shown as an 8-byte placeholder at 10..18.
const RUNS: [[i64; 4]; 1] = [[10, 20, 10, 18]];

#[test]
fn a_range_keeps_its_place_before_a_replaced_run_and_moves_past_it() {
    assert_eq!(
        shown_range(&RUNS, ByteRange::new(2, 8)),
        ShownRange::Clear(ByteRange::new(2, 8))
    );
    assert_eq!(
        shown_range(&RUNS, ByteRange::new(4, 10)),
        ShownRange::Clear(ByteRange::new(4, 10))
    );
    assert_eq!(
        shown_range(&RUNS, ByteRange::new(25, 30)),
        ShownRange::Clear(ByteRange::new(23, 28))
    );
}

#[test]
fn a_range_holding_a_replaced_run_shows_its_placeholder_and_one_cut_by_it_has_none() {
    assert_eq!(
        shown_range(&RUNS, ByteRange::new(5, 22)),
        ShownRange::Replaced(ByteRange::new(5, 20))
    );
    assert_eq!(shown_range(&RUNS, ByteRange::new(12, 22)), ShownRange::Cut);
    assert_eq!(shown_range(&RUNS, ByteRange::new(5, 15)), ShownRange::Cut);
}
