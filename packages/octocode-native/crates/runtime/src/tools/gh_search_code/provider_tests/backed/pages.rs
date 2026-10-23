//! Page facts: partial coverage, the terminal capped page and several extensions on one page.
use super::*;

/// A non-empty page GitHub marks incomplete says so in its data,
/// without extras, separately from whether another page exists.
#[tokio::test]
async fn a_non_empty_incomplete_page_reports_partial_coverage() {
    let server = MockServer::builder().start().await;
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
                "keywords":["wrap_app"],"matchIn":"path"}),
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

#[tokio::test]
async fn capped_search_is_terminal_only_on_the_last_reachable_page() {
    let server = MockServer::builder().start().await;
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

/// Several `extensions` are one call for the agent: one indexed search
/// per extension (GitHub ANDs repeated qualifiers), merged into one page.
/// The searches share `pageSize`, so the page does not grow with them.
#[tokio::test]
async fn several_extensions_share_one_page() {
    let server = MockServer::builder().start().await;
    for (extension, file) in [("ts", "a.ts"), ("js", "b.js")] {
        Mock::given(method("GET"))
            .and(path("/api/v3/search/code"))
            .and(wiremock::matchers::query_param_contains(
                "q",
                format!("extension:{extension}"),
            ))
            .and(wiremock::matchers::query_param("per_page", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":1,"incomplete_results":false,"items":[code_item(file)]
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let out = run(
        &server,
        json!({"operation":"code","owner":"a","keywords":["needle"],"extensions":["ts","js"],"pageSize":10}),
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
