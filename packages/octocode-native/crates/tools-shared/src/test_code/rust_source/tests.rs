use super::may_declare_module;

/// A module has a name: an empty one declares nothing, in any text.
#[test]
fn an_empty_module_name_is_never_declared() {
    for text in ["", "mod ;", "fn a ;", "mod /* c */ ;"] {
        assert!(!may_declare_module(text, "", 0), "{text:?}");
    }
}

/// The text check never rejects a real `mod` head: comments and `r#`
/// between its tokens still name the module.
#[test]
fn a_module_head_with_comments_or_a_raw_name_is_seen() {
    for text in [
        "mod deep;",
        "mod /* c */ deep {}",
        "mod /* a */ /* b */ deep;",
        "mod // d\n    r#deep;",
        "mod r#deep;",
        "pub(crate) mod\n\tdeep;",
    ] {
        assert!(may_declare_module(text, "deep", 0), "{text}");
    }
    for text in [
        "fn deep() {}",
        "// mod\nlet deeper = 1;",
        "mod deeper;",
        "submod deep;",
    ] {
        assert!(!may_declare_module(text, "deep", 0), "{text}");
    }
}
