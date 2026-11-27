//! A ref that moves between the parts of one search page.
use super::{files, mount, walk};
use crate::support::{self, Workspace};
use serde_json::json;
use std::collections::BTreeSet;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The commits a walk's file reads asked for (each read's `ref`).
async fn read_refs(server: &MockServer) -> BTreeSet<String> {
    let received = server.received_requests().await.expect("recorded");
    received
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .filter_map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "ref")
                .map(|(_, value)| value.into_owned())
        })
        .collect()
}

/// A ref search whose ref moves after the first page: the parts of one
/// search page read at the commits a walk over search pages reads at, and
/// list the same rows.
#[tokio::test]
async fn a_ref_moved_between_parts_reads_as_a_walk_over_search_pages_does() {
    const AT_FIRST: &str = "1111111111111111111111111111111111111111";
    const MOVED: &str = "2222222222222222222222222222222222222222";
    const HEAD: &str = "3333333333333333333333333333333333333333";
    let names = files(12, |n| format!("src/m{n:02}.rs"));
    let mut walks = Vec::new();
    for (limit, chars) in [(12, 2000), (4, 20_000)] {
        let server = MockServer::builder().start().await;
        mount(&server, &names, &[]).await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/commits/dev"))
            .respond_with(ResponseTemplate::new(200).set_body_string(AT_FIRST))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/commits/dev"))
            .respond_with(ResponseTemplate::new(200).set_body_string(MOVED))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/commits/HEAD"))
            .respond_with(ResponseTemplate::new(200).set_body_string(HEAD))
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&support::github_api(&server));
        let start = json!({
            "queries": [{"owner": "o", "repo": "r", "keywords": ["needle"], "ref": "dev",
                         "limit": limit}],
            "responseChars": chars
        });
        let calls = walk(&server, &runtime, start).await;
        assert!(calls.len() > 1, "limit {limit} pages");
        let mut shown = calls
            .iter()
            .flat_map(|call| call.listed.clone())
            .collect::<Vec<_>>();
        shown.sort();
        let commits = calls
            .iter()
            .flat_map(|call| {
                call.output["results"][0]["data"]["files"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .map(|file| file["commitSha"].to_string())
            .collect::<BTreeSet<_>>();
        walks.push((shown, commits, read_refs(&server).await));
        runtime.close().await;
    }
    assert_eq!(walks[0].0, names, "the parts list every file once");
    assert_eq!(walks[0], walks[1], "parts read as search pages do");
}
