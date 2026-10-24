//! Hit rows: numbered occurrences, reads of every hit, owner-wide fragments and path matches.
use super::*;
use octocode_github::mock_http::mount_json;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Repo-scoped hits list every keyword line of the top files with its
/// line number (`grep -n` over the fetched blob), name owner/repo
/// once, and read the top hit by line range.
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
    // A repo-scoped page echoes no owner/repo: they are the query's.
    assert!(data.get("owner").is_none(), "{data}");
    assert!(data.get("repo").is_none(), "{data}");
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
        assert!(
            row["matches"]
                .as_array()
                .is_some_and(|matches| !matches.is_empty()),
            "{row}"
        );
    }
    let read = &data["next"]["read"]["input"]["queries"][0];
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

/// A file whose hits are capped or whose hit lines are cut carries an
/// executable read of every keyword line at the resolved commit, so
/// no hit stays unreachable; a fully shown file gets none.
#[tokio::test]
async fn capped_or_clipped_hit_files_carry_a_read_of_every_hit() {
    let server = MockServer::builder().start().await;
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
        json!({"owner":"a","repo":"b","keywords":["wrap_app"]}),
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
        .map(|read| read["input"]["queries"][0]["path"].as_str().expect("path"))
        .collect();
    paths.sort_unstable();
    assert_eq!(paths, ["src/handler.py", "src/routing.py"], "{data}");
    for read in reads {
        assert_eq!(read["tool"], "ghGetFileContent", "{data}");
        let query = &read["input"]["queries"][0];
        assert_eq!(query["ref"], TREE_SHA, "{data}");
        assert_eq!(query["matchString"], json!(["wrap_app"]), "{data}");
        assert_eq!(
            query["contextLines"],
            json!({"before":0,"after":0}),
            "{data}"
        );
        // A file whose first five hits are shown resumes after them
        // (lines 6-25 hold the other twenty); a file whose shown line
        // was cut re-reads its hits whole.
        match query["path"].as_str().expect("path") {
            "src/routing.py" => assert_eq!(
                query["ranges"],
                json!([{"startLine":6,"endLine":25}]),
                "{data}"
            ),
            _ => assert!(query.get("ranges").is_none(), "{data}"),
        }
        crate::contracts::validate_query("ghGetFileContent", query.clone())
            .expect("readHits is a valid ghGetFileContent query");
    }
}

/// The listing matches keywords case-insensitively, so the hit read
/// does too: an uppercase keyword under smart case would skip counted
/// lowercase hits.
#[tokio::test]
async fn hit_reads_match_case_like_the_listing() {
    let server = MockServer::builder().start().await;
    mount_code_search(&server).await;
    mount_ref(&server, "HEAD").await;
    mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
    mount_content(&server, "src/routing.py", &"wrap_app()\n".repeat(25)).await;
    let out = run(
        &server,
        json!({"owner":"a","repo":"b","keywords":["Wrap_App"]}),
    )
    .await
    .expect("search");
    let query = &out.data["next"]["readHits"]["input"]["queries"][0];
    assert_eq!(query["path"], "src/routing.py", "{}", out.data);
    assert_eq!(query["caseMode"], "insensitive", "{}", out.data);
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

/// Owner-wide searches spend no contents quota: rows keep their
/// owner/repo, and fragment offsets stay out of default output.
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
    // Every row is in a/b: the page names the repository once.
    let row = &out.data["files"][0];
    assert_eq!(out.data["owner"], "a", "{}", out.data);
    assert_eq!(out.data["repo"], "b", "{}", out.data);
    assert!(row.get("owner").is_none(), "{}", out.data);
    assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
    // Offsets are a diagnostics extra (core field class): the default
    // response drops them, `extras:["diagnostics"]` keeps them.

    assert!(out.data.get("commitSha").is_none(), "{}", out.data);
    // The diagnostics extra keeps the offsets.
    let diagnostics = run(&server, json!({"owner":"a","keywords":["wrap_app"]}))
        .await
        .expect("search");
    assert!(
        diagnostics.data["files"][0]["matches"][0]["matchIndices"].is_array(),
        "{}",
        diagnostics.data
    );
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
