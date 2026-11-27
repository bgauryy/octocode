//! A kept record answers a listing only at the `modified` stamp it lists.
use super::same_stamp;

#[test]
fn a_stamp_is_the_listed_one_only_when_it_adds_fractional_digits() {
    let listed = "2026-09-10T03:50:56.951750Z";
    assert!(same_stamp(listed, listed));
    assert!(same_stamp(listed, "2026-09-10T03:50:56.951750105Z"));
    for other in [
        "2026-09-10T03:50:56.951751105Z",
        "2026-09-10T03:50:57.951750105Z",
        "2026-09-10T03:50:56.951750105+01:00",
        "2026-09-10T03:50:56.951750x05Z",
        "2026-09-10T03:50:56.95175Z",
    ] {
        assert!(!same_stamp(listed, other), "{other}");
    }
    // Whole seconds: more digits would name another second.
    assert!(!same_stamp("2026-09-10T03:50:5Z", "2026-09-10T03:50:56Z"));
    assert!(!same_stamp("m1", "m2"));
}
