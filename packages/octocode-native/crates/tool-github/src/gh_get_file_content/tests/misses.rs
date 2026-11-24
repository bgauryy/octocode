//! Missed matches: empty reads, their leads and the fallback hint.
use super::*;
use octocode_test_support::json_fixture;

/// The last-change read answers: the path has no history.
async fn mount_no_history(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "data": {"repository": {"object": {"history": {"nodes": []}}}}
            }"#,
        )))
        .mount(server)
        .await;
}

/// A matchString with no hit in an existing file is `matchNotFound` with
/// an empty `content` (the contract requires it, as in localFetch) and
/// its leads; it carries no zero counters, no echo of the request and no
/// hint restating the absence `status` and `matchNotFound` state.
#[tokio::test]
async fn no_match_read_keeps_its_leads_without_empty_echo() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\ntwo\n")}),).await;
    mount_no_history(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":["absent"]}),
    )
    .await
    .expect("read");
    let data = &read.data;
    assert_eq!(read.status, Some("empty"), "{data}");
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert_eq!(data["content"], "", "{data}");
    assert_eq!(
        data["commitSha"], sha,
        "a pinned read retains its source: {data}"
    );
    octocode_runtime_base::test_support::validate_sealed(
        "ghGetFileContent",
        &serde_json::json!({"results":[{"index":0,"status":"empty","data":data}]}),
    )
    .expect("an empty match row satisfies the output contract");
    for junk in [
        "returnedLines",
        "returnedChars",
        "searchedFor",
        "pagination",
        "warnings",
    ] {
        assert!(data.get(junk).is_none(), "{junk}: {data}");
    }
    // A miss at a pinned ref offers no code search (its index is the
    // default branch's) and no history leads; the row states the absence.
    assert!(data.get("hints").is_none(), "{data}");
    assert!(data.get("related").is_none(), "{data}");
}

/// A miss offers only reads that differ from the request and that select
/// lines of the redacted text: no ignoreCase lead for a read already
/// case-insensitive, none for a hit hidden behind a redaction, and a
/// `ranges` miss with hits elsewhere in this file leads to the same read
/// without `ranges`, not back to a repository search.
#[tokio::test]
async fn a_miss_leads_only_to_reads_that_select_visible_lines() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nkey TOKEN\nthree\nfour needle\n")}),).await;
    mount_no_history(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha});
    let with = |fields: Value| {
        let mut row = base.clone();
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        row
    };
    for fields in [
        json_fixture(r#"{"matchString": ["token"]}"#),
        json_fixture(r#"{"matchString": ["TOKEN"], "caseMode": "insensitive"}"#),
    ] {
        let read = run_default(&provider, with(fields.clone()))
            .await
            .expect("read");
        let data = &read.data;
        assert_eq!(data["matchNotFound"], true, "{fields}: {data}");
        assert!(data["next"].get("ignoreCase").is_none(), "{fields}: {data}");
        // The hit hidden behind a redaction bounds the absence: the one
        // warning says the masked value was never searched.
        assert_eq!(
            data["warnings"],
            serde_json::json!([super::super::shape::masked_coverage(Some(1))]),
            "{fields}: {data}"
        );
    }
    let ranged = run_default(
        &provider,
        with(json_fixture(
            r#"{
                "matchString": ["needle"],
                "ranges": [{"startLine": 1, "endLine": 2}]
            }"#,
        )),
    )
    .await
    .expect("read");
    let data = &ranged.data;
    assert_eq!(data["matchNotFound"], true, "{data}");
    // The miss the file itself answers states its one hint, a counted fact.
    assert!(data.get("warnings").is_none(), "{data}");
    let hints = data["hints"]
        .as_array()
        .unwrap_or_else(|| panic!("one hint: {data}"));
    assert_eq!(hints.len(), 1, "{data}");
    assert!(
        hints[0]
            .as_str()
            .is_some_and(|hint| hint.contains("1 line matches outside")),
        "{data}"
    );
    let lead =
        octocode_runtime_base::test_support::checks::related_call(data, "ghGetFileContent", |_| {
            true
        });
    assert_eq!(lead["tool"], "ghGetFileContent", "{data}");
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["matchString"], json_fixture(r#"["needle"]"#), "{data}");
    assert!(row.get("ranges").is_none(), "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    assert!(data["next"].get("findCode").is_none(), "{data}");
}

/// A literal that a large file does not contain leads to a repository
/// code search for it (it may live in another file), never to a
/// semantic locate of the same file.
#[tokio::test]
async fn a_pinned_miss_in_a_large_file_offers_no_code_search() {
    let server = MockServer::builder().start().await;
    let text = (0..1200).map(|n| format!("line {n}\n")).collect::<String>();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&text)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","ref":"0123456789abcdef0123456789abcdef01234567","path":"big.txt","matchString":["absent"]}),
    )
    .await
    .expect("read");
    let data = &read.data;
    assert!(data["next"].get("clasify").is_none(), "{data}");
    // A pinned read gets no code search: its index is the default branch's.
    assert!(
        data["related"]
            .as_array()
            .is_none_or(|calls| calls.iter().all(|call| call["tool"] != "ghSearchCode")),
        "{data}"
    );
}

/// A miss over a file holding masked secret values claims no absence inside
/// them: one warning says masked values are never searched, in the wording
/// localFetch and localSearch use, and no hint; a file with none has
/// neither.
#[tokio::test]
async fn a_miss_over_masked_values_warns_they_were_never_searched() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(&server, "/api/v3/repos/a/b/contents/a.txt", 200, serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("one\nkey TOKEN\nthree\n")}),).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"a.txt","ref":sha,"matchString":["absent"]}),
    )
    .await
    .expect("read");
    let data = &read.data;
    assert_eq!(data["matchNotFound"], true, "{data}");
    assert!(data.get("hints").is_none(), "{data}");
    let masked = data["maskedSpans"]
        .as_u64()
        .and_then(|count| usize::try_from(count).ok())
        .expect("masked spans");
    assert_eq!(
        data["warnings"],
        serde_json::json!([super::super::shape::masked_coverage(Some(masked))]),
        "{data}"
    );
    let clause = octocode_tools_shared::source_view::masked_unsearched(masked);
    assert!(
        data["warnings"][0]
            .as_str()
            .is_some_and(|warning| warning.contains(&clause)),
        "{data}"
    );
    for none in [None, Some(0)] {
        assert_eq!(super::super::shape::masked_coverage(none), None);
    }
}

/// A symbols read of a file no outline can serve is answered, not refused:
/// the row is the file's content with `viewFallback` naming the view served,
/// and it carries no error for a recovery hint to answer.
#[tokio::test]
async fn a_symbols_read_with_no_outline_serves_the_content_with_a_fallback() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = "plain notes\nwith no declaration\n";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/notes.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(text)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"notes.txt","ref":sha,"view":"symbols"}),
    )
    .await
    .expect("read");
    let data = &read.data;
    assert!(data.get("errorCode").is_none(), "{data}");
    assert!(data.get("error").is_none(), "{data}");
    assert_eq!(data["viewFallback"]["requested"], "symbols", "{data}");
    assert_eq!(data["viewFallback"]["applied"], "content", "{data}");
    let served = data["content"].as_str().expect("content");
    for line in text.lines() {
        assert!(served.contains(line), "{line} missing: {data}");
    }
}

/// A language-specific read the file's language cannot serve names its own
/// recovery. The shared hint would send a GitHub read to the local
/// filesystem tools, so this tool states none.
#[test]
fn an_unsupported_language_failure_gets_no_local_tool_hint() {
    use octocode_runtime_base::response::output::ToolOutput;
    assert_eq!(
        super::super::Output.error_hint(ErrorCode::LanguageUnsupported, &serde_json::Value::Null),
        Some(octocode_runtime_base::response::output::NO_HINT),
    );
}

/// A miss in a file with an outline leads first to that outline (the
/// shared miss's read of its declarations, offered once), then to the
/// repository's code and the path's history: the outline answers nothing
/// by itself, so the repository's leads stay. The hint names that order.
#[tokio::test]
async fn a_pinned_miss_in_an_outlined_file_leads_to_its_outline() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/a.rs",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("fn one() {}\n")}),
    )
    .await;
    mount_no_history(&server).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","ref":"0123456789abcdef0123456789abcdef01234567","path":"a.rs","matchString":["absent"]}),
    )
    .await
    .expect("read");
    let data = &read.data;
    let leads = data["related"].as_array().expect("leads");
    let at = |tool: &str, view: Option<&str>| {
        leads
            .iter()
            .position(|lead| {
                lead["tool"] == tool
                    && view.is_none_or(|view| lead["input"]["queries"][0]["view"] == view)
            })
            .unwrap_or_else(|| panic!("{tool} {view:?}: {data}"))
    };
    let outlines = leads
        .iter()
        .filter(|lead| lead["input"]["queries"][0]["view"] == "symbols")
        .count();
    assert_eq!(outlines, 1, "one outline lead: {data}");
    at("ghGetFileContent", Some("symbols"));
    assert!(
        leads.iter().all(|lead| lead["tool"] != "ghSearchCode"),
        "a pinned read gets no code search: {data}"
    );
    // The leads' order is `related[]`'s own: no hint restates it.
    assert!(data.get("hints").is_none(), "{data}");
}
