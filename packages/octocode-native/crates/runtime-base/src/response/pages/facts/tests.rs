use super::*;
use octocode_test_support::json_fixture;

#[test]
fn list_pages_state_offset_length_and_total() {
    assert_eq!(
        PageFacts::nth(2, 10, 25).to_value(),
        json_fixture(
            r#"{
                "unit": "items",
                "offset": 10,
                "length": 10,
                "total": 25,
                "hasMore": true
            }"#
        )
    );
    assert_eq!(
        PageFacts::nth(3, 10, 25).with_unit("files").to_value(),
        json_fixture(
            r#"{
                "unit": "files",
                "offset": 20,
                "length": 5,
                "total": 25,
                "hasMore": false
            }"#
        )
    );
    assert_eq!(
        PageFacts::open(10, 10, false).to_value(),
        json_fixture(
            r#"{
                "unit": "items",
                "offset": 10,
                "length": 10,
                "hasMore": false
            }"#
        )
    );
    assert_eq!(
        PageFacts::unshown(4).to_value(),
        json_fixture(
            r#"{
                "unit": "items",
                "offset": 0,
                "length": 0,
                "total": 4,
                "hasMore": true
            }"#
        )
    );
}

#[test]
fn a_count_page_past_the_end_is_empty_at_the_end_and_flagged() {
    let past = PageFacts::nth(4, 10, 25);
    assert!(past.is_past_end());
    assert_eq!(past.to_value()["offset"], 25);
    assert_eq!(past.to_value()["length"], 0);
    assert!(!PageFacts::nth(1, 10, 0).is_past_end(), "page 1 of nothing");
}

#[test]
fn source_spans_preserve_coordinates_and_stop_at_the_end() {
    let first = SpanFacts::window("lines", 0, 10, 25);
    assert_eq!(first.next_offset(), Some(10));
    assert_eq!(first.to_value()["length"], 10);
    let last = SpanFacts::window("lines", 20, 10, 25);
    assert!(!last.has_more());
    assert_eq!(last.to_value()["length"], 5);
    assert_eq!(last.next_offset(), None);
    assert_eq!(
        SpanFacts::window("bytes", 900, 50, 100).to_value(),
        json_fixture(
            r#"{
                "unit": "bytes",
                "offset": 100,
                "length": 0,
                "total": 100,
                "hasMore": false
            }"#
        )
    );
}

#[test]
fn a_walked_window_states_the_walk_not_its_offsets() {
    // A last page that ends before the view's total: nothing is left.
    let last = SpanFacts::window("bytes", 36_864, 3_137, 40_023).walked(false);
    assert!(!last.has_more());
    assert_eq!(last.next_offset(), None);
    assert_eq!(last.to_value()["hasMore"], false);
    // A page at the view's end with more of the walk behind it (a
    // match shown before the bytes ahead of it).
    let early = SpanFacts::window("bytes", 90, 10, 100).walked(true);
    assert_eq!(
        early.to_value(),
        json_fixture(
            r#"{
                "unit": "bytes",
                "offset": 90,
                "length": 10,
                "total": 100,
                "hasMore": true
            }"#
        )
    );
    assert_eq!(
        early.next_offset(),
        None,
        "the cursor names where it goes on"
    );
}
