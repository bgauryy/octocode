use super::{glued_regex, respelled, split_capture_name};

/// Non-ASCII text beside a sigil splits on characters, never inside one.
#[test]
fn non_ascii_text_beside_a_sigil_splits_on_characters() {
    assert_eq!(split_capture_name("Xü"), ("X", "ü"));
    assert_eq!(split_capture_name("XY"), ("XY", ""));
    assert_eq!(glued_regex("é_$Xü").as_deref(), Some("^é_.+ü$"));
    assert_eq!(respelled("é$x.f($x)ü", "$x", "$X"), "é$x.f($X)ü");
}

#[test]
fn a_respelling_replaces_only_whole_tokens() {
    assert_eq!(respelled("$x.f($xy, $x)", "$x", "$X"), "$X.f($xy, $X)");
    assert_eq!(respelled("a$x $$x", "$x", "$X"), "a$x $$x");
}
