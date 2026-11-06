//! The OSV source against a recorded double: wire, states and paging.
use super::*;
use crate::providers::artifact::test_http::{RouteHttp, test_budget};
use crate::providers::saved_answers::ReusedSince;
use serde_json::json;

fn package<'a>(kind: ArtifactType, name: &'a str, version: Option<&'a str>) -> Ask<'a> {
    Ask::Package {
        kind,
        name,
        version,
    }
}

async fn on(
    http: &RouteHttp,
    query: &Ask<'_>,
    start: Option<&str>,
) -> Result<SourceResult, ArtifactError> {
    let budget = test_budget();
    let reused = ReusedSince::default();
    let config = crate::tools::security_check::tests::default_config();
    let net = Net {
        http,
        budget: &budget,
        answers: None,
        reused: &reused,
        config: &config,
    };
    walk(&net, query, &Selection::All, start).await
}

async fn run(http: &RouteHttp, start: Option<&str>) -> Result<SourceResult, ArtifactError> {
    on(
        http,
        &package(ArtifactType::Npm, "lodash", Some("4.17.20")),
        start,
    )
    .await
}

fn record(id: &str) -> Value {
    json!({"id": id, "schema_version": "1.6.0", "affected": [
        {"package": {"name": "lodash", "ecosystem": "npm", "purl": "pkg:npm/lodash"}}
    ]})
}

/// The saved walk a stopped row carries, read back.
fn saved_token(resume: &crate::tools::security_check::source::Resume) -> String {
    let saved: Value = serde_json::from_str(&resume.token).expect("saved walk");
    saved["token"].as_str().expect("token").to_owned()
}

fn ids(walk: &SourceResult) -> Vec<&str> {
    walk.advisories.iter().map(|row| row.id.as_str()).collect()
}

#[tokio::test]
async fn the_request_holds_only_the_package_the_version_and_the_token() {
    let http = RouteHttp::new().post_sequence(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (200, json!({})),
    ]);
    run(&http, None).await.expect("walk");
    let bodies = http.post_bodies();
    assert_eq!(
        bodies,
        vec![
            json!({"package": {"name": "lodash", "ecosystem": "npm"}, "version": "4.17.20"}),
            json!({"package": {"name": "lodash", "ecosystem": "npm"}, "version": "4.17.20", "page_token": "t1"}),
        ]
    );
    assert!(http.seen().iter().all(|seen| !seen.authorized));
    assert_eq!(http.seen()[0].url.as_str(), "https://api.osv.dev/v1/query");
    // A name-only query carries no version key at all.
    let http = RouteHttp::new().post_sequence(vec![(200, json!({}))]);
    on(&http, &package(ArtifactType::Pypi, "requests", None), None)
        .await
        .expect("walk");
    assert_eq!(
        http.post_bodies(),
        vec![json!({"package": {"name": "requests", "ecosystem": "PyPI"}})]
    );
}

#[test]
fn every_registry_uses_osvs_case_sensitive_ecosystem_name() {
    for (kind, name) in [
        (ArtifactType::Npm, "npm"),
        (ArtifactType::Pypi, "PyPI"),
        (ArtifactType::Crates, "crates.io"),
        (ArtifactType::Maven, "Maven"),
        (ArtifactType::Nuget, "NuGet"),
        (ArtifactType::Go, "Go"),
        (ArtifactType::Packagist, "Packagist"),
        (ArtifactType::Rubygems, "RubyGems"),
    ] {
        assert_eq!(ecosystem(kind), name);
    }
}

/// `{}` finishes the walk with nothing: the only empty answer.
#[tokio::test]
async fn an_empty_object_is_a_finished_walk_with_no_records() {
    let http = RouteHttp::new().post_sequence(vec![(200, json!({}))]);
    let done = run(&http, None).await.expect("walk");
    assert!(done.advisories.is_empty());
    assert!(done.resume.is_none());
}

/// "The response might contain only the next_page_token": a page with no
/// `vulns` and a token is not empty, so the walk goes on.
#[tokio::test]
async fn a_token_only_page_walks_on() {
    let http = RouteHttp::new().post_sequence(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t2"})),
        (200, json!({"vulns": [record("A")]})),
    ]);
    let done = run(&http, None).await.expect("walk");
    assert_eq!(ids(&done), ["A"]);
    assert_eq!(http.post_bodies().len(), 3);
    assert!(done.resume.is_none());
}

#[tokio::test]
async fn pages_join_in_order_exactly_once() {
    let http = RouteHttp::new().post_sequence(vec![
        (
            200,
            json!({"vulns": [record("A"), record("B")], "next_page_token": "t1"}),
        ),
        (200, json!({"vulns": [record("C")]})),
    ]);
    let done = run(&http, None).await.expect("walk");
    assert_eq!(ids(&done), ["A", "B", "C"]);
}

/// A later page lost to a transient failure saves what was read with the exact
/// token that reaches the rest: the row lists nothing until the walk finished,
/// and then every record once, with no gap or repeat.
#[tokio::test]
async fn a_walk_stopped_midway_resumes_with_no_gap_or_repeat() {
    let http = RouteHttp::new().post_sequence(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (503, json!({})),
    ]);
    let stopped = run(&http, None).await.expect("partial");
    assert!(stopped.advisories.is_empty(), "listed only once finished");
    let resume = stopped.resume.expect("a token to resume from");
    assert_eq!(saved_token(&resume), "t1");
    assert!(resume.reason.contains("OSV"), "{}", resume.reason);
    let http = RouteHttp::new().post_sequence(vec![(200, json!({"vulns": [record("B")]}))]);
    let rest = run(&http, Some(&resume.token)).await.expect("rest");
    assert_eq!(ids(&rest), ["A", "B"]);
    assert_eq!(http.post_bodies()[0]["page_token"], "t1");
}

/// A token-only page at the deadline is not empty: no records, and a token.
#[tokio::test]
async fn a_token_only_page_at_the_deadline_is_partial_not_empty() {
    let http = RouteHttp::new()
        .post_sequence(vec![(200, json!({"next_page_token": "t1"}))])
        .post_fails_from(1, ErrorCode::Timeout);
    let stopped = run(&http, None).await.expect("partial");
    assert!(stopped.advisories.is_empty());
    assert_eq!(saved_token(&stopped.resume.expect("resume")), "t1");
}

#[tokio::test]
async fn failures_name_osv_and_never_read_as_clean() {
    for status in [429, 500, 503, 400] {
        let http =
            RouteHttp::new().post_sequence(vec![(status, json!({"code": 3, "message": "why"}))]);
        let error = run(&http, None).await.expect_err("failure");
        assert_eq!(error.status, Some(status));
        assert_eq!(
            error.code,
            ProviderErrorKind::from_status(status).code(),
            "{status}"
        );
        for spelled in ["OSV", "lodash@4.17.20", "not checked"] {
            assert!(error.message.contains(spelled), "{}", error.message);
        }
        assert!(!error.message.contains("registry"), "{}", error.message);
    }
    let http = RouteHttp::new().post_fails_from(0, ErrorCode::ProviderError);
    let error = run(&http, None).await.expect_err("transport");
    assert!(
        error.message.starts_with("OSV request failed"),
        "{}",
        error.message
    );
    assert!(!error.message.contains("registry"), "{}", error.message);
}

#[tokio::test]
async fn a_malformed_answer_fails_whole() {
    for body in [
        b"<html>nope</html>".to_vec(),
        b"[]".to_vec(),
        br#"{"vulns":{"id":"A"}}"#.to_vec(),
        br#"{"vulns":[{"summary":"no id"}]}"#.to_vec(),
        br#"{"vulns":[{"id":"A","schema_version":"2.0.0"}]}"#.to_vec(),
        br#"{"vulns":[{"id":"A"}],"next_page_token":7}"#.to_vec(),
    ] {
        let http = RouteHttp::new().post_raw(body.clone());
        let error = run(&http, None).await.expect_err("malformed");
        assert_eq!(error.code, ErrorCode::ProviderError, "{body:?}");
        assert!(error.message.contains("not checked"), "{}", error.message);
    }
    // A record lost on a later page fails the whole answer, not just that page.
    let http = RouteHttp::new().post_sequence(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (200, json!({"vulns": [{"summary": "no id"}]})),
    ]);
    run(&http, None).await.expect_err("fail whole");
    // A token that repeats would never end.
    let http = RouteHttp::new().post_sequence(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t1"})),
    ]);
    run(&http, None).await.expect_err("a repeated token");
}

#[tokio::test]
async fn a_non_adjacent_page_token_cycle_fails_before_requesting_it_again() {
    let http = RouteHttp::new().post_sequence(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t2"})),
        (200, json!({"next_page_token": "t1"})),
        (200, json!({})),
    ]);
    let error = run(&http, None).await.expect_err("a repeated token");
    assert_eq!(error.code, ErrorCode::ProviderError);
    assert!(error.message.contains("token"), "{}", error.message);
    assert_eq!(http.post_bodies().len(), 3, "no repeated provider request");
}

#[tokio::test]
async fn a_page_token_cycle_across_a_resumed_walk_fails() {
    let http = RouteHttp::new().post_sequence(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({"next_page_token": "t2"})),
        (503, json!({})),
    ]);
    let stopped = run(&http, None).await.expect("partial");
    let resume = stopped.resume.expect("resumable provider failure");
    let http = RouteHttp::new().post_sequence(vec![
        (200, json!({"next_page_token": "t1"})),
        (200, json!({})),
    ]);
    let error = run(&http, Some(&resume.token))
        .await
        .expect_err("a token from an earlier page");
    assert_eq!(error.code, ErrorCode::ProviderError);
    assert_eq!(http.post_bodies().len(), 1);
}

/// A rejected token does not resume: it fails, so the row is an error.
#[tokio::test]
async fn a_rejected_token_fails_instead_of_resuming() {
    let http = RouteHttp::new().post_sequence(vec![
        (
            200,
            json!({"vulns": [record("A")], "next_page_token": "t1"}),
        ),
        (400, json!({"code": 3, "message": "invalid cursor"})),
    ]);
    let error = run(&http, None).await.expect_err("rejected");
    assert_eq!(error.status, Some(400));
}

#[tokio::test]
async fn an_advisory_reads_by_id_and_a_withdrawn_one_keeps_the_field() {
    let http = RouteHttp::new().json(
        "/v1/vulns/GHSA-8rfx-6mr3-5jh3",
        json!({"id": "GHSA-8rfx-6mr3-5jh3", "withdrawn": "2024-01-01T00:00:00Z",
            "affected": [{"package": {"name": "a", "ecosystem": "NuGet"}}]}),
    );
    let budget = test_budget();
    let reused = ReusedSince::default();
    let config = crate::tools::security_check::tests::default_config();
    let net = Net {
        http: &http,
        budget: &budget,
        answers: None,
        reused: &reused,
        config: &config,
    };
    let found = check_advisory(&net, "GHSA-8rfx-6mr3-5jh3")
        .await
        .expect("a record");
    assert_eq!(
        found.advisories[0].withdrawn.as_deref(),
        Some("2024-01-01T00:00:00Z")
    );
    assert_eq!(
        found.advisories[0].affected.len(),
        1,
        "every entry is its own"
    );
    assert_eq!(
        http.seen()[0].url.as_str(),
        "https://api.osv.dev/v1/vulns/GHSA-8rfx-6mr3-5jh3"
    );
    let missing = check_advisory(&net, "GHSA-0000-0000-0000")
        .await
        .expect_err("no record");
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert!(
        missing.message.contains("OSV has no record"),
        "{}",
        missing.message
    );
}

/// A commit goes alone: OSV refuses a version beside it.
#[tokio::test]
async fn a_commit_query_holds_only_the_commit_and_the_token() {
    let commit = "6879efc2c1596d11a6a6ad296f80063b558d5e0f";
    let http = RouteHttp::new().post_sequence(vec![
        (
            200,
            json!({"vulns": [{"id": "OSV-2020-484"}], "next_page_token": "t1"}),
        ),
        (200, json!({})),
    ]);
    let done = on(&http, &Ask::Commit(commit), None).await.expect("walk");
    assert_eq!(ids(&done), ["OSV-2020-484"]);
    assert_eq!(
        http.post_bodies(),
        [
            json!({"commit": commit}),
            json!({"commit": commit, "page_token": "t1"})
        ]
    );
}
