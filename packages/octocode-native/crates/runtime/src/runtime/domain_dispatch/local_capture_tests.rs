use super::*;

#[test]
fn a_missing_capture_preserves_the_readers_executable_recovery() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("main.rs"), "fn main() {}\n").expect("write");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let query = serde_json::from_value(serde_json::json!({"path":"mains.rs"})).expect("query");
    let Err(failed) = crate::tools::local_fetch::capture_text(
        &query,
        &paths,
        &crate::execution::cancel::NeverCancel,
    ) else {
        panic!("a missing path is no capture");
    };
    let error = local_capture_failure(failed);
    assert_eq!(error.code, ErrorCode::PathNotFound);
    let read = error
        .leads
        .iter()
        .map(|(_, call)| call)
        .find(|call| call["tool"] == "localFetch")
        .expect("related source read");
    assert_eq!(read["kind"], "related");
    let query = serde_json::from_value(read["input"]["queries"][0].clone()).expect("reader query");
    assert!(
        crate::tools::local_fetch::capture_text(
            &query,
            &paths,
            &crate::execution::cancel::NeverCancel,
        )
        .is_ok(),
        "recovery must read an existing source: {read}"
    );
}
