//! Requests a search sends or refuses: invalid refs and globs before any request, one request for identical searches.
use super::*;

/// `ref` re-reads one repository's hits: without `repo` it is rejected as
/// invalid input before any request, and the repair is the same search
/// without it.
#[tokio::test]
async fn a_ref_without_a_repo_is_invalid_input_repaired_without_it() {
    let server = MockServer::builder().start().await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"o","keywords":["needle"],"ref":"dev","pageSize":5
    }))
    .expect("query");
    let context = default_context();
    let Err(failure) = super::super::run(&provider, &query, Ok(&context), &Passthrough).await
    else {
        panic!("a ref without a repo is rejected");
    };
    assert_eq!(
        failure.error.kind,
        octocode_github::ProviderErrorKind::Validation
    );
    assert!(
        failure.message.contains("needs repo"),
        "{}",
        failure.message
    );
    let next = failure.next.expect("repair");
    let repair = &next["repair"]["query"]["queries"][0];
    assert!(repair.get("ref").is_none(), "{next}");
    assert_eq!(repair["owner"], "o", "{next}");
    assert_eq!(repair["keywords"], json!(["needle"]), "{next}");
    assert_eq!(repair["pageSize"], 5, "{next}");
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// `filename:` matches names containing its text, never a glob: a glob is
/// invalid input before any request. One whose only wildcards are leading
/// or trailing `*` repairs to its text; any other names the rule.
#[tokio::test]
async fn a_filename_glob_is_refused_before_any_request() {
    let server = MockServer::builder().start().await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    async fn refused(
        provider: &octocode_github::GitHubProvider,
        context: &octocode_github::RequestContext,
        filename: &str,
    ) -> crate::tools::gh_shared::GhFailure {
        let query: GhSearchCodeQuery = serde_json::from_value(json!({
            "owner":"o","repo":"r","keywords":["describe"],"filename":filename
        }))
        .expect("query");
        match super::super::run(provider, &query, Ok(context), &Passthrough).await {
            Err(failure) => failure,
            Ok(_) => panic!("{filename}: a glob filename is refused"),
        }
    }
    for (glob, text) in [
        ("*.test.js", ".test.js"),
        ("README*", "README"),
        ("*util*", "util"),
    ] {
        let failure = refused(&provider, &context, glob).await;
        assert_eq!(
            failure.error.kind,
            octocode_github::ProviderErrorKind::Validation
        );
        assert!(failure.message.contains(glob), "{}", failure.message);
        let next = failure.next.expect("repair");
        let repair = &next["repair"]["query"]["queries"][0];
        assert_eq!(repair["filename"], text, "{next}");
        assert_eq!(repair["keywords"], json!(["describe"]), "{next}");
    }
    for glob in ["a*b.js", "*.{js,ts}", "*"] {
        let failure = refused(&provider, &context, glob).await;
        assert!(failure.next.is_none(), "{glob}");
        assert!(
            failure.message.contains("names containing its text"),
            "{}",
            failure.message
        );
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// `path` is a directory prefix, never a glob: a glob there is invalid input
/// before any request, so no lead ever carries it. Trailing `**`/`*`
/// segments repair to the directory, and a last `*.ext` segment to
/// `extensions`; any other glob names the rule.
#[tokio::test]
async fn a_path_glob_is_refused_before_any_request() {
    let server = MockServer::builder().start().await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    async fn refused(
        provider: &octocode_github::GitHubProvider,
        context: &octocode_github::RequestContext,
        path: &str,
    ) -> crate::tools::gh_shared::GhFailure {
        let query: GhSearchCodeQuery = serde_json::from_value(json!({
            "owner":"o","repo":"r","keywords":["SearcherBuilder"],"path":path
        }))
        .expect("query");
        match super::super::run(provider, &query, Ok(context), &Passthrough).await {
            Err(failure) => failure,
            Ok(_) => panic!("{path}: a glob path is refused"),
        }
    }
    for (glob, dir, extensions) in [
        ("crates/**", Some("crates"), None),
        ("crates/*", Some("crates"), None),
        (
            "crates/runtime/src/**/*.rs",
            Some("crates/runtime/src"),
            Some(json!(["rs"])),
        ),
        ("**/*.rs", None, Some(json!(["rs"]))),
    ] {
        let failure = refused(&provider, &context, glob).await;
        assert_eq!(
            failure.error.kind,
            octocode_github::ProviderErrorKind::Validation
        );
        assert!(failure.message.contains(glob), "{}", failure.message);
        let next = failure.next.expect("repair");
        let repair = &next["repair"]["query"]["queries"][0];
        assert_eq!(repair.get("path").and_then(|p| p.as_str()), dir, "{next}");
        assert_eq!(repair.get("extensions").cloned(), extensions, "{next}");
        assert_eq!(repair["keywords"], json!(["SearcherBuilder"]), "{next}");
    }
    for glob in ["src/*/lib", "src/{a,b}", "src/foo?.rs"] {
        let failure = refused(&provider, &context, glob).await;
        assert!(failure.next.is_none(), "{glob}");
        assert!(
            failure.message.contains("directory prefix"),
            "{}",
            failure.message
        );
    }
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

/// Identical searches in one batch share one request: the second waits for
/// the first and reads the page it stored, so the duplicate spends no
/// code-search quota.
#[tokio::test]
async fn identical_concurrent_searches_send_one_request() {
    let server = MockServer::builder().start().await;
    let search = Delayed::new(
        Duration::from_millis(200),
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })),
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(search.clone())
        .mount(&server)
        .await;
    let mut provider = one_shot_provider(&server);
    provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["app"],"matchIn":"path"
    }))
    .expect("query");
    let context = default_context();
    let (first, second) = futures_util::future::join(
        execute(&provider, &query, &context, &Passthrough),
        execute(&provider, &query, &context, &Passthrough),
    )
    .await;
    assert_eq!(
        first.expect("first").data["files"],
        second.expect("second").data["files"]
    );
    assert_eq!(search.arrivals.count(), 1, "one GET /search/code");
}
