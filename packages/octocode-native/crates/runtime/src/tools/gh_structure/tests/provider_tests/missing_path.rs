//! A listing of a missing path: what it costs and how it is classified.
use super::*;
use octocode_github::ProviderErrorKind;
use wiremock::matchers::path_regex;

#[tokio::test]
async fn a_missing_path_is_classified_as_missing_and_located_from_its_parent_listing() {
    let server = server().await;
    mount_json(
        &server,
        &format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha": SHA, "truncated": false, "tree": [
            {"path":"README.md","type":"blob","mode":"100644","size":3},
            {"path":"docs","type":"tree","mode":"040000"},
            {"path":"docs/guide","type":"tree","mode":"040000"}
        ]}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v3/repos/a/b/contents/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"docs","path":"docs","type":"dir","size":0},
            {"name":"README.md","path":"README.md","type":"file","size":3}
        ])))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/docs",
        200,
        json!([{"name":"guide","path":"docs/guide","type":"dir","size":0}]),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let home = tempfile::tempdir().expect("home");
    let failure = run(
        &provider,
        &query(json!({"operation":"tree","owner":"a","repo":"b","path":"docs/nope/missing"})),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("missing path");
    // Classification: the path is missing (not the repository, not a file),
    // and the recovery lists the deepest directory that exists.
    assert_eq!(failure.error.kind, ProviderErrorKind::NotFound);
    assert_eq!(failure.message, "Path not found in a/b: docs/nope/missing");
    let calls = failure.calls.expect("recovery lead");
    assert_eq!(calls["viewTree"]["input"]["queries"][0]["path"], "docs");
    // Cost: the repository and its head, the tree read, the Contents read
    // (a git tree answers 404 for a submodule path too, so only Contents
    // tells a missing path from a submodule), then the listings from the
    // missing path's parent up to the directory that exists: never the
    // commit's recursive tree, whose size follows the repository's.
    let requests = server.received_requests().await.unwrap_or_default();
    let mut paths = requests
        .iter()
        .map(|request| {
            request
                .url
                .path()
                .trim_start_matches("/api/v3/repos/a/b")
                .to_owned()
        })
        .collect::<Vec<_>>();
    // The repository and its head are read concurrently: their order is not
    // part of the cost; everything after them is sequential.
    let first_two = paths.len().min(2);
    paths[..first_two].sort();
    assert_eq!(
        paths,
        [
            "",
            "/commits/HEAD",
            &format!("/git/trees/{SHA}:docs%2Fnope%2Fmissing"),
            "/contents/docs%2Fnope%2Fmissing",
            "/contents/docs%2Fnope",
            "/contents/docs",
        ],
        "{paths:?}"
    );
}

/// The Contents API lists 1,000 entries of a directory at most: a missing
/// path whose parent lists that many may sit past the cut, and the failure
/// says so beside its listing lead.
#[tokio::test]
async fn a_missing_path_in_a_full_listing_says_it_may_be_past_the_cut() {
    let server = server().await;
    mount_json(
        &server,
        &format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha": SHA, "truncated": false, "tree": []}),
    )
    .await;
    let entries = (0..crate::tools::gh_shared::CONTENTS_LIMIT)
        .map(|n| json!({"name":format!("f{n}"),"path":format!("docs/f{n}"),"type":"file","size":1}))
        .collect::<Vec<_>>();
    mount_json(&server, "/api/v3/repos/a/b/contents/docs", 200, entries).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let home = tempfile::tempdir().expect("home");
    let failure = run(
        &provider,
        &query(json!({"operation":"tree","owner":"a","repo":"b","path":"docs/readme.md"})),
        Ok(&context),
        home.path(),
        None,
    )
    .await
    .expect_err("missing path");
    assert_eq!(
        failure.hints,
        [crate::tools::gh_shared::LISTING_MAY_BE_CUT_HINT]
    );
}
