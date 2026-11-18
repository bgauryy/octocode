//! Request parsing, section resolution, sanitizing and the response cap.
use super::*;
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;

/// A comparison pinned to resolved commits reads head text under the same path
/// policy as the row it continues: the opt-in that opens a credential name stays
/// open, and a sensitive directory stays withheld.
#[test]
fn a_pinned_comparison_keeps_the_rows_path_policy() {
    let root = tempfile::tempdir().expect("root");
    let opt_in = octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            allow_credential_files: true,
            ..Default::default()
        },
    )
    .expect("policy");
    let mut row = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "compare",
            "owner": "o",
            "repo": "r",
            "baseRef": "main",
            "headRef": "dev"
        }"#,
    ))
    .expect("compare row");
    row.paths = Some(std::sync::Arc::new(opt_in));
    assert!(!row.withholds_file(".env"));
    let pinned = row.with_compare_refs("a1", "b2").expect("pinned");
    assert!(!pinned.withholds_file(".env"), "the opt-in is lost");
    assert!(pinned.withholds_file(".ssh/config"));
}

/// A read with no section named is the summary: it selects none.
#[test]
fn a_summary_read_selects_no_section() {
    let query = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 1
        }"#,
    ))
    .expect("summary read");
    assert!(query.sections().is_empty());
    assert_eq!(query.sections().wire(), None);
}

/// The typed `sections` are what every handler reads; a `matchString`
/// with none named reads the patches it searches, on every operation
/// that takes one.
#[test]
fn sections_resolve_from_the_typed_query() {
    let row = |fields: Value| {
        HistoryItemRequest::from_row(super::util::merge(
            json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "o",
                    "repo": "r",
                    "number": 1
                }"#,
            ),
            fields,
        ))
        .expect("pull request row")
    };
    let read = row(json_fixture(
        r#"{
            "sections": [
                "reviews",
                "patches",
                "body",
                "comments",
                "files",
                "reviewComments"
            ],
            "include": ["src/**"],
            "fileStatus": ["modified"]
        }"#,
    ));
    // Published order on the wire, whatever order the caller named.
    assert_eq!(
        read.sections().wire(),
        Some(json_fixture(
            r#"[
                "body",
                "files",
                "patches",
                "comments",
                "reviewComments",
                "reviews"
            ]"#
        ))
    );
    assert!(!read.sections().lists_commits());
    assert_eq!(
        row(json_fixture(r#"{"matchString": ["needle"]}"#)).sections(),
        Sections::of([Section::Patches])
    );
    let commit = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "ref": "head-sha",
            "sections": ["patches"],
            "include": ["src/*.rs"]
        }"#,
    ))
    .expect("commit");
    assert_eq!(
        gh_get_history_item_query::operation(&commit),
        ItemOperation::Commit
    );
    assert!(
        HistoryItemRequest::from_row(json_fixture(
            r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "ref": "h",
                "baseRef": "b"
            }"#
        ))
        .is_err()
    );
    assert!(commit.include_diff());
    assert_eq!(
        gh_get_history_item_query::include_patterns(&commit),
        ["src/*.rs"]
    );
    let grep = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "compare",
            "owner": "o",
            "repo": "r",
            "baseRef": "a",
            "headRef": "b",
            "matchString": ["x"]
        }"#,
    ))
    .expect("compare grep");
    assert!(grep.include_diff());
    let issue = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "number": 2,
            "sections": ["comments"]
        }"#,
    ))
    .expect("issue include");
    assert_eq!(issue.sections(), Sections::of([Section::Comments]));
}

struct ReplacingScan;
impl ContentScan for ReplacingScan {
    fn sanitize(
        &self,
        text: &str,
        _path: &Path,
    ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((text.replace("secret", "[MASKED]"), Vec::new()))
    }
}

#[test]
fn missing_identity_is_rejected() {
    // The wire contract requires each operation's identity.
    for row in [
        json_fixture(r#"{"operation": "commit", "owner": "a", "repo": "b"}"#),
        json_fixture(r#"{"operation": "pullRequest", "owner": "a", "repo": "b"}"#),
        json_fixture(
            r#"{
                "operation": "compare",
                "owner": "a",
                "repo": "b",
                "baseRef": "x"
            }"#,
        ),
    ] {
        assert!(HistoryItemRequest::from_row(row).is_err());
    }
    assert!(
        HistoryItemRequest::from_row(json_fixture(
            r#"{
                "operation": "commit",
                "owner": "a",
                "repo": "b",
                "ref": "x"
            }"#
        ),)
        .is_ok()
    );
    // The wire contract rejects a blank comparison ref.
    assert!(
        HistoryItemRequest::from_row(json_fixture(
            r#"{
                "operation": "compare",
                "owner": "a",
                "repo": "b",
                "baseRef": "",
                "headRef": "x"
            }"#
        ))
        .is_err()
    );
}

#[test]
fn sanitizes_every_nested_returned_string() {
    let mut value = json_fixture(
        r#"{
            "title": "secret",
            "nested": [{"body": "a secret value"}],
            "next": {"tool": "secret-tool", "query": {"path": "secret.rs"}}
        }"#,
    );
    sanitize_provider_json(&ReplacingScan, &mut value, Path::new(SCAN_PATH))
        .expect("GitHub history test data should be valid");
    assert_eq!(value["title"], "[MASKED]");
    assert_eq!(value["nested"][0]["body"], "a [MASKED] value");
    // The executable `tool` identifier survives verbatim, but the query
    // leaves are scanned: redaction fires only on a real
    // secret, so a legitimate path is untouched while a secret is masked.
    assert_eq!(value["next"]["tool"], "secret-tool");
    assert_eq!(value["next"]["query"]["path"], "[MASKED].rs");
}

/// The transport owns rate-limit facts: a secondary limit states no
/// remaining count, so the read does not invent a spent quota (which
/// would tell an anonymous caller to log in for a limit login cannot
/// lift).
#[tokio::test]
async fn a_secondary_rate_limit_keeps_its_unknown_remaining_count() {
    use octocode_runtime_base::security::scan::Passthrough;
    use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/issues/7"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("retry-after", "60")
                .set_body_json(json_fixture(
                    r#"{"message": "You have exceeded a secondary rate limit."}"#,
                )),
        )
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let request = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "number": 7
        }"#,
    ))
    .expect("issue read");
    let error = execute(&provider, &request, &default_context(), &Passthrough)
        .await
        .expect_err("rate limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
    let rate_limit = error.rate_limit.as_deref().expect("rate-limit facts");
    assert_eq!(rate_limit.remaining, None, "{rate_limit:?}");
}

#[test]
fn normalized_response_budget_is_enforced() {
    let error = enforce_response_limit(&json_fixture(r#"{"body": "abcdefgh"}"#), 4)
        .expect_err("GitHub history operation should fail in this test");
    assert_eq!(error.kind, ProviderErrorKind::ResponseTooLarge);
    assert_eq!(
        error.message.as_ref(),
        "GitHub history item response exceeds 4 bytes"
    );
}

/// A commit that names its pull request ("(#874)") leads to reading it; one
/// that names none states no route at all: no search by SHA is guessed.
#[test]
fn a_commit_leads_to_the_pull_request_its_message_names_and_guesses_none() {
    let query = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "ref": "abc123abc123abc123abc123abc123abc123abcd"
        }"#,
    ))
    .expect("commit row");
    let lead = super::patch_hop::commit_pull_request_lead("Fix the loop (#874)", &query)
        .expect("a named pull request");
    assert_eq!(lead["tool"], "ghGetHistoryItem", "{lead}");
    assert_eq!(lead["input"]["queries"][0]["number"], 874, "{lead}");
    assert_eq!(
        lead["input"]["queries"][0]["operation"], "pullRequest",
        "{lead}"
    );
    assert_eq!(
        super::patch_hop::commit_pull_request_lead("Fix the loop", &query),
        None
    );
}
