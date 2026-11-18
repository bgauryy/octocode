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
