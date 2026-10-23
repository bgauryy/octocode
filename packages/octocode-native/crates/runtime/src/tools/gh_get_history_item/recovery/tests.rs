use super::*;

/// A missing pull request, issue, or commit SHA lists the repository's
/// recent ones; another failure kind (rate limit, auth) gets no lead.
#[test]
fn missing_items_list_where_the_right_one_is() {
    for (operation, name) in [
        ("pullRequest", "findPullRequest"),
        ("issue", "findIssue"),
        ("commit", "findCommit"),
    ] {
        let query =
            json!({"operation":operation,"owner":"o","repo":"r","number":9,"ref":"0000000"});
        let next = recovery(ProviderErrorKind::NotFound, None, &query).expect(operation);
        assert_eq!(next[name]["tool"], "ghSearchHistory", "{next}");
        assert_eq!(
            next[name]["query"]["queries"][0],
            json!({"operation":operation,"owner":"o","repo":"r"}),
            "{next}"
        );
        assert!(recovery(ProviderErrorKind::RateLimited, None, &query).is_none());
    }
    let commit = json!({"operation":"commit","owner":"o","repo":"r","ref":"0000000000000000000000000000000000000000"});
    let next = recovery(
        ProviderErrorKind::NotFound,
        Some(ProviderErrorReason::RefNotFound),
        &commit,
    )
    .expect("commit");
    assert!(next.get("findCommit").is_some(), "{next}");
}

/// [`super::super::run`] states a missing ref exactly as the runtime
/// dispatch did (tests/runtime_gh_default_branch.rs): the message names
/// the default branch, `atDefaultBranch` retries there, `viewRefs` stays;
/// a comparison names its missing side, and offers no retry that would
/// compare the default branch with itself.
#[tokio::test]
async fn a_missing_ref_is_recovered_inside_the_tool() {
    use crate::security::scan::Passthrough;
    use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"default_branch":"master","full_name":"a/b","private":false,"archived":false}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: main"})),
        )
        .mount(&server)
        .await;
    for resolved in ["next", "master"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{resolved}")))
            .respond_with(ResponseTemplate::new(200).set_body_string("a".repeat(40)))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/compare/"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let fail = async |row: Value| {
        let request = super::super::HistoryItemRequest::from_row(&row).expect("row");
        super::super::run(&provider, &request, &row, &default_context(), &Passthrough)
            .await
            .expect_err("missing ref")
    };
    let lead = |failure: &GhFailure, name: &str| {
        failure
            .next
            .as_deref()
            .and_then(|next| next.get(name))
            .cloned()
            .unwrap_or(Value::Null)
    };

    let commit = fail(json!({"operation":"commit","owner":"a","repo":"b","ref":"main"})).await;
    assert_eq!(commit.error.kind, ProviderErrorKind::NotFound);
    assert!(commit.message.contains("\"main\""), "{}", commit.message);
    assert!(
        commit
            .message
            .ends_with("; the default branch is \"master\"."),
        "{}",
        commit.message
    );
    let retry = lead(&commit, "atDefaultBranch");
    assert_eq!(retry["tool"], "ghGetHistoryItem");
    assert_eq!(retry["query"]["queries"][0]["ref"], "master");
    assert_eq!(lead(&commit, "viewRefs")["tool"], "ghStructure");

    let compare =
        fail(json!({"operation":"compare","owner":"a","repo":"b","base":"main","head":"next"}))
            .await;
    assert_eq!(
        compare.message,
        "Cannot compare main...next in a/b: \"main\" (base) not found; the default branch is \"master\"."
    );
    let row = &lead(&compare, "atDefaultBranch")["query"]["queries"][0];
    assert_eq!(
        (&row["base"], &row["head"]),
        (&json!("master"), &json!("next"))
    );
    assert_eq!(lead(&compare, "viewRefs")["tool"], "ghStructure");

    let same =
        fail(json!({"operation":"compare","owner":"a","repo":"b","base":"main","head":"master"}))
            .await;
    assert!(lead(&same, "atDefaultBranch").is_null());
    assert!(!lead(&same, "viewRefs").is_null());
}

/// A missing ref in a repository GitHub redirected to a new name states
/// the rename once and leads (`atDefaultBranch`, `viewRefs`) under the
/// canonical name; a repository that was never redirected states none.
#[tokio::test]
async fn a_missing_ref_of_a_renamed_repository_names_the_rename() {
    use crate::security::scan::Passthrough;
    use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/api/v3/repositories/1/commits/main", server.uri()),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/main"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: main"})),
        )
        .mount(&server)
        .await;
    for (name, full) in [("a/b", "c/d"), ("c/d", "c/d")] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"default_branch":"master","full_name":full,"private":false,"archived":false}),
            ))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let row = json!({"operation":"commit","owner":"a","repo":"b","ref":"main"});
    let request = super::super::HistoryItemRequest::from_row(&row).expect("row");
    let failure = super::super::run(&provider, &request, &row, &default_context(), &Passthrough)
        .await
        .expect_err("missing ref");
    assert_eq!(
        failure
            .message
            .matches("Repository a/b was renamed to c/d; leads use the canonical name.")
            .count(),
        1,
        "{}",
        failure.message
    );
    assert!(
        failure
            .message
            .contains("the default branch is \"master\"."),
        "{}",
        failure.message
    );
    let next = failure.next.as_deref().expect("leads");
    let named = |lead: &str| {
        let query = &next[lead]["query"]["queries"][0];
        (query["owner"].clone(), query["repo"].clone())
    };
    assert_eq!(named("atDefaultBranch"), (json!("c"), json!("d")), "{next}");
    assert_eq!(named("viewRefs"), (json!("c"), json!("d")), "{next}");
}
