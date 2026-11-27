//! Owner-wide and repo-scoped rows: numbered lines at each row's commit, fragments when missing.
use super::*;
use octocode_test_support::json_fixture;

/// Two owner-wide hits in different repositories, indexed at `SHA`.
async fn mount_owner_wide_search(server: &MockServer) {
    let item = |repo: &str| {
        json!({"name":"x.py","path":"src/x.py","sha":"1",
         "html_url": format!("https://github.com/o/{repo}/blob/{SHA}/src/x.py"),
         "repository":{"full_name":format!("o/{repo}"),"html_url":"https://x","url":"https://x"},
         "text_matches":[{"fragment":"class HTTPAdapter(BaseAdapter):\n    pass","matches":[{"text":"HTTPAdapter","indices":[6,17]}]}]})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,"items":[item("a"), item("b")]
        })))
        .mount(server)
        .await;
}

async fn run_owner_wide(server: &MockServer) -> octocode_runtime_base::response::result::ToolData {
    let provider = one_shot_provider(server);
    let query: GhSearchCodeQuery = serde_json::from_value(json_fixture(
        r#"{"owner": "o", "keywords": ["HTTPAdapter"]}"#,
    ))
    .expect("query");
    let context = default_context();
    execute(
        &provider,
        &query,
        &context,
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
}

/// Owner-wide rows are read at the commit each hit was indexed at
/// (`html_url`): numbered `lines` replace fragments, each row stating that
/// commit.
#[tokio::test]
async fn owner_wide_rows_list_numbered_lines_at_the_indexed_commit() {
    let server = MockServer::builder().start().await;
    mount_owner_wide_search(&server).await;
    for repo in ["a", "b"] {
        octocode_test_support::mock_http::list_in_trees(&server, "o", repo, "src/x.py").await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/{repo}/contents/src%2Fx.py")))
            .and(wiremock::matchers::query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64",
                "content":STANDARD.encode("import x\n\nclass HTTPAdapter(BaseAdapter):\n    pass\n")
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let out = run_owner_wide(&server).await;
    let files = out.data["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{}", out.data);
    for row in files {
        assert_eq!(
            row["matches"],
            json!(["3\tclass HTTPAdapter(BaseAdapter):"]),
            "{}",
            out.data
        );
        assert!(row.get("fragments").is_none(), "{}", out.data);
        assert!(row["repo"].is_string(), "{}", out.data);
        assert_eq!(row["owner"], "o", "{}", out.data);
        assert_eq!(row["commitSha"], SHA, "{}", out.data);
    }
}

/// Owner-wide rows of one repository read at one commit state it once in
/// `repoCommits`; a repository's lone row keeps its own. Every row still
/// names its repository, in rank order, and finds its commit on the page.
#[tokio::test]
async fn owner_wide_rows_of_one_repository_state_their_commit_once() {
    let server = MockServer::builder().start().await;
    let item = |repo: &str, file: &str| {
        json!({"name":file,"path":format!("src/{file}"),"sha":format!("{repo}{file}"),
         "html_url": format!("https://github.com/o/{repo}/blob/{SHA}/src/{file}"),
         "repository":{"full_name":format!("o/{repo}"),"html_url":"https://x","url":"https://x"},
         "text_matches":[{"fragment":"class HTTPAdapter(BaseAdapter):\n    pass","matches":[{"text":"HTTPAdapter","indices":[6,17]}]}]})
    };
    let hits = [("a", "x.py"), ("b", "x.py"), ("a", "y.py")];
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":hits.len(),"incomplete_results":false,
            "items":hits.iter().map(|(repo, file)| item(repo, file)).collect::<Vec<_>>()
        })))
        .mount(&server)
        .await;
    for (repo, file) in hits {
        let at = format!("src/{file}");
        octocode_test_support::mock_http::list_in_trees(&server, "o", repo, &at).await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/{repo}/contents/src%2F{file}")))
            .and(wiremock::matchers::query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64",
                "content":STANDARD.encode(format!("class HTTPAdapter(BaseAdapter):  # {repo} {file}\n    pass\n"))
            })))
            .mount(&server)
            .await;
    }
    let out = run_owner_wide(&server).await;
    let files = out.data["files"].as_array().expect("files");
    assert_eq!(files.len(), hits.len(), "{}", out.data);
    assert_eq!(
        out.data["repoCommits"],
        json!([format!("a {SHA}")]),
        "{}",
        out.data
    );
    for row in files {
        assert_eq!(row["owner"], "o", "{}", out.data);
        let own = row["commitSha"].as_str();
        match row["repo"].as_str() {
            Some("a") => assert_eq!(own, None, "{}", out.data),
            Some("b") => assert_eq!(own, Some(SHA), "{}", out.data),
            other => panic!("a row names its repository: {other:?}"),
        }
    }
    // The page's reads stay pinned to the commit the rows were read at.
    for call in out.data["related"].as_array().into_iter().flatten() {
        for read in call["input"]["queries"].as_array().into_iter().flatten() {
            assert_eq!(read["ref"], SHA, "{}", out.data);
        }
    }
}

/// A row whose file is gone at its indexed commit keeps its fragment
/// and says its lines were not read.
#[tokio::test]
async fn owner_wide_row_missing_at_its_commit_keeps_fragments() {
    let server = MockServer::builder().start().await;
    mount_owner_wide_search(&server).await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "^/api/v3/repos/o/[ab]/contents/",
        ))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    let out = run_owner_wide(&server).await;
    let row = &out.data["files"][0];
    assert_eq!(row["lineResolved"], false, "{}", out.data);
    assert!(row.get("fragments").is_none(), "{}", out.data);
    assert!(row.get("matches").is_none(), "{}", out.data);
}

/// Each scoped file retains the repository, path and commit of its numbered lines.
#[tokio::test]
async fn a_repo_scoped_row_is_path_and_numbered_lines_at_one_commit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": false,
                "items": [
                    {
                        "name": "app.rs",
                        "path": "src/app.rs",
                        "sha": "1",
                        "html_url": "https://x",
                        "repository": {
                            "full_name": "a/b",
                            "html_url": "https://x",
                            "url": "https://x"
                        },
                        "text_matches": [
                            {
                                "fragment": "fn wrap_app() {}",
                                "matches": [{"text": "wrap_app", "indices": [3, 11]}]
                            }
                        ]
                    }
                ]
            }"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/app.rs").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fapp.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content":STANDARD.encode("// app\nfn wrap_app() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json_fixture(
        r#"{"owner": "a", "repo": "b", "keywords": ["wrap_app"]}"#,
    ))
    .expect("query");
    let context = default_context();
    let out = execute(
        &provider,
        &query,
        &context,
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search");
    assert_eq!(out.data["files"][0]["commitSha"], SHA, "{}", out.data);
    assert_eq!(out.data["files"][0]["owner"], "a", "{}", out.data);
    assert_eq!(out.data["files"][0]["repo"], "b", "{}", out.data);
    assert_eq!(
        out.data["files"][0]["matches"],
        json!(["2\tfn wrap_app() {}"]),
        "{}",
        out.data
    );
    for echo in ["owner", "repo", "ref", "indexRef", "keywords"] {
        assert!(out.data.get(echo).is_none(), "{echo}: {}", out.data);
    }
}
