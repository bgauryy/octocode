//! Owner-wide and repo-scoped rows: numbered lines at each row's commit, fragments when missing.
use super::*;

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

async fn run_owner_wide(server: &MockServer) -> crate::response::result::ToolData {
    let provider = one_shot_provider(server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"o","keywords":["HTTPAdapter"]})).expect("query");
    let context = default_context();
    execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search")
}

/// Owner-wide rows are read at the commit each hit was indexed at
/// (`html_url`): numbered `lines` replace fragments, and the top read is
/// pinned to that commit instead of a `contextLines` fragment read.
#[tokio::test]
async fn owner_wide_rows_list_numbered_lines_at_the_indexed_commit() {
    let server = MockServer::builder().start().await;
    mount_owner_wide_search(&server).await;
    for repo in ["a", "b"] {
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
            row["lines"],
            json!(["3\tclass HTTPAdapter(BaseAdapter):"]),
            "{}",
            out.data
        );
        assert!(row["matches"].is_array(), "{}", out.data);
        assert!(row["repo"].is_string(), "{}", out.data);
        assert_eq!(row["owner"], "o", "{}", out.data);
        assert_eq!(row["commitSha"], SHA, "{}", out.data);
    }
    let read =
        &crate::tools::test_support::checks::related_call(&out.data, "ghGetFileContent", |_| true)
            ["input"]["queries"][0];
    assert_eq!(read["ref"], SHA, "{}", out.data);
    assert_eq!(read["owner"], "o", "{}", out.data);
    assert_eq!(read["repo"], "a", "{}", out.data);
    assert!(read.get("contextLines").is_none(), "{}", out.data);
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
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let out = run_owner_wide(&server).await;
    let row = &out.data["files"][0];
    assert_eq!(row["lineResolved"], false, "{}", out.data);
    assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
    assert!(row.get("lines").is_none(), "{}", out.data);
}

/// Each scoped file retains the repository, path and commit of its numbered lines.
#[tokio::test]
async fn a_repo_scoped_row_is_path_and_numbered_lines_at_one_commit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"src/app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fapp.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content":STANDARD.encode("// app\nfn wrap_app() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["wrap_app"]}))
            .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    assert_eq!(out.data["files"][0]["commitSha"], SHA, "{}", out.data);
    assert_eq!(out.data["files"][0]["owner"], "a", "{}", out.data);
    assert_eq!(out.data["files"][0]["repo"], "b", "{}", out.data);
    assert_eq!(
        out.data["files"][0]["lines"],
        json!(["2\tfn wrap_app() {}"]),
        "{}",
        out.data
    );
    for echo in ["owner", "repo", "ref", "indexRef", "keywords"] {
        assert!(out.data.get(echo).is_none(), "{echo}: {}", out.data);
    }
}
