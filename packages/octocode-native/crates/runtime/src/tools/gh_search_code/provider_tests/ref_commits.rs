//! The commit a page's hit lines are read at: a ref's, resolved beside the search, or each hit's indexed one.
use super::*;

/// A ref search's commit, which its hit lines are pinned to, resolves while
/// the index search runs: one round trip, not two in series, before the
/// file reads.
#[tokio::test]
async fn the_ref_commit_resolves_while_the_search_runs() {
    let server = MockServer::builder().start().await;
    let delay = Duration::from_millis(300);
    let search = Delayed::new(
        delay,
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
            ]
        })),
    );
    let commit = Delayed::new(delay, ResponseTemplate::new(200).set_body_string(SHA));
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(search.clone())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/dev"))
        .respond_with(commit.clone())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/app.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("fn wrap_app() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","ref":"dev","keywords":["wrap_app"]}))
            .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    assert_eq!(out.data["files"][0]["commitSha"], SHA, "{}", out.data);
    // The page's core-quota cost: one contents read (an `extras:["stats"]` field).
    assert_eq!(out.data["contentReads"], 1, "{}", out.data);
    assert_eq!(
        out.data["files"][0]["lines"],
        json!(["1\tfn wrap_app() {}"])
    );
    let first = |slow: &Delayed| *slow.arrivals.times().first().expect("requested");
    let (searched, resolved) = (first(&search), first(&commit));
    let gap = if resolved > searched {
        resolved - searched
    } else {
        searched - resolved
    };
    assert!(
        gap < delay / 2,
        "the commit lookup waited {gap:?} for the search"
    );
}

/// A default-branch search reads each hit at the commit the index matched
/// (its `html_url`), so its lines are the text the index searched, with no
/// commit lookup; files whose index blob is the same are read once.
#[tokio::test]
async fn default_branch_lines_are_read_at_the_indexed_commit() {
    let server = MockServer::builder().start().await;
    let item = |name: &str, blob: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":blob,
            "html_url":format!("https://github.com/a/b/blob/{SHA}/src/{name}"),
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":3,"incomplete_results":false,"items":[
                item("one.rs", "b1"), item("copy.rs", "b1"), item("two.rs", "b2")
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/commits/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    for (name, body, reads) in [
        ("one.rs", "fn wrap_app() {}\n", 1),
        ("copy.rs", "fn wrap_app() {}\n", 0),
        ("two.rs", "let x = wrap_app();\n", 1),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .and(wiremock::matchers::query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .expect(reads)
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["wrap_app"]}))
            .expect("query");
    let out = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("search");
    let data = &out.data;
    for file in data["files"].as_array().expect("files") {
        assert_eq!(file["commitSha"], SHA, "{file}");
    }
    assert_eq!(data["contentReads"], 2, "{data}");
    let listed = data["files"]
        .as_array()
        .expect("files")
        .iter()
        .flat_map(|file| {
            std::iter::once(file["path"].clone())
                .chain(file["alsoAt"].as_array().cloned().unwrap_or_default())
        })
        .collect::<Vec<_>>();
    assert_eq!(listed.len(), 3, "every hit is listed once: {data}");
    assert!(
        data["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|file| file.get("lines").is_some()),
        "{data}"
    );
}

/// A path-only search, or a `fields` pick without `lines`, reads no lines,
/// so it resolves no commit.
#[tokio::test]
async fn searches_without_line_reads_resolve_no_commit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    for row in [
        json!({"owner":"a","repo":"b","keywords":["app"],"matchIn":["path"]}),
        json!({"owner":"a","repo":"b","keywords":["app"],"fields":["path"]}),
    ] {
        let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
        execute(&provider, &query, &context, &Passthrough)
            .await
            .expect("search");
    }
    let requests = server.received_requests().await.expect("requests");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() == "/api/v3/search/code"),
        "{:?}",
        requests.iter().map(|r| r.url.path()).collect::<Vec<_>>()
    );
}

/// A path-only page reads no lines, yet names the commit the index matched
/// (from each hit's `html_url`), so a read of a listed path can be pinned;
/// rows indexed at different commits each name theirs.
#[tokio::test]
async fn path_only_rows_name_their_indexed_commit() {
    let other = "89abcdef0123456789abcdef0123456789abcdef";
    let server = MockServer::builder().start().await;
    let item = |name: &str, sha: &str| {
        json!({"name":name,"path":name,"sha":"1",
            "html_url":format!("https://github.com/a/b/blob/{sha}/{name}"),
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param("q", "app repo:a/b in:path"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,
            "items":[item("app.rs", SHA), item("app_test.rs", SHA)]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param("q", "lib repo:a/b in:path"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,
            "items":[item("lib.rs", SHA), item("lib_test.rs", other)]
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    let search = |keyword: &str| -> GhSearchCodeQuery {
        serde_json::from_value(
            json!({"owner":"a","repo":"b","keywords":[keyword],"matchIn":["path"]}),
        )
        .expect("query")
    };
    let shared = execute(&provider, &search("app"), &context, &Passthrough)
        .await
        .expect("search");
    for file in shared.data["files"].as_array().expect("files") {
        assert_eq!(file["commitSha"], SHA, "{file}");
    }
    let mixed = execute(&provider, &search("lib"), &context, &Passthrough)
        .await
        .expect("search");
    assert!(mixed.data.get("commitSha").is_none(), "{}", mixed.data);
    let commits = mixed.data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|row| {
            (
                row["path"].as_str().unwrap_or_default(),
                row["commitSha"].as_str().unwrap_or_default(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(commits["lib.rs"], SHA, "{}", mixed.data);
    assert_eq!(commits["lib_test.rs"], other, "{}", mixed.data);
}
