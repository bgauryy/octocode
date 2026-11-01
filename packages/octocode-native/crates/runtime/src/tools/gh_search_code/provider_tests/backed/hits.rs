//! Hit rows: numbered occurrences, reads of every hit, owner-wide fragments and path matches.
use super::*;
use octocode_github::mock_http::mount_json;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Repo-scoped hits list every keyword line of the top files with its
/// line number, retain repository and commit identity, and read the top hit.
#[tokio::test]
async fn repo_scoped_hits_list_every_occurrence_with_line_numbers() {
    let server = MockServer::builder().start().await;
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
        json!({"owner":"a","repo":"b","keywords":["wrap_app"]}),
    )
    .await
    .expect("search");
    let data = &out.data;
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
        assert_eq!(row["owner"], "a", "{row}");
        assert_eq!(row["repo"], "b", "{row}");
        assert_eq!(row["commitSha"], TREE_SHA, "{row}");
        assert!(
            row["matches"]
                .as_array()
                .is_some_and(|matches| !matches.is_empty()),
            "{row}"
        );
    }
    let read = &crate::tools::test_support::checks::related_call(data, "ghGetFileContent", |_| {
        true
    })["input"]["queries"][0];
    assert_eq!(read["owner"], "a", "{data}");
    // The top hit declares `wrap_app`, so the read is that whole
    // declaration, never a fixed line window that may cut it.
    assert_eq!(read["path"], "src/handler.py", "{data}");
    assert_eq!(read["matchString"], json!(["def wrap_app(app):"]), "{data}");
    assert_eq!(read["block"], true, "{data}");
    assert!(
        read.get("startLine").is_none() && read.get("endLine").is_none(),
        "{data}"
    );
    crate::contracts::validate_query("ghGetFileContent", read.clone())
        .expect("read is a valid ghGetFileContent query");
}

/// Resolved source hits reach the response pager whole, without a replay read.
#[tokio::test]
async fn complete_long_and_many_hits_need_no_recovery_read() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    let long = format!("x = '{}' + wrap_app", "λ🦀".repeat(400));
    mount_content(&server, "src/handler.py", &format!("{long}\n")).await;
    mount_content(&server, "src/routing.py", &"wrap_app()\n".repeat(25)).await;
    let out = run(
        &server,
        json!({"owner":"a","repo":"b","keywords":["Wrap_App"]}),
    )
    .await
    .expect("search");
    let items = out.data["files"].as_array().expect("files");
    let lines = |path: &str| {
        items.iter().find(|row| row["path"] == path).expect("file")["lines"]
            .as_array()
            .expect("lines")
            .clone()
    };
    assert_eq!(lines("src/handler.py"), vec![json!(format!("1\t{long}"))]);
    assert_eq!(
        lines("src/routing.py"),
        (1..=25)
            .map(|n| json!(format!("{n}\twrap_app()")))
            .collect::<Vec<_>>()
    );
    assert!(out.data["next"].get("readHits").is_none());
    assert_eq!(
        crate::tools::test_support::checks::related_call(&out.data, "ghGetFileContent", |_| true)["kind"],
        "related"
    );
}

/// A fully shown hit list needs no extra read.
#[tokio::test]
async fn fully_shown_hit_files_get_no_hit_read() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
    mount_content(&server, "src/routing.py", "wrap_app()\n").await;
    let out = run(
        &server,
        json!({"owner":"a","repo":"b","keywords":["wrap_app"]}),
    )
    .await
    .expect("search");
    assert!(out.data["next"].get("readHits").is_none(), "{}", out.data);
}

/// Unresolved owner-wide hits retain repository identity and fragment offsets.
#[tokio::test]
async fn owner_wide_hits_keep_fragments_without_offsets() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(TREE_SHA))
        .expect(0)
        .mount(&server)
        .await;
    let out = run(&server, json!({"owner":"a","keywords":["wrap_app"]}))
        .await
        .expect("search");
    let row = &out.data["files"][0];
    assert_eq!(row["owner"], "a", "{}", out.data);
    assert_eq!(row["repo"], "b", "{}", out.data);
    assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
    assert!(row.get("commitSha").is_none(), "{row}");
    assert!(row["matches"][0]["matchIndices"].is_array(), "{row}");
}

/// Path matches carry no empty snippet list.
#[tokio::test]
async fn path_match_rows_omit_empty_matches() {
    let server = MockServer::builder().start().await;
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
        json!({ "owner":"a","repo":"b",
                   "keywords":["pool"],"matchIn":["path"]}),
    )
    .await
    .expect("search");
    let row = &out.data["files"][0];
    assert_eq!(row["path"], "src/pool.rs", "{}", out.data);
    assert!(row.get("matches").is_none(), "{}", out.data);
}
