use super::{Axis, Cursor};

/// Every axis parses back from its key; text axes continue an offset,
/// collections a page.
#[test]
fn every_axis_round_trips_and_names_its_cursor() {
    for axis in Axis::ALL {
        assert_eq!(Axis::parse(axis.key()), Some(axis));
        let text = axis.cursor() == Cursor::Offset;
        assert_eq!(text, axis.own_sections().is_empty(), "{axis:?}");
    }
    assert_eq!(Axis::parse("commitFiles"), None);
}
