//! Reads of a renamed repository, and a path the tree lists but the read
//! could not find.
use super::*;
use octocode_test_support::json_fixture;
use octocode_tools_shared::gh_shared::test_support::{assert_leads_canonical, mount_renamed};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A read GitHub answered through a rename redirect states the rename once
/// and names the repository as it is now, on the row and in every lead (a
/// match that found nothing leads to the repository's search and history).
#[tokio::test]
async fn a_read_of_a_renamed_repository_leads_canonically() {
    let server = MockServer::builder().start().await;
    mount_renamed(&server).await;
    mount_json(
        &server,
        "/api/v3/repositories/1/contents/src%2Flib.rs",
        200,
        json!({"type":"file", "encoding":"base64", "content":STANDARD.encode("fn a() {}\n")}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA,"matchString":["absent"]}),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    let data = &read.data;
    assert_eq!(
        (data["owner"].as_str(), data["repo"].as_str()),
        (Some("c"), Some("d")),
        "{data}"
    );
    assert_eq!(
        data["warnings"][0],
        octocode_tools_shared::gh_shared::renamed_warning("a", "b", &("c".into(), "d".into())),
        "{data}"
    );
    assert_leads_canonical(data);
}

/// The tree lists the very path the read could not find: no letter case
/// differs, so the row says what was observed and leads to the listing that
/// answered, never to the failed read again.
#[tokio::test]
async fn a_listed_path_that_did_not_read_is_not_a_case_difference() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(r#"{"default_branch": "main", "full_name": "a/b"}"#),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        404,
        json_fixture(r#"{"message": "Not Found"}"#),
    )
    .await;
    octocode_test_support::mock_http::mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        SHA,
        "src",
        &json_fixture(r#"[{"name": "lib.rs", "path": "src/lib.rs", "type": "file"}]"#),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let asked = json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA});
    let Err(failure) = run_default(&provider, asked.clone()).await else {
        panic!("a missing path is a failure")
    };
    assert_eq!(
        failure.hints,
        vec![errors::LISTED_NOT_READ_HINT.to_owned()],
        "{:?}",
        failure.calls
    );
    let next = failure.calls.expect("recovery lead");
    assert!(
        next.get("read").is_none(),
        "the failed read repeats: {next}"
    );
    assert_eq!(
        next["viewTree"]["input"]["queries"][0],
        json!({"operation":"tree","owner":"a","repo":"b","path":"src","ref":SHA}),
        "{next}"
    );
}

/// The same under a renamed repository's old name: the read is led to
/// under the canonical name (another call than the one that failed), and
/// the hint names what failed instead of a case difference.
#[tokio::test]
async fn a_listed_path_of_a_renamed_repository_is_read_canonically() {
    let server = MockServer::builder().start().await;
    mount_renamed(&server).await;
    mount_json(
        &server,
        "/api/v3/repositories/1/contents/src%2Flib.rs",
        404,
        json_fixture(r#"{"message": "Not Found"}"#),
    )
    .await;
    octocode_test_support::mock_http::mount_folder_tree(
        &server,
        "/api/v3/repos/c/d",
        SHA,
        "src",
        &json_fixture(r#"[{"name": "lib.rs", "path": "src/lib.rs", "type": "file"}]"#),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let Err(failure) = run_default(
        &provider,
        json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA}),
    )
    .await
    else {
        panic!("a missing path is a failure")
    };
    assert_eq!(
        failure.hints,
        vec![errors::OLD_NAME_NOT_READ_HINT.to_owned()],
        "{:?}",
        failure.calls
    );
    let next = failure.calls.expect("recovery lead");
    assert_eq!(
        next["read"]["input"]["queries"][0],
        json!({"owner":"c","repo":"d","path":"src/lib.rs","ref":SHA}),
        "{next}"
    );
}
