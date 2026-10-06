//! Provider-backed tests of the GitHub search tools (ghSearchCode,
//! ghSearchRepo, ghStructure) against one mock server, plus the shared
//! paging helpers.
use super::*;
use crate::tools::gh_search_code::GhSearchCodeQuery;
use crate::tools::gh_search_repo::GhSearchRepoQuery;

#[test]
fn owner_listing_page_ceiling_keeps_rows_and_discloses_remaining_results() {
    let query: GhSearchRepoQuery = serde_json::from_value(
        json!({"owner":"o","page":1000,"mainGoal":"test","reasoning":"test"}),
    )
    .expect("query");
    let mut page = json!({"repositories":[{"repo":"o/r"}],"pagination":{"hasMore":true}});
    add_next(&mut page, ToolId::GhSearchRepo, &query, 1000, true);
    assert_eq!(page["repositories"].as_array().expect("rows").len(), 1);
    assert!(page.get("next").is_none(), "{page}");
    assert_eq!(page["terminalLimit"], true, "{page}");
    let mut earlier = json!({"repositories":[],"pagination":{"hasMore":true}});
    add_next(&mut earlier, ToolId::GhSearchRepo, &query, 999, true);
    assert_eq!(
        earlier["next"]["nextPage"]["query"]["queries"][0]["page"],
        1000
    );
    assert!(earlier.get("terminalLimit").is_none());
}

#[test]
fn search_window_is_rejected_past_the_thousandth_result() {
    assert!(reject_window(11, 100).is_err());
    assert!(reject_window(10, 100).is_ok());
}

#[test]
fn incomplete_and_cap_are_losslessly_typed() {
    let query = serde_json::from_str::<GhSearchCodeQuery>(
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","keywords":["x"],"page":10,"pageSize":100}"#,
        )
        .expect("GitHub search test data should be valid");
    let mut value = json!({"pagination":{"hasMore":false}});
    apply_partial(
        &mut value,
        ToolId::GhSearchCode,
        &query,
        true,
        true,
        10,
        false,
        "code",
    );
    assert_eq!(value["terminalLimit"], true);
    // `partialReasons` names the cap and `pagination.totalItems` its
    // size: no `providerLimit` restates them.
    assert!(value.get("providerLimit").is_none(), "{value}");
    assert_eq!(
        value["partialReasons"],
        json!(["providerResultCap", "providerIncompleteResults"])
    );
    assert_eq!(value["next"]["retry"]["query"]["queries"][0]["page"], 10);
}

mod provider_backed {
    use crate::providers::github::{
        GitHubProvider, NoCache, RetryPolicy, StaticCredentialResolver,
    };
    use crate::providers::github::{ProviderError, ProviderErrorKind, RequestContext};
    use crate::security::scan::Passthrough;
    use crate::tools::gh_shared::test_support::{mock_provider, mount_json};
    use crate::tools::result::ToolData;
    use crate::tools::{gh_search_code, gh_search_repo, gh_structure};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::{Value, json};
    use std::time::Duration;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param, query_param_contains},
    };

    /// One attempt per request: a mock answer is final.
    fn provider(server: &MockServer) -> GitHubProvider<StaticCredentialResolver, NoCache> {
        mock_provider(
            server,
            RetryPolicy {
                max_attempts: 1,
                ..Default::default()
            },
        )
    }

    /// Fixtures name their tool with a test-only `operation` key:
    /// code → ghSearchCode, repositories → ghSearchRepo, tree → ghStructure.
    async fn run(server: &MockServer, mut query: Value) -> Result<ToolData, ProviderError> {
        let operation = query
            .as_object_mut()
            .and_then(|object| object.remove("operation"))
            .expect("fixture operation");
        let home = std::env::temp_dir().join(format!(
            "gh-search-test-{}-{}",
            std::process::id(),
            server.address().port()
        ));
        let provider = provider(server);
        let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
        match operation.as_str() {
            Some("code") => {
                let query = serde_json::from_value(query).expect("code query");
                gh_search_code::execute(&provider, &query, &context, &Passthrough).await
            }
            Some("repositories") => {
                let query = serde_json::from_value(query).expect("repositories query");
                gh_search_repo::execute(&provider, &query, &context).await
            }
            Some("tree") => {
                let query = serde_json::from_value(query).expect("tree query");
                gh_structure::execute(&provider, &query, &context, &home).await
            }
            other => panic!("unknown fixture operation {other:?}"),
        }
    }

    /// An empty code search result, `incomplete` or not.
    async fn empty_code_search(server: &MockServer, incomplete: bool) {
        mount_json(
            server,
            "/api/v3/search/code",
            200,
            json!({"total_count":0,"incomplete_results":incomplete,"items":[]}),
        )
        .await;
    }

    /// Repository `a/b` now lives at `c/d`.
    async fn renamed_to_c_d(server: &MockServer) {
        mount_json(
            server,
            "/api/v3/repos/a/b",
            200,
            json!({"default_branch":"main","full_name":"c/d"}),
        )
        .await;
    }

    /// A code search with one `wrap_app` hit in each `src/<name>.rs`.
    async fn wrap_app_hits(server: &MockServer, names: &[&str]) {
        let items = names
            .iter()
            .map(|name| {
                json!({"name":format!("{name}.rs"),"path":format!("src/{name}.rs"),"sha":"1",
                        "html_url":"https://x",
                        "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                        "text_matches":[{"fragment":"fn wrap_app() {}",
                            "matches":[{"text":"wrap_app","indices":[3,11]}]}]})
            })
            .collect::<Vec<_>>();
        mount_json(
            server,
            "/api/v3/search/code",
            200,
            json!({
                "total_count":items.len(),"incomplete_results":false,"items":items
            }),
        )
        .await;
    }

    /// A code search whose one hit is `fn wrap_app() {}` in `app.rs`.
    async fn app_rs_hit(server: &MockServer) {
        mount_json(server, "/api/v3/search/code", 200, json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
            ]
        }))
        .await;
    }

    /// Org `o`'s repositories, most recently pushed first.
    async fn recently_pushed_org_repos(server: &MockServer, repos: Value) {
        Mock::given(method("GET"))
            .and(path("/api/v3/orgs/o/repos"))
            .and(query_param("sort", "pushed"))
            .and(query_param("direction", "desc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(repos))
            .mount(server)
            .await;
    }

    const TREE_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    /// Resolve `reference` to [`TREE_SHA`], as GitHub's commits endpoint does.
    async fn mount_ref(server: &MockServer, reference: &str) {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(TREE_SHA))
            .mount(server)
            .await;
    }

    fn repo_item(name: &str, archived: bool) -> Value {
        json!({
            "full_name": format!("o/{name}"), "name": name,
            "html_url": "https://x", "default_branch": "main",
            "archived": archived
        })
    }

    /// A renamed repository is followed, as ghSearchHistory does: the same
    /// search (every filter) runs against the canonical name and says so,
    /// instead of an empty, partial row with a retry lead.
    #[tokio::test]
    async fn renamed_repository_is_followed_with_every_filter() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/code"))
            .and(query_param_contains("q", "repo:c/d"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":1,"incomplete_results":false,"items":[
                    {"name":"lib.rs","path":"src/lib.rs","sha":"1","html_url":"https://x",
                     "repository":{"full_name":"c/d","html_url":"https://x","url":"https://x"},
                     "text_matches":[{"fragment":"fn needle() {}","matches":[{"text":"needle","indices":[3,9]}]}]}
                ]})))
            .mount(&server)
            .await;
        empty_code_search(&server, true).await;
        renamed_to_c_d(&server).await;
        let out = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],
                       "extensions":["rs"],"path":"src","language":"rust","pageSize":10}),
            )
            .await
            .expect("search");
        let data = &out.data;
        assert_ne!(out.status, Some("empty"), "{data}");
        assert_eq!(data["files"][0]["path"], "src/lib.rs", "{data}");
        assert!(data["next"].get("retryRenamed").is_none(), "{data}");
        assert!(data["next"].get("retry").is_none(), "{data}");
        assert!(data.get("isPartial").is_none(), "{data}");
        let warnings = data["warnings"].to_string();
        assert!(warnings.contains("renamed to c/d"), "{data}");
        let searched = server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|request| {
                request
                    .url
                    .query_pairs()
                    .find(|(key, _)| key == "q")
                    .map(|(_, q)| q.into_owned())
            })
            .filter(|q| q.contains("repo:c/d"))
            .collect::<Vec<_>>();
        assert_eq!(searched.len(), 1, "{searched:?}");
        for filter in ["extension:rs", "path:src", "language:rust", "needle"] {
            assert!(searched[0].contains(filter), "{filter}: {searched:?}");
        }
    }

    /// D9: an inaccessible repository gets no `retry` for its incomplete
    /// result (a retry cannot help) and says why the search is empty.
    #[tokio::test]
    async fn inaccessible_repository_drops_the_incomplete_retry() {
        let server = MockServer::start().await;
        empty_code_search(&server, true).await;
        mount_json(
            &server,
            "/api/v3/repos/a/b",
            404,
            json!({"message":"Not Found"}),
        )
        .await;
        // A missing repository is a not-found failure (never an empty,
        // partial search a retry could fix) that names repository access.
        let error = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            )
            .await
            .expect_err("missing repository");
        assert_eq!(error.kind, ProviderErrorKind::NotFound);
        assert_eq!(
            error.reason,
            Some(crate::providers::github::ProviderErrorReason::RepositoryNotFound)
        );
        let failure = super::search_failure(error, "");
        assert!(failure.message.contains("private"), "{}", failure.message);
    }

    /// Search items for `a/b` with one GitHub fragment each (GitHub caps
    /// fragments, so routing.py's second call is not in the index text).
    async fn mount_code_search(server: &MockServer) {
        mount_json(server, "/api/v3/search/code", 200, json!({
                    "total_count":2,"incomplete_results":false,"items":[
                        {"name":"handler.py","path":"src/handler.py","sha":"1","html_url":"https://x",
                         "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                         "text_matches":[{"fragment":"def wrap_app(app):","matches":[{"text":"wrap_app","indices":[4,12]}]}]},
                        {"name":"routing.py","path":"src/routing.py","sha":"2","html_url":"https://x",
                         "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                         "text_matches":[{"fragment":"    await wrap_app(app)(scope)","matches":[{"text":"wrap_app","indices":[10,18]}]}]}
                    ]
                })).await;
    }

    async fn mount_content(server: &MockServer, file: &str, body: &str) {
        Mock::given(method("GET"))
                // The contents route encodes the path as one segment.
                .and(path(format!(
                    "/api/v3/repos/a/b/contents/{}",
                    file.replace('/', "%2F")
                )))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode(body)
                })))
                .mount(server)
                .await;
    }

    /// Repo-scoped hits list every keyword line of the top files with its
    /// line number (`grep -n` over the fetched blob), name owner/repo
    /// once, and read the top hit by line range.
    #[tokio::test]
    async fn repo_scoped_hits_list_every_occurrence_with_line_numbers() {
        let server = MockServer::start().await;
        mount_code_search(&server).await;
        mount_ref(&server, "HEAD").await;
        mount_content(
            &server,
            "src/handler.py",
            "import x\n\ndef wrap_app(app):\n    return app\n",
        )
        .await;
        mount_content(
                &server,
                "src/routing.py",
                "from h import wrap_app\n\nasync def a():\n    await wrap_app(app)(scope)\n\nasync def b():\n    await wrap_app(s)(scope)\n",
            )
            .await;
        let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
        let data = &out.data;
        assert_eq!(data["owner"], "a", "{data}");
        assert_eq!(data["repo"], "b", "{data}");
        assert_eq!(data["commitSha"], TREE_SHA, "{data}");
        let files = data["files"].as_array().expect("files");
        let routing = files
            .iter()
            .find(|row| row["path"] == "src/routing.py")
            .expect("routing row");
        assert_eq!(
            routing["lines"],
            json!([
                "1\tfrom h import wrap_app",
                "4\t    await wrap_app(app)(scope)",
                "7\t    await wrap_app(s)(scope)"
            ]),
            "{data}"
        );
        for row in files {
            assert!(
                row.get("owner").is_none() && row.get("repo").is_none(),
                "{row}"
            );
            assert!(row.get("matches").is_none(), "{row}");
        }
        let read = &data["next"]["readTopMatch"]["query"]["queries"][0];
        assert_eq!(read["owner"], "a", "{data}");
        // E10: the top hit declares `wrap_app`, so the read is that whole
        // declaration, never a fixed line window that may cut it.
        assert_eq!(read["path"], "src/handler.py", "{data}");
        assert_eq!(read["matchString"], "def wrap_app(app):", "{data}");
        assert_eq!(read["block"], true, "{data}");
        assert!(
            read.get("startLine").is_none() && read.get("endLine").is_none(),
            "{data}"
        );
        crate::contracts::validate_query("ghGetFileContent", {
            let mut query = read.clone();
            query["mainGoal"] = json!("g");
            query
        })
        .expect("readTopMatch is a valid ghGetFileContent query");
    }

    /// A file whose hits are capped or whose hit lines are cut carries an
    /// executable read of every keyword line at the resolved commit, so
    /// no hit stays unreachable; a fully shown file gets none.
    #[tokio::test]
    async fn capped_or_clipped_hit_files_carry_a_read_of_every_hit() {
        let server = MockServer::start().await;
        mount_code_search(&server).await;
        mount_ref(&server, "HEAD").await;
        mount_content(
            &server,
            "src/handler.py",
            &format!("x = '{}' + wrap_app\n", "a".repeat(400)),
        )
        .await;
        mount_content(&server, "src/routing.py", &"wrap_app()\n".repeat(25)).await;
        let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
        let data = &out.data;
        let reads: Vec<&Value> = ["readHits", "readHits2"]
            .iter()
            .filter_map(|key| data["next"].get(*key))
            .collect();
        assert_eq!(reads.len(), 2, "{data}");
        assert!(data["next"].get("readHits3").is_none(), "{data}");
        let mut paths: Vec<&str> = reads
            .iter()
            .map(|read| read["query"]["queries"][0]["path"].as_str().expect("path"))
            .collect();
        paths.sort_unstable();
        assert_eq!(paths, ["src/handler.py", "src/routing.py"], "{data}");
        for read in reads {
            assert_eq!(read["tool"], "ghGetFileContent", "{data}");
            let query = &read["query"]["queries"][0];
            assert_eq!(query["ref"], TREE_SHA, "{data}");
            assert_eq!(query["matchString"], "wrap_app", "{data}");
            assert_eq!(query["contextLines"], 0, "{data}");
            crate::contracts::validate_query("ghGetFileContent", {
                let mut query = query.clone();
                query["mainGoal"] = json!("g");
                query
            })
            .expect("readHits is a valid ghGetFileContent query");
        }
    }

    /// A fully shown hit list needs no extra read.
    #[tokio::test]
    async fn fully_shown_hit_files_get_no_hit_read() {
        let server = MockServer::start().await;
        mount_code_search(&server).await;
        mount_ref(&server, "HEAD").await;
        mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
        mount_content(&server, "src/routing.py", "wrap_app()\n").await;
        let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
        assert!(out.data["next"].get("readHits").is_none(), "{}", out.data);
    }

    /// Owner-wide searches spend no contents quota: rows keep their
    /// owner/repo, and fragment offsets stay out of default output.
    #[tokio::test]
    async fn owner_wide_hits_keep_fragments_without_offsets() {
        let server = MockServer::start().await;
        mount_code_search(&server).await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits/HEAD"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TREE_SHA))
            .expect(0)
            .mount(&server)
            .await;
        let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
        let row = &out.data["files"][0];
        assert_eq!(row["owner"], "a", "{}", out.data);
        assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
        // Offsets are verbose (core field class): the default response
        // drops them, debug keeps them.
        assert!(
            crate::tools::id::ToolId::GhSearchCode
                .verbose_paths()
                .contains(&"results[].data.files[].matches[].matchIndices"),
            "{}",
            out.data
        );
        assert!(out.data.get("commitSha").is_none(), "{}", out.data);
        // debug keeps the offsets.
        let debug = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","keywords":["wrap_app"],"debug":true}),
            )
            .await
            .expect("search");
        assert!(
            debug.data["files"][0]["matches"][0]["matchIndices"].is_array(),
            "{}",
            debug.data
        );
    }

    /// `branch` verifies the top files at that ref: lines come from the
    /// ref, a path absent there is flagged, and the page says its
    /// candidates came from the default-branch index.
    #[tokio::test]
    async fn branch_hits_are_verified_at_the_ref_and_labeled() {
        let server = MockServer::start().await;
        mount_code_search(&server).await;
        mount_ref(&server, "dev").await;
        mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/src%2Frouting.py",
            404,
            json!({"message":"Not Found"}),
        )
        .await;
        let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"],"ref":"dev"}),
            )
            .await
            .expect("search");
        let data = &out.data;
        // The caller's own ref is not echoed back.
        assert!(data.get("ref").is_none(), "{data}");
        assert_eq!(data["indexRef"], "defaultBranch", "{data}");
        let files = data["files"].as_array().expect("files");
        let handler = files
            .iter()
            .find(|row| row["path"] == "src/handler.py")
            .expect("handler");
        assert_eq!(handler["lines"], json!(["1\tdef wrap_app(app):"]), "{data}");
        let routing = files
            .iter()
            .find(|row| row["path"] == "src/routing.py")
            .expect("routing");
        assert_eq!(routing["atRef"], false, "{data}");
        assert!(
            routing.get("matches").is_none(),
            "default-branch text is not shown as the ref: {data}"
        );
        assert_eq!(
            data["next"]["readTopMatch"]["query"]["queries"][0]["ref"], TREE_SHA,
            "the read is pinned to the commit the lines came from: {data}"
        );
    }

    /// A search pinned to a non-default ref warns that its files come
    /// from the default-branch index at the index commit and leads to
    /// the ref's own listing first; a ref that is the default-branch head
    /// adds neither.
    #[tokio::test]
    async fn a_non_default_ref_discloses_the_index_commit_and_leads_to_the_ref() {
        const HEAD_SHA: &str = "fedcba9876543210fedcba9876543210fedcba98";
        let server = MockServer::start().await;
        mount_code_search(&server).await;
        mount_ref(&server, "dev").await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits/HEAD"))
            .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
            .mount(&server)
            .await;
        mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
        mount_content(&server, "src/routing.py", "await wrap_app(app)\n").await;
        let out = run(
            &server,
            json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"],
                    "path":"src","ref":"dev"}),
        )
        .await
        .expect("search");
        let data = &out.data;
        let warning = data["warnings"][0].as_str().unwrap_or_default();
        assert!(
            warning.contains("default-branch index at fedcba9") && warning.contains("not dev"),
            "{data}"
        );
        assert!(warning.contains("hints.viewRepo"), "{data}");
        let lead = &data["next"]["viewRepo"];
        assert_eq!(lead["tool"], "ghStructure", "{data}");
        for (key, value) in [
            ("owner", "a"),
            ("repo", "b"),
            ("path", "src"),
            ("ref", TREE_SHA),
        ] {
            assert_eq!(lead["query"]["queries"][0][key], value, "{data}");
        }
        assert_eq!(
            data["next"]
                .as_object()
                .and_then(|next| next.keys().next())
                .map(String::as_str),
            Some("viewRepo"),
            "the ref's listing is the first lead: {data}"
        );

        let default_head = MockServer::start().await;
        mount_code_search(&default_head).await;
        mount_ref(&default_head, "main").await;
        mount_ref(&default_head, "HEAD").await;
        mount_content(&default_head, "src/handler.py", "def wrap_app(app):\n").await;
        mount_content(&default_head, "src/routing.py", "await wrap_app(app)\n").await;
        let out = run(
            &default_head,
            json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"],
                    "ref":"main"}),
        )
        .await
        .expect("search");
        assert!(out.data.get("warnings").is_none(), "{}", out.data);
        assert!(out.data.pointer("/next/viewRepo").is_none(), "{}", out.data);
    }

    /// A file the index lists but the requested ref lacks offers no
    /// read: the fragment came from the default branch, and reading it
    /// there would cross the requested scope.
    #[tokio::test]
    async fn a_file_missing_at_the_requested_ref_offers_no_default_branch_read() {
        let server = MockServer::start().await;
        app_rs_hit(&server).await;
        mount_ref(&server, "dev").await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/app.rs",
            404,
            json!({"message":"Not Found"}),
        )
        .await;
        let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "keywords":["wrap_app"],"ref":"dev"}),
        )
        .await
        .expect("search");
        let data = &out.data;
        assert_eq!(data["files"][0]["atRef"], false, "{data}");
        assert!(
            data.pointer("/next/readTopMatch").is_none(),
            "no read outside the requested ref: {data}"
        );
        // GC4: the ref listing searches the moved file's name tree-wide.
        let listing = &data["next"]["viewRepo"]["query"]["queries"][0];
        assert_eq!(listing["include"], json!(["app.rs"]), "{data}");
        assert_eq!(listing["ref"], TREE_SHA, "{data}");
        assert!(listing.get("path").is_none(), "{data}");

        // The file exists at the ref but no line holds the keyword.
        let unmatched = MockServer::start().await;
        app_rs_hit(&unmatched).await;
        mount_ref(&unmatched, "dev").await;
        mount_content(&unmatched, "app.rs", "fn renamed() {}\n").await;
        let out = run(
            &unmatched,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "keywords":["wrap_app"],"ref":"dev"}),
        )
        .await
        .expect("search");
        // The file exists at the ref but no line holds the keyword: the
        // hit is not at the ref, and the default-branch fragment is not
        // shown as the ref's text.
        let row = &out.data["files"][0];
        assert_eq!(row["atRef"], false, "{}", out.data);
        assert!(row.get("matches").is_none(), "{}", out.data);
        assert!(
            out.data.pointer("/next/readTopMatch").is_none(),
            "{}",
            out.data
        );
    }

    /// A default-branch page reads every row's hit lines, not only the
    /// top files: no row is a fragment without line numbers.
    #[tokio::test]
    async fn every_row_of_a_default_branch_page_has_numbered_lines() {
        let server = MockServer::start().await;
        let names = ["a", "b", "c", "d", "e", "f", "g"];
        wrap_app_hits(&server, &names).await;
        mount_ref(&server, "HEAD").await;
        // Distinct bodies: rows with identical evidence would list once.
        for name in names {
            mount_content(
                &server,
                &format!("src/{name}.rs"),
                &format!("fn wrap_app() {{}} // {name}\n"),
            )
            .await;
        }
        let out = run(
            &server,
            json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"]}),
        )
        .await
        .expect("search");
        let files = out.data["files"].as_array().expect("files");
        assert_eq!(files.len(), names.len(), "{}", out.data);
        for (row, name) in files.iter().zip(names) {
            assert_eq!(
                row["lines"],
                json!([format!("1\tfn wrap_app() {{}} // {name}")]),
                "{row}"
            );
            assert!(row.get("matches").is_none(), "{row}");
        }
    }

    /// With `branch`, every row of the page is read at the ref, not only
    /// the top files: each has numbered `lines`, or `atRef:false` (the
    /// hit is not at the ref), or `lineResolved:false` with a read of
    /// its keyword lines at the ref. No row shows default-branch index
    /// text as the ref's.
    #[tokio::test]
    async fn every_row_of_a_ref_page_is_read_at_the_ref() {
        let server = MockServer::start().await;
        let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
        wrap_app_hits(&server, &names).await;
        mount_ref(&server, "dev").await;
        for name in &names[..6] {
            mount_content(
                &server,
                &format!("src/{name}.rs"),
                &format!("\nfn wrap_app() {{}} // {name}\n"),
            )
            .await;
        }
        mount_content(&server, "src/g.rs", "fn renamed() {}\n").await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents/src%2Fh.rs"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let out = run(
            &server,
            json!({"operation":"code","owner":"a","repo":"b",
                    "keywords":["wrap_app"],"ref":"dev"}),
        )
        .await
        .expect("search");
        let data = &out.data;
        let files = data["files"].as_array().expect("files");
        assert_eq!(files.len(), names.len(), "{data}");
        for row in files {
            assert!(row.get("matches").is_none(), "{row}");
        }
        for (row, name) in files[..6].iter().zip(names) {
            assert_eq!(
                row["lines"],
                json!([format!("2\tfn wrap_app() {{}} // {name}")]),
                "{row}"
            );
        }
        assert_eq!(files[6]["atRef"], false, "{data}");
        assert_eq!(files[7]["lineResolved"], false, "{data}");
        let reads = data["next"]
            .as_object()
            .expect("next")
            .values()
            .filter(|read| read["query"]["queries"][0]["path"] == "src/h.rs")
            .collect::<Vec<_>>();
        assert_eq!(reads.len(), 1, "{data}");
        assert_eq!(reads[0]["query"]["queries"][0]["ref"], TREE_SHA, "{data}");
    }

    /// A non-empty page GitHub marks incomplete says so in its data,
    /// without debug, separately from whether another page exists.
    #[tokio::test]
    async fn a_non_empty_incomplete_page_reports_partial_coverage() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/v3/search/code",
            200,
            json!({
                "total_count":1,"incomplete_results":true,"items":[
                    {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                     "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                     "text_matches":[]}
                ]
            }),
        )
        .await;
        let out = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a",
                    "keywords":["wrap_app"],"match":"path"}),
        )
        .await
        .expect("search");
        let data = &out.data;
        assert_eq!(data["isPartial"], true, "{data}");
        assert_eq!(
            data["partialReasons"],
            json!(["providerIncompleteResults"]),
            "{data}"
        );
        assert!(data.pointer("/pagination/hasMore").is_none(), "{data}");
        assert_eq!(
            data["next"]["retry"]["query"]["queries"][0]["page"], 1,
            "{data}"
        );
        assert!(out.diagnostics.partial);
    }

    /// Empty searches name the default-branch index only when a branch
    /// was requested; otherwise the hint names the scope that came up empty.
    #[tokio::test]
    async fn empty_code_search_names_the_default_branch_index() {
        let server = MockServer::start().await;
        empty_code_search(&server, false).await;
        mount_json(
            &server,
            "/api/v3/repos/a/b",
            200,
            json!({"default_branch":"main","full_name":"a/b"}),
        )
        .await;
        for (query, cause) in [
            (
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"ref":"dev"}),
                "default branch",
            ),
            (
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
                "in a/b",
            ),
            (
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","keywords":["needle"]}),
                "any a repository",
            ),
            (
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"extensions":["rs"]}),
                "extensions",
            ),
            (
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"match":"path"}),
                "path",
            ),
        ] {
            let out = run(&server, query).await.expect("search");
            assert_eq!(out.status, Some("empty"));
            let hint = out.data["hints"][0].as_str().unwrap_or_default();
            assert!(hint.contains(cause), "{cause}: {}", out.data);
            assert!(hint.len() <= 120, "{hint}");
            if cause != "default branch" {
                assert!(!hint.contains("default branch"), "{}", out.data);
            }
        }
    }

    /// D5: empty-search hints name the actual cause, and a repository
    /// known to exist gets no root viewStructure.
    #[tokio::test]
    async fn empty_code_search_hints_are_cause_specific() {
        let server = MockServer::start().await;
        empty_code_search(&server, false).await;
        for (repo, full_name) in [("a/b", "a/b"), ("a/old", "c/d"), ("c/d", "c/d")] {
            mount_json(
                &server,
                format!("/api/v3/repos/{repo}"),
                200,
                json!({"default_branch":"main","full_name":full_name}),
            )
            .await;
        }
        let hint = |out: &ToolData| out.data["hints"][0].as_str().unwrap_or_default().to_owned();
        let renamed = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"old","keywords":["needle"]}),
            )
            .await
            .expect("search");
        // The rename is followed: the warning names it, and the empty
        // answer is about the canonical repository.
        assert!(
            renamed.data["warnings"]
                .to_string()
                .contains("renamed to c/d"),
            "{}",
            renamed.data
        );
        assert!(hint(&renamed).contains("in c/d"), "{}", renamed.data);
        assert!(
            renamed.data["next"].get("retryRenamed").is_none(),
            "{}",
            renamed.data
        );

        let existing = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            )
            .await
            .expect("search");
        // An existing repository with no hit leads to its root structure.
        assert_eq!(
            existing.data["next"]["viewStructure"]["query"]["queries"][0]["path"], "",
            "{}",
            existing.data
        );
        let scoped = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"path":"src/io"}),
            )
            .await
            .expect("search");
        assert_eq!(
            scoped.data["next"]["viewStructure"]["query"]["queries"][0]["path"], "src/io",
            "{}",
            scoped.data
        );

        let path_mode = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                       "keywords":["fn spawn_blocking"],"match":"path"}),
        )
        .await
        .expect("search");
        assert!(
            hint(&path_mode).contains("match:\"path\""),
            "{}",
            path_mode.data
        );
        let content = &path_mode.data["next"]["searchContent"]["query"]["queries"][0];
        assert_eq!(content["match"], "file", "{}", path_mode.data);
        assert_eq!(content["keywords"], json!(["fn spawn_blocking"]));
    }

    /// A zero-hit recovery for a requested branch inspects that branch,
    /// not the default branch the index covers.
    #[tokio::test]
    async fn empty_scoped_recovery_keeps_the_requested_branch() {
        let server = MockServer::start().await;
        empty_code_search(&server, false).await;
        for (repo, archived) in [("b", false), ("old", true)] {
            mount_json(&server, format!("/api/v3/repos/a/{repo}"), 200, json!({"default_branch":"main","full_name":format!("a/{repo}"),"archived":archived}),).await;
        }
        for repo in ["b", "old"] {
            let out = run(
                    &server,
                    json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":repo,
                           "keywords":["needle"],"path":"src","ref":"nondefault"}),
                )
                .await
                .expect("search");
            let view = &out.data["next"]["viewStructure"]["query"]["queries"][0];
            assert_eq!(view["ref"], "nondefault", "{}", out.data);
        }
        let default = run(
            &server,
            json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                       "keywords":["needle"],"path":"src"}),
        )
        .await
        .expect("search");
        assert!(
            default.data["next"]["viewStructure"]["query"]["queries"][0]
                .get("ref")
                .is_none(),
            "{}",
            default.data
        );
    }

    /// D9: path matches carry no empty snippet list.
    #[tokio::test]
    async fn path_match_rows_omit_empty_matches() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/v3/search/code",
            200,
            json!({
                "total_count":1,"incomplete_results":false,"items":[{
                    "name":"pool.rs","path":"src/pool.rs","sha":"1","html_url":"https://x",
                    "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}
                }]
            }),
        )
        .await;
        let out = run(
            &server,
            json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                       "keywords":["pool"],"match":"path"}),
        )
        .await
        .expect("search");
        let row = &out.data["files"][0];
        assert_eq!(row["path"], "src/pool.rs", "{}", out.data);
        assert!(row.get("matches").is_none(), "{}", out.data);
    }

    #[tokio::test]
    async fn concise_repositories_are_flat_rows() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/v3/search/repositories",
            200,
            json!({
                "total_count":1,"incomplete_results":false,"items":[repo_item("r", false)]
            }),
        )
        .await;
        let out = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","keywords":["x"],"concise":true}),
            )
            .await
            .expect("search");
        assert_eq!(out.data["repositories"], json!(["o/r"]));
        assert_eq!(
            out.data["next"]["viewRepo"],
            json!({"tool":"ghStructure","query":{"queries":[{"owner":"o","repo":"r"}]}}),
            "{}",
            out.data
        );
    }

    /// The top repository leads to its tree; an empty page leads nowhere.
    #[tokio::test]
    async fn top_repository_leads_to_its_tree() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/v3/search/repositories",
            200,
            json!({
                "total_count":2,"incomplete_results":false,
                "items":[repo_item("top", false), repo_item("second", false)]
            }),
        )
        .await;
        let none = MockServer::start().await;
        mount_json(
            &none,
            "/api/v3/search/repositories",
            200,
            json!({
                "total_count":0,"incomplete_results":false,"items":[]
            }),
        )
        .await;
        let out = run(
            &server,
            json!({"operation":"repositories","keywords":["x"]}),
        )
        .await
        .expect("search");
        let lead = &out.data["next"]["viewRepo"];
        assert_eq!(
            lead,
            &json!({"tool":"ghStructure","query":{"queries":[{"owner":"o","repo":"top"}]}}),
            "{}",
            out.data
        );
        let replay: crate::tools::gh_structure::GhStructureQuery =
            serde_json::from_value(lead["query"]["queries"][0].clone())
                .expect("valid ghStructure query");
        assert_eq!(replay.repo.as_str(), "top");
        let empty = run(
            &none,
            json!({"operation":"repositories","keywords":["none"]}),
        )
        .await
        .expect("empty search");
        assert!(
            empty.data.pointer("/next/viewRepo").is_none(),
            "{}",
            empty.data
        );
    }

    #[tokio::test]
    async fn owner_listing_updated_sort_tracks_pushes_and_excludes_archived() {
        let server = MockServer::start().await;
        recently_pushed_org_repos(
            &server,
            json!([repo_item("live", false), repo_item("old", true)]),
        )
        .await;
        let out = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o","sort":"updated"}),
            )
            .await
            .expect("owner listing");
        let names = out.data["repositories"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["repo"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["live".to_owned()], "{}", out.data);
        assert_eq!(out.data["order"], "pushed");
    }

    /// D8: the listing API's own order is creation (oldest first); the
    /// default best-match listing asks for the most recently pushed.
    #[tokio::test]
    async fn owner_listing_default_sort_is_recent_activity_not_creation() {
        let server = MockServer::start().await;
        recently_pushed_org_repos(&server, json!([repo_item("live", false)])).await;
        let out = run(
            &server,
            json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o"}),
        )
        .await
        .expect("owner listing");
        assert_eq!(out.data["repositories"][0]["repo"], "live", "{}", out.data);
        assert_eq!(out.data["order"], "pushed", "{}", out.data);
    }

    #[tokio::test]
    async fn owner_listing_reads_past_archived_pages_and_follows_the_link_cursor() {
        // A full provider page of mostly archived repositories plus a Link
        // next must keep paging instead of ending with a fabricated total.
        let server = MockServer::start().await;
        let link = |page: usize| {
            format!(
                "<{}/api/v3/orgs/o/repos?page={page}&per_page=3>; rel=\"next\"",
                server.uri()
            )
        };
        for (page, rows, next) in [
            (
                1,
                vec![
                    repo_item("a", false),
                    repo_item("x1", true),
                    repo_item("x2", true),
                ],
                Some(2),
            ),
            (
                2,
                vec![
                    repo_item("x3", true),
                    repo_item("b", false),
                    repo_item("c", false),
                ],
                Some(3),
            ),
            (3, vec![repo_item("d", false)], None),
        ] {
            let mut response = ResponseTemplate::new(200).set_body_json(json!(rows));
            if let Some(next) = next {
                response = response.insert_header("link", link(next).as_str());
            }
            Mock::given(method("GET"))
                .and(path("/api/v3/orgs/o/repos"))
                .and(query_param("page", page.to_string()))
                .respond_with(response)
                .mount(&server)
                .await;
        }
        let names = |out: &ToolData| {
            out.data["repositories"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| row["repo"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        };
        let first = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o","pageSize":3}),
            )
            .await
            .expect("owner listing");
        assert_eq!(names(&first), vec!["a", "b", "c"], "{}", first.data);
        let pagination = &first.data["pagination"];
        assert_eq!(pagination["hasMore"], true, "{}", first.data);
        // The page cursor lives in next.nextPage only.
        assert!(pagination.get("nextPage").is_none(), "{}", first.data);
        assert!(pagination.get("totalItems").is_none(), "{}", first.data);
        assert!(pagination.get("totalPages").is_none(), "{}", first.data);
        assert_eq!(
            first.data["next"]["nextPage"]["query"]["queries"][0]["page"],
            3
        );
        assert_ne!(first.status, Some("empty"));

        let last = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o","pageSize":3,"page":3}),
            )
            .await
            .expect("last page");
        assert_eq!(names(&last), vec!["d"], "{}", last.data);
        assert_eq!(last.data["pagination"]["hasMore"], false);
        assert!(last.data["next"].get("nextPage").is_none(), "{}", last.data);
    }

    /// ghSearchRepo rows carry decision facts only, topics
    /// favor the query, and pagination does not restate next.nextPage.
    #[tokio::test]
    async fn repository_rows_are_compact_and_paging_is_not_duplicated() {
        let server = MockServer::start().await;
        let topics = (0..20)
            .map(|n| format!("t{n}"))
            .chain(["http-client".into()])
            .collect::<Vec<String>>();
        let long = "x".repeat(300);
        Mock::given(method("GET"))
            .and(path("/api/v3/search/repositories"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":25,"incomplete_results":false,"items":[{
                    "full_name":"httpie/cli","name":"cli","html_url":"h","default_branch":"master",
                    "stargazers_count":38602,"forks_count":4012,"language":"Python",
                    "license":{"spdx_id":"BSD-3-Clause"},"description":long,
                    "topics":topics,"pushed_at":"2024-12-17T00:00:00Z",
                    "created_at":"2012-02-25T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"
                }]
            })))
            .mount(&server)
            .await;
        let query = json!({"operation":"repositories","mainGoal": "test", "reasoning":"test",
                "keywords":["http client"],"pageSize":1});
        let out = run(&server, query.clone()).await.expect("search");
        let row = &out.data["repositories"][0];
        assert_eq!(row["repo"], "cli", "{row}");
        assert_eq!(row["owner"], "httpie", "{row}");
        assert_eq!(row["stars"], 38602);
        assert_eq!(row["pushedAt"], "2024-12-17");
        // Every topic and the whole description stay: nothing reaches a
        // cut remainder, so a row never shortens them.
        let shown = row["topics"].as_array().expect("topics");
        assert_eq!(shown.len(), 21, "{row}");
        assert_eq!(
            shown[0], "http-client",
            "query-matching topics come first: {row}"
        );
        assert_eq!(row["description"], long.as_str(), "{row}");
        assert!(row.get("topicCount").is_none(), "{row}");
        // Forks and the metadata-update date are verbose: the verbose stage
        // drops them by default and debug keeps them.
        for verbose in ["forks", "updatedAt"] {
            assert!(
                crate::tools::id::ToolId::GhSearchRepo
                    .verbose_paths()
                    .contains(&format!("results[].data.repositories[].{verbose}").as_str()),
                "{verbose}"
            );
        }
        // One pagination block with stable keys on every page.
        let pagination = out.data["pagination"].clone();
        assert_eq!(
            pagination,
            json!({"totalItems":25,"hasMore":true,"currentPage":1,"totalPages":25}),
            "{}",
            out.data
        );
        assert_eq!(
            out.data["next"]["nextPage"]["query"]["queries"][0]["page"],
            2
        );

        let mut debug = query;
        debug["debug"] = json!(true);
        let out = run(&server, debug).await.expect("debug search");
        let row = &out.data["repositories"][0];
        assert_eq!(row["forks"], 4012, "{row}");
        assert_eq!(row["createdAt"], "2012-02-25");
    }

    /// ghSearchRepo: `qualifiers` alone is a search, not an owner
    /// listing, and reaches GitHub as normalized qualifiers.
    #[tokio::test]
    async fn qualifiers_reach_the_search_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/repositories"))
            .and(query_param(
                "q",
                "user:o archived:false forks:>50 good-first-issues:>2",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":1,"incomplete_results":false,"items":[repo_item("r", false)]
            })))
            .mount(&server)
            .await;
        let out = run(
            &server,
            json!({"operation":"repositories","mainGoal": "test", "reasoning":"test",
                    "owner":"o","qualifiers":"forks:>50 good-first-issues:>2"}),
        )
        .await
        .expect("qualified search");
        assert_eq!(out.data["repositories"][0]["repo"], "r", "{}", out.data);
        assert!(out.data.get("order").is_none(), "a search, not a listing");
    }

    /// ghStructure: `pattern` finds a file by name at any depth in one
    /// call, keeping the dir/files row shape.
    #[tokio::test]
    async fn tree_pattern_finds_paths_by_name_at_any_depth() {
        let server = MockServer::start().await;
        mount_ref(&server, "main").await;
        let tree = json!([
            {"path":"starlette","type":"tree"},
            {"path":"starlette/_exception_handler.py","type":"blob","size":3},
            {"path":"starlette/routing.py","type":"blob","size":3},
            {"path":"tests","type":"tree"},
            {"path":"tests/test_exception_handler.py","type":"blob","size":3},
            {"path":"docs/exceptions.md","type":"blob","size":3}
        ]);
        mount_json(
            &server,
            format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
            200,
            json!({"sha":TREE_SHA,"tree":tree,"truncated":false}),
        )
        .await;
        let find = |pattern: &str| {
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                    "ref":"main","include":[pattern]})
        };
        let out = run(&server, find("**/_exception_handler.py"))
            .await
            .expect("glob");
        assert_eq!(
            out.data["entries"],
            json!([{"dir":"starlette","files":["_exception_handler.py"]}]),
            "{}",
            out.data
        );
        assert_eq!(out.data["summary"]["totalFiles"], 1);
        let bare = run(&server, find("Exception_Handler"))
            .await
            .expect("bare word");
        assert_eq!(bare.data["summary"]["totalFiles"], 2, "{}", bare.data);
        let none = run(&server, find("**/nope.py")).await.expect("no match");
        assert_eq!(none.status, Some("empty"));
        assert!(
            none.data["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("include"))
        );
        let invalid = run(&server, find("src/[")).await.expect_err("bad glob");
        assert_eq!(invalid.kind, ProviderErrorKind::Validation);
    }

    /// ghStructure: a recursive listing fetches its tree once, by the
    /// resolved commit SHA. GitHub's tree response carries the tree
    /// object's SHA, never the commit's, so a tree fetched by ref name
    /// cannot be proven to belong to the resolved commit.
    #[tokio::test]
    async fn deep_tree_is_fetched_once_at_the_resolved_commit() {
        let server = MockServer::start().await;
        mount_ref(&server, "main").await;
        let tree_object = "b".repeat(40);
        Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/git/trees/main"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    json!({"sha":tree_object,"truncated":false,"tree":[{"path":"by-ref.rs","type":"blob","size":1}]}),
                ))
                .expect(0)
                .mount(&server)
                .await;
        Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    json!({"sha":tree_object,"truncated":false,"tree":[{"path":"pinned.rs","type":"blob","size":1}]}),
                ))
                .expect(1)
                .mount(&server)
                .await;
        let out = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                    "ref":"main","maxDepth":3}),
        )
        .await
        .expect("tree");
        assert_eq!(
            out.data["entries"][0]["files"],
            json!(["pinned.rs"]),
            "{}",
            out.data
        );
        assert_eq!(out.data["commitSha"], TREE_SHA);
        let requests = server.received_requests().await.unwrap_or_default();
        let paths = requests
            .iter()
            .map(|request| request.url.path().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                "/api/v3/repos/a/b/commits/main".to_owned(),
                format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
                // The page's last-commit dates.
                "/api/graphql".to_owned(),
            ],
            "no speculative tree request"
        );
    }

    /// Paths beneath an ignored directory are dropped before paging,
    /// sizing, and materializing, and the recursive tree and the
    /// Contents walk list the same permitted entries.
    #[tokio::test]
    async fn ignored_directories_hide_their_descendants_from_every_consumer() {
        let expected = json!([{"dir":".","files":["app.rs"]}]);
        // Recursive Git Trees listing.
        let server = MockServer::start().await;
        mount_ref(&server, "main").await;
        mount_json(
            &server,
            format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
            200,
            json!({
            "sha":"c".repeat(40),"truncated":false,"tree":[
                {"path":"app.rs","type":"blob","size":3},
                {"path":"vendor","type":"tree"},
                {"path":"vendor/hidden.rs","type":"blob","size":9},
                {"path":"vendor/deep","type":"tree"},
                {"path":"vendor/deep/more.rs","type":"blob","size":9}
            ]}),
        )
        .await;
        mount_content(&server, "app.rs", "app").await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex("/contents/vendor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode("hidden")
            })))
            .expect(0)
            .mount(&server)
            .await;
        let out = run(
            &server,
            json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "ref":"main","maxDepth":5,"pageSize":1,"materialize":true}),
        )
        .await
        .expect("recursive tree");
        let data = &out.data;
        assert_eq!(data["entries"], expected, "{data}");
        assert!(data.get("pagination").is_none(), "no phantom page: {data}");
        assert_eq!(data["omitted"]["entries"], json!({"vendor":1}), "{data}");
        let local = std::path::Path::new(
            data["location"]["localPath"]
                .as_str()
                .expect("materialized location"),
        );
        assert!(local.join("app.rs").exists(), "{data}");
        assert!(!local.join("vendor").exists(), "{data}");
        let _ = std::fs::remove_dir_all(local);

        // Contents walk (the recursive tree is unavailable).
        let walked = MockServer::start().await;
        mount_ref(&walked, "main").await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex("/git/trees/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&walked)
            .await;
        mount_json(
            &walked,
            "/api/v3/repos/a/b/contents",
            200,
            json!([
                {"name":"app.rs","path":"app.rs","type":"file","size":3},
                {"name":"vendor","path":"vendor","type":"dir"}
            ]),
        )
        .await;
        let out = run(
            &walked,
            json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "ref":"main","maxDepth":5,"pageSize":1}),
        )
        .await
        .expect("contents walk");
        assert_eq!(out.data["entries"], expected, "{}", out.data);
        assert_eq!(
            out.data["omitted"]["entries"],
            json!({"vendor":1}),
            "{}",
            out.data
        );
    }

    #[tokio::test]
    async fn capped_search_is_terminal_only_on_the_last_reachable_page() {
        let server = MockServer::start().await;
        let item = json!({"name":"a.rs","path":"a.rs","sha":"s","html_url":"h",
                "repository":{"full_name":"o/r","html_url":"h","url":"u"}});
        mount_json(
            &server,
            "/api/v3/search/code",
            200,
            json!({
                "total_count":5000,"incomplete_results":false,"items":[item]
            }),
        )
        .await;
        let first = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":1,"pageSize":100}),
            )
            .await
            .expect("page 1");
        assert!(first.data["next"]["nextPage"].is_object());
        assert!(first.data.get("terminalLimit").is_none(), "{}", first.data);
        let last = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":10,"pageSize":100}),
            )
            .await
            .expect("page 10");
        assert_eq!(last.data["terminalLimit"], true, "{}", last.data);
        assert!(last.data["next"].get("nextPage").is_none());
    }

    #[tokio::test]
    async fn tree_missing_path_on_existing_branch_does_not_fall_back() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents/missing"))
            .and(query_param("ref", TREE_SHA))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
            .mount(&server)
            .await;
        // The path exists on the default branch only: the read must report
        // it missing on `dev`, not show the default-branch copy.
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents/missing"))
            .and(query_param("ref", "main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"name":"x.rs","path":"missing/x.rs","type":"file","size":1}
            ])))
            .mount(&server)
            .await;
        mount_ref(&server, "dev").await;
        mount_json(
            &server,
            "/api/v3/repos/a/b",
            200,
            json!({"default_branch":"main"}),
        )
        .await;
        let error = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"dev","path":"missing"}),
            )
            .await
            .expect_err("path missing on an existing branch");
        assert_eq!(error.kind, ProviderErrorKind::NotFound);
    }

    /// D3: an explicit ref that does not exist is an error naming it,
    /// never a silent listing of the default branch.
    #[tokio::test]
    async fn tree_missing_branch_is_an_error_not_a_default_branch_listing() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/commits/gone",
            422,
            json!({"message":"No commit found for SHA: gone"}),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b",
            200,
            json!({"default_branch":"main"}),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents",
            200,
            json!([
                {"name":"x.rs","path":"x.rs","type":"file","size":1}
            ]),
        )
        .await;
        let error = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"gone"}),
            )
            .await
            .expect_err("a missing ref is an error");
        assert_eq!(error.kind, ProviderErrorKind::NotFound);
        assert_eq!(
            error.reason,
            Some(crate::providers::github::ProviderErrorReason::RefNotFound)
        );
        assert!(error.message.contains("\"gone\""), "{}", error.message);
    }

    /// D5: a listing names the commit it read, and its next page reads
    /// that same commit even if the branch moves.
    #[tokio::test]
    async fn tree_pins_its_pages_to_the_resolved_commit() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/v3/repos/a/b",
            200,
            json!({"default_branch":"main"}),
        )
        .await;
        mount_ref(&server, "HEAD").await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/contents"))
            .and(query_param("ref", TREE_SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"name":"a.rs","path":"a.rs","type":"file","size":1},
                {"name":"b.rs","path":"b.rs","type":"file","size":1}
            ])))
            .mount(&server)
            .await;
        let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","pageSize":1}),
            )
            .await
            .expect("tree");
        assert_eq!(out.data["resolvedRef"], "main", "{}", out.data);
        assert_eq!(out.data["commitSha"], TREE_SHA, "{}", out.data);
        assert_eq!(
            out.data["next"]["nextPage"]["query"]["queries"][0]["ref"],
            TREE_SHA
        );
    }

    /// D10: a deep listing pages directory by directory, so its first
    /// page carries files, not only folders.
    #[tokio::test]
    async fn deep_tree_pages_carry_files_with_their_folders() {
        let server = MockServer::start().await;
        mount_ref(&server, "main").await;
        let mut tree = Vec::new();
        // 10 top-level folders × 3 subfolders × 3 files: 40 folders
        // sort before the first file in a folders-first listing.
        for dir in 0..10 {
            tree.push(json!({"path":format!("d{dir}"),"type":"tree"}));
            for sub in 0..3 {
                tree.push(json!({"path":format!("d{dir}/s{sub}"),"type":"tree"}));
                for file in 0..3 {
                    tree.push(
                        json!({"path":format!("d{dir}/s{sub}/f{file}.rs"),"type":"blob","size":1}),
                    );
                }
            }
        }
        mount_json(
            &server,
            format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
            200,
            json!({"sha":TREE_SHA,"tree":tree,"truncated":false}),
        )
        .await;
        let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main","maxDepth":5,"pageSize":20}),
            )
            .await
            .expect("tree");
        let files = out.data["summary"]["totalFiles"].as_u64().unwrap_or(0);
        assert!(files >= 5, "page 1 lists files: {}", out.data);
    }

    #[tokio::test]
    async fn tree_on_a_file_path_is_a_clear_error() {
        let server = MockServer::start().await;
        mount_ref(&server, "main").await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/src%2Flib.rs",
            200,
            json!({
                "name":"lib.rs","path":"src/lib.rs","type":"file","size":3,"sha":"1"
            }),
        )
        .await;
        let error = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main","path":"src/lib.rs"}),
            )
            .await
            .expect_err("file path");
        assert_eq!(error.kind, ProviderErrorKind::Validation);
        assert!(error.message.contains("is a file"), "{}", error.message);
    }

    #[tokio::test]
    async fn materialize_skips_binary_files_with_a_warning() {
        let server = MockServer::start().await;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents",
            200,
            json!([
                {"name":"img.png","path":"img.png","type":"file","size":4},
                {"name":"ok.rs","path":"ok.rs","type":"file","size":4}
            ]),
        )
        .await;
        mount_json(
            &server,
            format!("/api/v3/repos/a/b/commits/{sha}"),
            200,
            json!({"sha":sha}),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/img.png",
            200,
            json!({
                "type":"file","encoding":"base64","content":STANDARD.encode([0u8, 1, 2, 3])
            }),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/ok.rs",
            200,
            json!({
                "type":"file","encoding":"base64","content":STANDARD.encode("fn x(){}")
            }),
        )
        .await;
        let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":sha,"materialize":true}),
            )
            .await
            .expect("materialize continues past binary files");
        let local = out.data["location"]["localPath"]
            .as_str()
            .expect("location");
        assert!(std::path::Path::new(local).join("ok.rs").exists());
        // The warning carries the count; `location.skipped` names the file.
        assert_eq!(
            out.data["location"]["skipped"],
            json!(["img.png"]),
            "{}",
            out.data
        );
        let warnings = out.data["warnings"].to_string();
        assert!(
            warnings.contains("Skipped 1 unreadable file(s)"),
            "{}",
            out.data
        );
        let _ = std::fs::remove_dir_all(local);
    }

    #[tokio::test]
    async fn tree_reports_omitted_entries_and_next_page_drops_completed_metadata() {
        let server = MockServer::start().await;
        mount_ref(&server, "main").await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents",
            200,
            json!([
                {"name":"node_modules","path":"node_modules","type":"dir"},
                {"name":".DS_Store","path":".DS_Store","type":"file","size":1},
                {"name":"a.rs","path":"a.rs","type":"file","size":4},
                {"name":"b.rs","path":"b.rs","type":"file","size":4}
            ]),
        )
        .await;
        mount_json(
            &server,
            "/api/v3/repos/a/b/branches",
            200,
            json!([{"name":"main"}]),
        )
        .await;
        let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main",
                    "pageSize":1}),
            )
            .await
            .expect("tree");
        assert_eq!(
            out.data["omitted"]["entries"],
            json!({".DS_Store":1,"node_modules":1}),
            "{}",
            out.data
        );
        let next = &out.data["next"]["nextPage"]["query"]["queries"][0];
        assert_eq!(next["page"], 2, "{}", out.data);
    }

    #[tokio::test]
    async fn tree_fallback_walk_stops_at_the_directory_fetch_cap() {
        let server = MockServer::start().await;
        mount_ref(&server, "main").await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex("/git/trees/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let dirs = (0..250)
            .map(|n| json!({"name":format!("d{n}"),"path":format!("d{n}"),"type":"dir"}))
            .collect::<Vec<_>>();
        mount_json(&server, "/api/v3/repos/a/b/contents", 200, json!(dirs)).await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex("/contents/d[0-9]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main","maxDepth":2}),
            )
            .await
            .expect("tree");
        assert_eq!(out.data["terminalLimit"], true, "{}", out.data);
        assert_eq!(out.data["providerLimit"]["reason"], "treeFetchLimit");
        let requests = server.received_requests().await.unwrap_or_default();
        let contents = requests
            .iter()
            .filter(|request| request.url.path().contains("/contents"))
            .count();
        assert_eq!(contents, gh_structure::MAX_DIRECTORY_FETCHES);
    }

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    async fn mount_listing(server: &MockServer, dir: &str, entries: Value) {
        let route = if dir.is_empty() {
            "/api/v3/repos/a/b/contents".to_owned()
        } else {
            format!("/api/v3/repos/a/b/contents/{}", dir.replace('/', "%2F"))
        };
        mount_json(server, route, 200, entries).await;
    }

    /// A listing leads to the read a caller makes next: the package's
    /// entry file (outlined) in a wide directory, at the listed commit.
    #[tokio::test]
    async fn a_listing_leads_to_an_outline_read_of_its_entry_file() {
        let server = MockServer::start().await;
        let files = (0..12)
                .map(|n| json!({"name":format!("m{n}.py"),"path":format!("pkg/m{n}.py"),"type":"file","size":4}))
                .chain(std::iter::once(json!({"name":"__init__.py","path":"pkg/__init__.py","type":"file","size":4})))
                .collect::<Vec<_>>();
        mount_listing(&server, "pkg", json!(files)).await;
        let out = run(
            &server,
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"pkg"}),
        )
        .await
        .expect("listing");
        let read = &out.data["next"]["read"];
        assert_eq!(read["tool"], "ghGetFileContent", "{}", out.data);
        let row = &read["query"]["queries"][0];
        assert_eq!(row["path"], "pkg/__init__.py", "{}", out.data);
        assert_eq!(row["ref"], SHA, "{}", out.data);
        assert_eq!(row["minify"], "symbols", "{}", out.data);
    }

    /// A materialized listing points at the listed directory on disk.
    #[tokio::test]
    async fn materialize_leads_into_the_local_copy_of_the_listed_directory() {
        let server = MockServer::start().await;
        mount_listing(
            &server,
            "src",
            json!([{"name":"lib.rs","path":"src/lib.rs","type":"file","size":8}]),
        )
        .await;
        mount_content(&server, "src/lib.rs", "fn x(){}").await;
        let out = run(
                &server,
                json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"src","materialize":true}),
            )
            .await
            .expect("materialize");
        let local = out.data["location"]["localPath"]
            .as_str()
            .expect("location")
            .to_owned();
        assert!(local.ends_with("/src"), "{}", out.data);
        assert!(std::path::Path::new(&local).join("lib.rs").exists());
        // GS4: no lead re-lists the tree this response just returned.
        assert!(
            out.data.pointer("/next/exploreClone").is_none(),
            "{}",
            out.data
        );
        let _ = std::fs::remove_dir_all(&local);
    }

    /// A listing without a ref names its default branch from one
    /// repository lookup per repository, not one per call.
    #[tokio::test]
    async fn the_default_branch_is_looked_up_once_per_repository() {
        let server = MockServer::start().await;
        mount_ref(&server, "HEAD").await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"default_branch":"main","full_name":"a/b"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        mount_listing(
            &server,
            "",
            json!([{"name":"a.rs","path":"a.rs","type":"file","size":1}]),
        )
        .await;
        let provider = GitHubProvider {
            transport: provider(&server).transport,
            cache: MemoryCache::default(),
        };
        let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
        let home = std::env::temp_dir().join("gh-default-branch-memo");
        for _ in 0..2 {
            let query =
                serde_json::from_value(json!({"owner":"a","repo":"b"})).expect("tree query");
            let out = gh_structure::execute(&provider, &query, &context, &home)
                .await
                .expect("listing");
            assert_eq!(out.data["resolvedRef"], "main", "{}", out.data);
        }
    }

    #[derive(Default)]
    struct MemoryCache(
        std::sync::Mutex<
            std::collections::HashMap<String, crate::providers::github::CachedContent>,
        >,
    );
    impl crate::providers::github::ConditionalCache for MemoryCache {
        fn get<'a>(
            &'a self,
            _: &'a crate::providers::github::CachePartition,
            key: &'a str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Option<crate::providers::github::CachedContent>>
                    + Send
                    + 'a,
            >,
        > {
            let hit = self.0.lock().ok().and_then(|map| map.get(key).cloned());
            Box::pin(async move { hit })
        }
        fn put<'a>(
            &'a self,
            _: &'a crate::providers::github::CachePartition,
            key: String,
            value: crate::providers::github::CachedContent,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
            if let Ok(mut map) = self.0.lock() {
                map.insert(key, value);
            }
            Box::pin(async {})
        }
    }

    fn code_item(name: &str) -> Value {
        json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
            "text_matches":[{"fragment":"needle","matches":[{"text":"needle","indices":[0,6]}]}]})
    }

    /// Several `extensions` are one call for the agent: one indexed search
    /// per extension (GitHub ANDs repeated qualifiers), merged into one page.
    #[tokio::test]
    async fn several_extensions_fan_out_into_one_page() {
        let server = MockServer::start().await;
        for (extension, file) in [("ts", "a.ts"), ("js", "b.js")] {
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .and(wiremock::matchers::query_param_contains(
                    "q",
                    format!("extension:{extension}"),
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":1,"incomplete_results":false,"items":[code_item(file)]
                })))
                .expect(1)
                .mount(&server)
                .await;
        }
        let out = run(
            &server,
            json!({"operation":"code","owner":"a","keywords":["needle"],"extensions":["ts","js"]}),
        )
        .await
        .expect("search");
        // Both extensions' rows carry the same fragment: listed once, with
        // the other path in `alsoAt`.
        let paths = out.data["files"]
            .as_array()
            .expect("files")
            .iter()
            .flat_map(|row| {
                std::iter::once(&row["path"]).chain(row["alsoAt"].as_array().into_iter().flatten())
            })
            .map(|path| path.as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["a.ts", "b.js"], "{}", out.data);
    }

    /// A ref page discloses the default-branch index once (page 1); later
    /// pages carry neither the warning nor its HEAD lookup.
    #[tokio::test]
    async fn the_index_warning_is_stated_on_the_first_ref_page_only() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/v3/search/code",
            200,
            json!({
                "total_count":40,"incomplete_results":false,"items":[code_item("a.rs")]
            }),
        )
        .await;
        mount_ref(&server, "dev").await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits/HEAD"))
            .respond_with(ResponseTemplate::new(200).set_body_string("f".repeat(40)))
            .expect(1)
            .mount(&server)
            .await;
        mount_content(&server, "a.rs", "needle\n").await;
        for page in [1, 2] {
            let out = run(
                &server,
                json!({"operation":"code","owner":"a","repo":"b","keywords":["needle"],
                    "ref":"dev","page":page,"pageSize":10}),
            )
            .await
            .expect("search");
            let warned = out.data["warnings"]
                .to_string()
                .contains("default-branch index");
            assert_eq!(warned, page == 1, "page {page}: {}", out.data);
        }
    }

    /// An empty search of an existing repository leads to its structure:
    /// the index may lag, so verify the path before concluding absence.
    #[tokio::test]
    async fn an_empty_repository_search_leads_to_its_structure() {
        let server = MockServer::start().await;
        empty_code_search(&server, false).await;
        mount_json(
            &server,
            "/api/v3/repos/a/b",
            200,
            json!({"default_branch":"main","full_name":"a/b"}),
        )
        .await;
        let out = run(
            &server,
            json!({"operation":"code","owner":"a","repo":"b","keywords":["absent"]}),
        )
        .await
        .expect("search");
        assert_eq!(out.status, Some("empty"));
        let lead = &out.data["next"]["viewStructure"];
        assert_eq!(lead["tool"], "ghStructure", "{}", out.data);
        assert_eq!(lead["query"]["queries"][0]["repo"], "b", "{}", out.data);
    }

    /// The top repository leads to its tree (not a code search of the
    /// discovery words), and an empty filtered search leads to the same
    /// keywords without filters.
    #[tokio::test]
    async fn repositories_lead_to_the_tree_and_an_empty_search_broadens() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/repositories"))
            .and(wiremock::matchers::query_param_contains(
                "q",
                "language:rust",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
            )
            .mount(&server)
            .await;
        mount_json(
            &server,
            "/api/v3/search/repositories",
            200,
            json!({
                "total_count":1,"incomplete_results":false,"items":[repo_item("top", false)]
            }),
        )
        .await;
        let out = run(
            &server,
            json!({"operation":"repositories","keywords":["needle"]}),
        )
        .await
        .expect("search");
        // GR4: repo-discovery words are not code keywords; the top
        // repository leads to its tree only.
        assert!(
            out.data["next"].get("searchContent").is_none(),
            "{}",
            out.data
        );
        assert_eq!(
            out.data["next"]["viewRepo"]["tool"], "ghStructure",
            "{}",
            out.data
        );
        let empty = run(
            &server,
            json!({"operation":"repositories","keywords":["needle"],"language":"rust"}),
        )
        .await
        .expect("search");
        assert_eq!(empty.status, Some("empty"));
        let broader = &empty.data["next"]["findRepository"];
        assert_eq!(broader["tool"], "ghSearchRepo", "{}", empty.data);
        assert_eq!(
            broader["query"]["queries"][0],
            json!({"keywords":["needle"]}),
            "{}",
            empty.data
        );
    }
}
