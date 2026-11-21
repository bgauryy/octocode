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
    assert_eq!(glob_miss_hint(0), GLOB_MISS_HINT);
    for withheld in [1, 2, 40] {
        let hint = glob_miss_hint(withheld);
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
