use super::ContentsEntry;
use octocode_test_support::json_fixture;

fn entry(value: serde_json::Value) -> ContentsEntry {
    serde_json::from_value(value).expect("entry")
}

/// A gitlink with no `.gitmodules` entry is listed as an empty `file`
/// with no object URL at all; a real blob, even an empty one, always
/// has its `/git/blobs/` URL.
#[test]
fn a_file_entry_without_an_object_url_is_a_submodule() {
    let gitlink = entry(json_fixture(
        r#"{
            "name": "vendored",
            "path": "vendored",
            "type": "file",
            "size": 0,
            "git_url": null
        }"#,
    ));
    assert!(gitlink.is_submodule());
    let empty_blob = entry(json_fixture(
        r#"{
            "name": ".keep",
            "path": ".keep",
            "type": "file",
            "size": 0,
            "git_url": "https://api.github.com/repos/o/r/git/blobs/e69de29"
        }"#,
    ));
    assert!(!empty_blob.is_submodule());
    let linked = entry(json_fixture(
        r#"{
            "name": "m",
            "path": "m",
            "type": "file",
            "size": 0,
            "git_url": "https://api.github.com/repos/x/y/git/trees/abc"
        }"#,
    ));
    assert!(linked.is_submodule());
}
