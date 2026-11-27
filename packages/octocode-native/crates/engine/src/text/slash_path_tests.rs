#[test]
fn a_path_reads_with_forward_slashes() {
    assert_eq!(super::slash_path(std::path::Path::new("a/b.rs")), "a/b.rs");
    #[cfg(windows)]
    assert_eq!(
        super::slash_path(std::path::Path::new("a\\b\\c.rs")),
        "a/b/c.rs"
    );
    #[cfg(not(windows))]
    assert_eq!(
        super::slash_path(std::path::Path::new("a\\b\\c.rs")),
        "a\\b\\c.rs"
    );
    assert_eq!(super::slash_path(std::path::Path::new("")), "");
}

#[test]
fn a_path_folds_dot_components_without_the_file_system() {
    for (path, folded) in [
        ("a/./b/../c", "a/c"),
        ("./a/b/..", "a"),
        ("../a", "a"),
        ("../../a/../b", "b"),
        ("/../a", "/a"),
        ("/a/b/../../..", "/"),
        ("/missing/./x/../y", "/missing/y"),
        ("", ""),
    ] {
        assert_eq!(
            super::lexical(std::path::Path::new(path)),
            std::path::PathBuf::from(folded),
            "{path}"
        );
    }
}
