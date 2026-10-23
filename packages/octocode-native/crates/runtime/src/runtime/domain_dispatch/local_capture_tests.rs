use super::*;

/// A missing local resource keeps the direct read's recovery: the tree,
/// the stem listing and the read of its one close sibling.
#[test]
fn a_missing_path_keeps_the_readers_leads() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("main.rs"), "fn main() {}\n").expect("write");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let query = serde_json::from_value(serde_json::json!({"path":"mains.rs","fullContent":true}))
        .expect("query");
    let Err(failed) = crate::tools::local_fetch::capture_text(
        &query,
        &paths,
        &crate::execution::cancel::NeverCancel,
    ) else {
        panic!("a missing path is no capture");
    };
    let error = local_capture_failure(failed);
    assert_eq!(error.code, ErrorCode::PathNotFound);
    let names: Vec<&str> = error.leads.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["viewTree", "findFile", "read"]);
    assert_eq!(error.leads[2].1["query"]["queries"][0]["path"], "main.rs");
}
