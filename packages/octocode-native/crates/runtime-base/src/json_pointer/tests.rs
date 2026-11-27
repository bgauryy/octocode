#[test]
fn a_key_round_trips_through_its_token() {
    for key in ["a", "a/b", "~", "~1", "/~0/", ""] {
        assert_eq!(super::unescape(&super::escape(key)), key);
    }
    assert_eq!(super::escape("~/"), "~0~1");
    assert_eq!(super::of(&["a/b".to_owned(), "~".to_owned()]), "/a~1b/~0");
}
