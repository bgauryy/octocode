use super::*;
use octocode_test_support::json_fixture;

#[test]
fn a_missing_capture_preserves_the_readers_executable_recovery() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("main.rs"), "fn main() {}\n").expect("write");
    let paths = octocode_runtime_base::test_support::workspace_policy(root.path());
    let query = serde_json::from_value(json_fixture(r#"{"path": "mains.rs"}"#)).expect("query");
    let Err(failed) = octocode_tool_local::local_fetch::capture_text(
        &query,
        &paths,
        &octocode_runtime_base::execution::cancel::NeverCancel,
    ) else {
        panic!("a missing path is no capture");
    };
    let error = local_capture_failure(failed);
    assert_eq!(error.code, ErrorCode::NotFound);
    let read = error
        .leads
        .iter()
        .map(|(_, call)| call)
        .find(|call| call["tool"] == "localFetch")
        .expect("related source read");
    assert_eq!(read["kind"], "related");
    let query = serde_json::from_value(read["input"]["queries"][0].clone()).expect("reader query");
    assert!(
        octocode_tool_local::local_fetch::capture_text(
            &query,
            &paths,
            &octocode_runtime_base::execution::cancel::NeverCancel,
        )
        .is_ok(),
        "recovery must read an existing source: {read}"
    );
}

/// What a classifier sends to its provider comes through the egress policy, so
/// `security.credentialFiles: allow` never opens a credential file to it.
#[test]
fn a_classifier_capture_stays_withheld_when_credential_files_are_allowed() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join(".env"), "A=1\n").expect("write");
    let allowed = octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            allow_credential_files: true,
            ..Default::default()
        },
    )
    .expect("policy");
    let query = serde_json::from_value(json_fixture(r#"{"path": ".env"}"#)).expect("query");
    let cancel = octocode_runtime_base::execution::cancel::NeverCancel;
    assert!(octocode_tool_local::local_fetch::capture_text(&query, &allowed, &cancel).is_ok());
    let Err(failed) = octocode_tool_local::local_fetch::capture_text(
        &query,
        &allowed.withholding_credentials(),
        &cancel,
    ) else {
        panic!("a classifier capture must not read a credential file");
    };
    assert_eq!(
        local_capture_failure(failed).code,
        ErrorCode::PathPolicyDenied
    );
}
