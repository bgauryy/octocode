use octocode_tools_shared::leads::find_file::close_name;
use octocode_tools_shared::leads::find_file::closest_name;

#[test]
fn a_mistyped_name_finds_its_one_close_sibling_file() {
    let dir = tempfile::tempdir().expect("dir");
    for name in ["executor.rs", "Readme.md", "a.rs", "b.rs"] {
        std::fs::write(dir.path().join(name), "").expect("file");
    }
    std::fs::create_dir(dir.path().join("executer")).expect("subdir");
    let close = |name| close_name(dir.path(), name);
    assert_eq!(close("exector.rs").as_deref(), Some("executor.rs"));
    assert_eq!(close("exectuor.rs").as_deref(), Some("executor.rs"));
    assert_eq!(close("README.md").as_deref(), Some("Readme.md"));
    // Two files are equally close: no guess.
    assert_eq!(close("c.rs"), None);
    // Too far, and a directory is no file to read.
    assert_eq!(close("scheduler.rs"), None);
    assert_eq!(close("executer"), None);
}

#[test]
fn any_named_candidates_find_their_one_close_name() {
    let names = || ["src", "docs", "tests"].map(String::from);
    assert_eq!(closest_name(names(), "sr").as_deref(), Some("src"));
    assert_eq!(closest_name(names(), "src"), None);
    assert_eq!(closest_name(names(), "bench"), None);
}
