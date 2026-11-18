//! Continuations: default-view fields, unsized pages, the row envelope and unknown windows.
use super::*;
use octocode_test_support::json_fixture;

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
    for (view, reads) in [
        ("content", octocode_contract_types::ReadView::Content),
        ("compact", octocode_contract_types::ReadView::Compact),
    ] {
        let mut query = base.clone();
        query["view"] = json!(view);
        let result = read(&provider, query).await;
        let next = result.file.calls.expect("continuation");
        let page = &next["nextPage"]["input"]["queries"][0];
        assert_eq!(
            octocode_tools_shared::test_support::read_view(
                octocode_runtime_base::id::ToolId::GhGetFileContent,
                page
            ),
            reads
        );
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
    let page = query(json_fixture(
        r#"{"ranges": [{"startLine": 1, "endLine": 20}]}"#,
    ));
    let github = execute_default_regex(&provider, &page, &default_context(), Some(1 << 20))
        .await
        .expect("line page");
    let shared = process_fetched_content(
        &local_query(&page).expect("local query"),
        body.as_bytes(),
        Path::new("src/lib.rs"),
        &octocode_tools_shared::source_view::SourceFacts {
            window: Some(1 << 20),
            beside: 0,
            interior: false,
            byte_origin: None,
            file_lines: None,
            local_file: false,
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
    let row = &octocode_runtime_base::test_support::checks::related_call(
        data,
        "ghGetFileContent",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(data, "ghGetFileContent", |_| {
            true
        })["tool"],
        "ghGetFileContent",
        "{data}"
    );
    assert_eq!(
        octocode_tools_shared::test_support::read_view(
            octocode_runtime_base::id::ToolId::GhGetFileContent,
            row
        ),
        octocode_contract_types::ReadView::Content,
        "{data}"
    );
    assert_eq!(
        row["ranges"],
        json_fixture(r#"[{"startLine": 1, "endLine": 2}]"#),
        "{data}"
    );
    assert_eq!(row["ref"], sha, "{data}");
    assert!(data["next"].get("findCode").is_none(), "{data}");
}
