//! A hit too large to line-scan is disclosed with its bound, and the other hits still resolve.
use super::*;
use crate::tools::gh_shared::test_support::fixture_context;

#[tokio::test]
async fn an_oversized_blob_is_named_with_its_bound_and_a_small_one_still_resolves() {
    let bound = crate::tools::gh_search_code::lines::MAX_SCAN_BYTES;
    let out = search_beside_a_big_blob(bound + 1, 4 * bound).await;
    assert_bound_named(&out, bound);
}

/// Under the default 1 MiB context a blob between that and the scan bound is
/// refused by the context's limit: the warning names the limit that refused
/// it, not a larger bound the blob is under.
#[tokio::test]
async fn a_blob_under_the_scan_bound_is_named_with_the_smaller_limit_that_refused_it() {
    let context_bytes = 1 << 20;
    let big = context_bytes + context_bytes / 2;
    assert!(big < crate::tools::gh_search_code::lines::MAX_SCAN_BYTES);
    let out = search_beside_a_big_blob(big, context_bytes).await;
    assert_bound_named(&out, context_bytes);
}

/// A search over `big.txt` (`big` bytes) and a small `small.txt`, read under
/// a context of `context_bytes`.
async fn search_beside_a_big_blob(
    big: usize,
    context_bytes: usize,
) -> crate::response::result::ToolData {
    let server = MockServer::builder().start().await;
    let items: Vec<_> = ["big.txt", "small.txt"]
        .iter()
        .map(|name| {
            json!({"name":name,"path":name,"sha":format!("blob-{name}"),
             "html_url":format!("https://github.com/a/b/blob/{SHA}/{name}"),
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,"items":items
        })))
        .mount(&server)
        .await;
    for (name, body) in [
        ("big.txt", "x".repeat(big)),
        ("small.txt", "one\nneedle\n".to_owned()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/vnd.github.raw; charset=utf-8")
                    .insert_header("etag", format!("\"{SHA}\""))
                    .set_body_bytes(body.into_bytes()),
            )
            .mount(&server)
            .await;
    }
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["needle"]}))
            .expect("query");
    execute(
        &one_shot_provider(&server),
        &query,
        &fixture_context(Duration::from_secs(5), context_bytes),
        &Passthrough,
        &crate::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
}

fn assert_bound_named(out: &crate::response::result::ToolData, bound: usize) {
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains("line-scan read bound") && warnings.contains(&bound.to_string()),
        "{}",
        out.data
    );
    let file = |name: &str| {
        out.data["files"]
            .as_array()
            .and_then(|files| files.iter().find(|row| row["path"] == name))
            .unwrap_or_else(|| panic!("{name} row: {}", out.data))
    };
    assert_eq!(
        file("big.txt")["lineResolved"],
        json!(false),
        "{}",
        out.data
    );
    assert!(
        file("small.txt")["lines"].to_string().contains("needle"),
        "{}",
        out.data
    );
}
