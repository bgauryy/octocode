#[test]
fn a_path_folds_to_a_rootless_slash_path() {
    for (path, folded) in [
        ("a/./b/../c", "a/c"),
        ("/x", "x"),
        ("/a/../../b", "b"),
        ("../a", "a"),
        ("a/b/", "a/b"),
        ("", ""),
        ("/", ""),
    ] {
        assert_eq!(super::normalize(path), folded, "{path}");
    }
}

/// On Unix `\` is a file-name byte: a walked `f\g.ts` keeps its own key,
/// apart from `f/g.ts`. A JS import specifier is URL-like, so its form
/// folds `\` as a separator.
#[test]
fn a_walked_backslash_is_a_name_byte_and_a_specifier_backslash_a_separator() {
    if cfg!(unix) {
        assert_eq!(super::normalize("f\\g.ts"), "f\\g.ts");
    }
    assert_eq!(
        crate::tools::ast_graph::paths::normalize_specifier("a\\b\\..\\c"),
        "a/c"
    );
}
