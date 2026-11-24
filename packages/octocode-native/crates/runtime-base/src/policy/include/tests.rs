use super::*;

#[test]
fn a_glob_with_a_parent_segment_is_found() {
    let include = ["*.rs".to_owned()];
    let exclude = ["../src/*.ts", "a/../b"].map(str::to_owned);
    let warning = parent_glob_warning(&include, &exclude).expect("warning");
    assert!(
        warning.starts_with("Glob `../src/*.ts` never matches"),
        "{warning}"
    );
    assert!(reaches_above("a\\..\\b"));
    assert!(!reaches_above("..rc"));
    assert!(!reaches_above("src/..x/*.rs"));
    assert_eq!(parent_glob_warning(&["*.md".to_owned()], &[]), None);
}

/// A glob that matched only withheld files did match: the hint states how
/// many and why, never the glob-syntax advice of a real miss, and no name.
#[test]
fn a_glob_that_matched_only_withheld_files_says_so() {
    assert_eq!(glob_miss_hint(0, &[], &[]), GLOB_MISS_HINT);
    let globs = ["*.pem".to_owned()];
    for withheld in [1, 2, 40] {
        let hint = glob_miss_hint(withheld, &globs, &[]);
        assert!(
            hint.contains(&format!("matched {withheld} withheld file")),
            "{hint}"
        );
        assert!(hint.contains("path policy"), "{hint}");
        assert!(
            !hint.contains("matched no file") && !hint.contains("**/"),
            "{hint}"
        );
        assert!(hint.len() < GLOB_MISS_HINT.len(), "{hint}");
    }
}

/// A real miss shows each glob as it was read, so a list sent as one
/// comma-joined string is seen as the one glob it is.
#[test]
fn a_glob_miss_names_the_globs_as_read() {
    let joined = ["tsdb/db.go,tsdb/head.go".to_owned()];
    let hint = glob_miss_hint(0, &joined, &[]);
    assert!(hint.starts_with(GLOB_MISS_HINT), "{hint}");
    assert!(
        hint.ends_with("Globs as read: 1 include (`tsdb/db.go,tsdb/head.go`)."),
        "{hint}"
    );
    let include = ["a/*.rs", "b/*.rs"].map(str::to_owned);
    let exclude = ["**/gen/**".to_owned()];
    let hint = glob_miss_hint(0, &include, &exclude);
    assert!(
        hint.ends_with("Globs as read: 2 include (`a/*.rs`, `b/*.rs`), 1 exclude (`**/gen/**`)."),
        "{hint}"
    );
}
