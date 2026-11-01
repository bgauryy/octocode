//! Continuations: default-view fields, unsized pages, the row envelope and unknown windows.
use super::*;

/// Continuations and leads carry only fields that change the read: the
/// default view (`fullContent` off, raw `minify: none`, sent or not) is
/// omitted, a chosen `minify: standard` is kept, and a whole-file read
/// pages without `fullContent` in the raw view its first page used.
#[tokio::test]
async fn continuations_omit_default_view_fields() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=6000).map(|i| format!("line of text {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha});
    for view in ["content", "compact"] {
        let mut query = base.clone();
        query["view"] = json!(view);
        let result = read(&provider, query).await;
        let next = result.file.calls.expect("continuation");
        let page = &next["nextPage"]["input"]["queries"][0];
        assert_eq!(page["view"], view);
        assert_eq!(page["ref"], sha);
        assert!(
            page["readProgress"]["offset"]
                .as_u64()
                .is_some_and(|offset| offset > 0)
        );
        assert!(page.get("length").is_none());
    }
}

/// An unsized page is paged as localFetch pages it: every later page's
/// continuation stays unsized (no injected `length`), and the chain
/// still reaches the file's end.
#[tokio::test]
async fn unsized_page_continuations_stay_unsized() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=6000).map(|i| format!("line of text {i}\n")).collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let mut query = serde_json::json!({"owner":"a","repo":"b","path":"big.txt","ref":sha});
    let mut pages = 0;
    loop {
        let result = read(&provider, query.clone()).await;
        pages += 1;
        let Some(next) = result.file.calls.clone() else {
            break;
        };
        let page = next["nextPage"]["input"]["queries"][0].clone();
        assert!(page.get("length").is_none(), "page {pages}: {next}");
        assert!(page.get("unit").is_none(), "page {pages}: {next}");
        assert!(pages < 100, "the chain never ends: {next}");
        query = page;
    }
    assert!(pages > 2, "the file spans several pages");
}

/// The shared read is cut to the row's window less what a GitHub row adds
/// around it ([`row_envelope_chars`]: the repository and commit beside the
/// content and on each rewritten continuation), so the whole row fits the
/// window its read was given and no response stage reads it again. A
/// whole file just under the window, but over it with the envelope, pages.
#[tokio::test]
async fn the_shared_read_leaves_room_for_the_github_row_envelope() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let body: String = (1..=200)
        .map(|i| format!("line {i:03} of text\n"))
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode(&body)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query = |fields: Value| -> GhGetFileContentQuery {
        let mut row = json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":sha});
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        serde_json::from_value(row).expect("query")
    };
    let envelope = row_envelope_chars(&query(json!({})), sha);
    let window = body.len() + envelope / 2;
    let whole = execute_default_regex(
        &provider,
        &query(json!({})),
        &default_context(),
        Some(window),
    )
    .await
    .expect("whole read");
    assert!(
        whole
            .file
            .calls
            .as_ref()
            .is_some_and(|next| next.get("nextPage").is_some()),
        "a whole file within the window but not its envelope pages: {:?}",
        whole.file.calls
    );
    // The envelope covers what the GitHub row adds to the shared read's row.
    let page = query(json!({"ranges":[{"startLine":1,"endLine":20}]}));
    let github = execute_default_regex(&provider, &page, &default_context(), Some(1 << 20))
        .await
        .expect("line page");
    let shared = process_fetched_content(
        &local_query(&page).expect("local query"),
        body.as_bytes(),
        Path::new("src/lib.rs"),
        &crate::tools::source_view::SourceFacts {
            views: Default::default(),
            modified: None,
            window: Some(1 << 20),
            beside: 0,
            interior: false,
            byte_origin: None,
        },
        &Safe,
        &NeverCancel,
    );
    let github_chars =
        octocode_engine::json_size::utf16_units(&serde_json::to_value(&github).expect("row"))
            .expect("size");
    let shared_chars = octocode_engine::json_size::utf16_units(&shared).expect("size");
    assert!(
        github_chars <= shared_chars + envelope,
        "{github_chars} > {shared_chars} + {envelope}"
    );
}

/// A minified read of a comment-only range is empty because the view
/// removed every line, not because the coordinates are wrong: it says so
/// and leads to the same read as is at the read commit.
#[tokio::test]
async fn a_minified_comment_range_leads_to_the_read_as_is() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let source = format!("//! doc one\n//! doc two\n{}", "pub fn f() {}\n".repeat(20));
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(&source)}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":sha,
            "view":"compact","ranges":[{"startLine": 1, "endLine": 2}]}),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    let data = &read.data;
    assert_eq!(data["content"], "", "{data}");
    assert!(
        data["hints"].as_array().is_some_and(|hints| hints
            .iter()
            .any(|h| h.as_str().is_some_and(|h| h.contains("compact")))),
        "{data}"
    );
    let row = &crate::tools::test_support::checks::related_call(data, "ghGetFileContent", |_| true)
        ["input"]["queries"][0];
    assert_eq!(
        crate::tools::test_support::checks::related_call(data, "ghGetFileContent", |_| true)["tool"],
        "ghGetFileContent",
        "{data}"
    );
    assert_eq!(row["view"], "content");
    assert_eq!(
        row["ranges"],
        json!([{"startLine":1,"endLine":2}]),
        "{data}"
    );
    assert_eq!(row["ref"], sha, "{data}");
    assert!(data["next"].get("findCode").is_none(), "{data}");
}

/// A file's unknown windows read in as few calls as one call's rows allow:
/// `unfilteredUnknown`, `unfilteredUnknown2`, … each a full call but the
/// last, never a lone first read beside the rest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_windows_fill_each_call_before_the_next() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let per_call = crate::response::result::rows_per_call(crate::id::ToolId::GhGetFileContent);
    let ranges_per_read = crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
    // One read more than a call holds; the last read takes one range.
    let unknown = ranges_per_read * per_call + 1;
    // Each line is its own chunk (over half the chunk target), alternating
    // unknown and kept, so every unknown line is its own range.
    let body: String = (0..2 * unknown)
        .map(|i| {
            let word = if i % 2 == 0 { "maybe" } else { "keep" };
            format!("{word} {}\n", "x".repeat(1_100))
        })
        .collect();
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/w.txt",
        200,
        json!({
            "type":"file","encoding":"base64","content":STANDARD.encode(&body)
        }),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query: GhGetFileContentQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","path":"w.txt","ref":sha,
        "classification":{"predicates":["Is it kept?"]}
    }))
    .expect("query");
    let judge = crate::tools::test_support::selection_fake::WordJudge { fail: None };
    // One page judges every line, so every unknown range is on this page.
    let mut shaping = shaping(judge);
    shaping.selection.limits.max_candidates = 2 * unknown;
    shaping.selection.limits.max_chars = 2 * body.len();
    let result = execute(
        &provider,
        &query,
        &fixture_context(Duration::from_secs(10), 1 << 22),
        Some(2 * body.len()),
        shaping,
    )
    .await
    .expect("read");
    let next = result.file.calls.expect("leads");
    let batches = std::iter::once("unfilteredUnknown".to_owned())
        .chain((2..).map(|n| format!("unfilteredUnknown{n}")))
        .map_while(|name| next[&name]["input"]["queries"].as_array().map(Vec::len))
        .collect::<Vec<_>>();
    let reads: usize = batches.iter().sum();
    assert_eq!(
        reads,
        unknown.div_ceil(ranges_per_read),
        "{unknown} unknown lines at {ranges_per_read} ranges a read: {next}"
    );
    assert!(batches.len() > 1, "more reads than one call holds: {next}");
    assert_eq!(batches.len(), reads.div_ceil(per_call), "{next}");
    assert!(
        batches[..batches.len() - 1].iter().all(|n| *n == per_call),
        "{next}"
    );
}
