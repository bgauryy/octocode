#[test]
fn a_path_folds_to_a_rootless_slash_path() {
    for (path, folded) in [
        ("a/./b/../c", "a/c"),
        ("a\\b\\..\\c", "a/c"),
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
