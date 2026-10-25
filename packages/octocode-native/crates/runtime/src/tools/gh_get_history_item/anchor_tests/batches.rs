//! Provider batch boundaries: reported totals past the last batch and file boundaries are lower bounds; small pages count every file.
use super::*;

#[tokio::test]
async fn pr_commit_last_short_batch_discloses_the_unreachable_reported_total() {
    for total in [250, 260] {
        let server = MockServer::builder().start().await;
        mount_json(
            &server,
            "/api/v3/repos/o/r/pulls/1",
            200,
            json!({
                "number":1,"title":"commits","state":"open","user":{"login":"ada"},
                "head":{"sha":HEAD},"base":{"sha":BASE},"commits":total,"changed_files":0
            }),
        )
        .await;
        for page in 1..=3 {
            let start = (page - 1) * 100;
            let rows = (start..(start + 100).min(250))
                .map(|i| commit_raw(&format!("{i:040x}"), &format!("commit {i}")))
                .collect::<Vec<_>>();
            let mut response = ResponseTemplate::new(200).set_body_json(rows);
            if page < 3 {
                response = response.insert_header(
                    "Link",
                    format!(
                        "<{}/api/v3/repos/o/r/pulls/1/commits?page={}&per_page=100>; rel=\"next\"",
                        server.uri(),
                        page + 1,
                    ),
                );
            }
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/o/r/pulls/1/commits"))
                .and(wiremock::matchers::query_param("page", page.to_string()))
                .respond_with(response)
                .mount(&server)
                .await;
        }
        // The details view keeps page detail: the row's limits still lift
        // off its pages (the page schema declares none of them).
        let mut query = json!({"operation":"pullRequest","owner":"o","repo":"r",
            "number":1,"sections":["commits"],"pageSize":100});
        let mut all = Vec::new();
        let mut pages = 0;
        let last = loop {
            pages += 1;
            assert!(pages <= 3);
            let data = read(&server, query.clone()).await;
            assert_history_page_contract(&data);
            all.extend(
                data["pullRequests"][0]["commits"]
                    .as_array()
                    .expect("commits")
                    .iter()
                    .map(|c| c["commitSha"].as_str().expect("sha").to_owned()),
            );
            let Some(next) = data.pointer("/next/nextCommitPage/input") else {
                break data;
            };
            let normalized = crate::contracts::validate_built("ghGetHistoryItem", next.clone())
                .expect("executable copied commit page");
            query = normalized["queries"][0].clone();
        };
        assert_eq!(
            all,
            (0..250).map(|i| format!("{i:040x}")).collect::<Vec<_>>()
        );
        assert_eq!(
            all.iter().collect::<std::collections::HashSet<_>>().len(),
            250
        );
        let page = &last["pullRequests"][0]["contentPagination"]["commits"];
        assert_eq!(page["hasMore"], false);
        if total > 250 {
            assert_eq!(last["terminalLimit"], true, "{last}");
            assert_eq!(last["isPartial"], true, "{last}");
            assert_eq!(last["pullRequests"][0]["commitsCount"], total, "{last}");
            assert_eq!(page["countScope"], "partial", "{last}");
            assert_eq!(
                last["providerLimit"]["reason"], "providerCommitListLimit",
                "{last}"
            );
            assert!(last["next"].get("nextCommitPage").is_none(), "{last}");
        } else {
            assert_ne!(last["terminalLimit"], true, "{last}");
            assert_eq!(page["countScope"], "complete", "{last}");
        }
    }
}

#[tokio::test]
async fn commit_file_boundary_is_a_lower_bound_with_or_without_a_final_link() {
    for (listed, final_link, bounded) in [
        (2999, false, false),
        (3000, false, true),
        (3000, true, true),
    ] {
        let server = MockServer::builder().start().await;
        let base_url = server.uri();
        // GitHub pages a commit's files by the request's `per_page`, and
        // ends the list at 3000.
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/r/commits/{HEAD}")))
            .respond_with(move |request: &wiremock::Request| {
                let param = |key: &str| {
                    request
                        .url
                        .query_pairs()
                        .find(|(name, _)| name == key)
                        .and_then(|(_, value)| value.parse::<usize>().ok())
                };
                let (page, per) = (param("page").unwrap_or(1), param("per_page").unwrap_or(300));
                let start = (page - 1) * per;
                let files = (start..(start + per).min(listed)).map(|i| {
                    json!({"filename":format!("file{i:04}.ts"),"status":"added","additions":1,"deletions":0})
                }).collect::<Vec<_>>();
                let mut raw = commit_raw(HEAD, "files");
                raw["parents"] = json!([{"sha":BASE}]);
                raw["files"] = json!(files);
                let mut response = ResponseTemplate::new(200).set_body_json(raw);
                if page * per < 3000 || final_link {
                    response = response.insert_header(
                        "Link",
                        format!(
                            "<{base_url}/api/v3/repos/o/r/commits/{HEAD}?page={}&per_page={per}>; rel=\"next\"",
                            page + 1,
                        ),
                    );
                }
                response
            })
            .mount(&server)
            .await;
        let mut query = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
            "sections":["files"],"pageSize":100});
        let mut all = Vec::new();
        let mut pages = 0;
        let last = loop {
            pages += 1;
            assert!(
                pages <= 30,
                "no phantom page past the hard provider boundary"
            );
            let data = read(&server, query.clone()).await;
            assert_history_page_contract(&data);
            all.extend(data["files"].as_array().expect("files").iter().map(|file| {
                file.as_str()
                    .expect("root-file inventory row")
                    .split_whitespace()
                    .last()
                    .expect("path")
                    .to_owned()
            }));
            let Some(next) = data.pointer("/next/nextFilePage/input") else {
                break data;
            };
            let normalized = crate::contracts::validate_built("ghGetHistoryItem", next.clone())
                .expect("executable copied file page");
            query = normalized["queries"][0].clone();
        };
        assert_eq!(
            all,
            (0..listed)
                .map(|i| format!("file{i:04}.ts"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            all.iter().collect::<std::collections::HashSet<_>>().len(),
            listed
        );
        assert_eq!(
            last["contentPagination"]["files"]["hasMore"], false,
            "{last}"
        );
        if bounded {
            assert_eq!(last["terminalLimit"], true, "{last}");
            assert_eq!(last["isPartial"], true, "{last}");
            assert_eq!(
                last["contentPagination"]["files"]["countScope"], "partial",
                "{last}"
            );
            assert_eq!(last["changedFilesCountScope"], "partial", "{last}");
            assert_eq!(
                last["changedFilesCount"], 3000,
                "only observed files, never an invented total"
            );
        } else {
            assert_ne!(last["terminalLimit"], true, "{last}");
            assert_eq!(
                last["contentPagination"]["files"]["countScope"], "complete",
                "{last}"
            );
        }
        assert!(last["next"].get("nextFilePage").is_none(), "{last}");
    }
}

/// A commit of up to 300 files is one provider batch (GitHub's commit page
/// is 300 files): a small page of it still counts every file exactly.
#[tokio::test]
async fn a_commit_under_the_commit_page_counts_every_file_on_a_small_page() {
    let server = MockServer::builder().start().await;
    let files = (0..113)
        .map(|i| changed_file(&format!("f{i:03}.rs")))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/commits/{HEAD}")))
        .respond_with(paged_like_github(server.uri(), files, |files| {
            let mut raw = commit_raw(HEAD, "many files");
            raw["parents"] = json!([{"sha":BASE}]);
            raw["files"] = json!(files);
            raw
        }))
        .mount(&server)
        .await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
        "sections":["files"],"pageSize":20});
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&data);
    assert_eq!(data["changedFilesCount"], 113, "{data}");
    assert!(data.get("changedFilesCountScope").is_none(), "{data}");
    let page = &data["contentPagination"]["files"];
    assert_eq!(page["totalItems"], 113, "{data}");
    assert_eq!(page["hasMore"], true, "{data}");
    assert_eq!(data["files"].as_array().map(Vec::len), Some(20), "{data}");
}
