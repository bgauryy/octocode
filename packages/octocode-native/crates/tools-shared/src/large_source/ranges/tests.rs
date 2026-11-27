//! The note a ranges page that ends inside a range writes.
use super::cut_note;

/// A page ending inside lines whose rest no page reads names those lines and
/// says so; with later ranges next reads those, and without any the page writes
/// no note (review B33 r5).
#[test]
fn a_withheld_rest_is_named_and_never_promised() {
    let withheld = cut_note(4, 7, false, true).expect("later ranges are read");
    assert!(withheld.contains("lines 4-7"), "{withheld}");
    assert!(withheld.contains("whose rest no page reads"), "{withheld}");
    assert!(
        withheld.contains("next reads the other ranges"),
        "{withheld}"
    );
    assert_eq!(cut_note(4, 7, false, false), None);
    for after in [true, false] {
        let continued = cut_note(4, 7, true, after).expect("the rest is read");
        assert!(continued.contains("Lines 4-7"), "{continued}");
        assert!(!continued.contains("no page reads"), "{continued}");
    }
}
