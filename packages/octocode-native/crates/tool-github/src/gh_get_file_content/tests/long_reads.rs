//! Reads longer than a page: a long declaration, a match inside a long line,
//! and a plain read that reaches a long line. The provider is mocked.
use super::*;
use octocode_contract_types::ReadProgressUnit;
use octocode_test_support::json_fixture;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn serve(server: &MockServer, name: &str, body: &str) {
    mount_json(
        server,
        format!("/api/v3/repos/a/b/contents/{name}"),
        200,
        json!({"type":"file","encoding":"base64","content":STANDARD.encode(body)}),
    )
    .await;
}

/// Walk `next.nextPage` of `query` to its end: each page's stated place
/// (unit, offset) and text, in walk order.
async fn walk(
    provider: &GitHubProvider,
    mut query: Value,
) -> Vec<(ReadProgressUnit, usize, String)> {
    let mut pages = Vec::new();
    loop {
        let result = read(provider, query).await;
        let file = &result.file;
        assert_eq!(file.content.error_code, None, "{:?}", file.content);
        let page = file.content.pagination.as_ref().expect("pagination");
        pages.push((
            page.unit,
            page.offset,
            file.content.content.clone().unwrap_or_default(),
        ));
        assert!(pages.len() < 200, "runaway paging");
        let next = file
            .calls
            .as_ref()
            .map(|next| next["nextPage"].clone())
            .filter(|next| !next.is_null());
        // The wire states the walk: `hasMore` exactly when a next call exists.
        let wire = serde_json::to_value(&file.content).expect("row");
        assert_eq!(
            wire["pagination"]["hasMore"],
            json!(next.is_some()),
            "{}",
            wire["pagination"]
        );
        match next {
            Some(next) => query = next["input"]["queries"][0].clone(),
            None => return pages,
        }
    }
}

/// `block` on a declaration longer than a page reads it from its first
/// line: page 1 is as full as a plain read's, and `next.nextPage` alone
/// reaches its last line.
#[tokio::test]
async fn a_long_block_fills_its_first_page_and_pages_to_its_end() {
    let server = MockServer::builder().start().await;
    let body: String = (0..3_000)
        .map(|i| format!("  const value{i} = compute({i});\n"))
        .collect();
    let declaration = format!("export function createServer() {{\n{body}  return 0;\n}}\n");
    let source = format!("import x from 'x';\n\n{declaration}\nexport const after = 1;\n");
    serve(&server, "index.ts", &source).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let base = json!({"owner":"a","repo":"b","path":"index.ts","ref":SHA});
    let plain = read(&provider, base.clone()).await;
    let plain_page = plain.file.content.pagination.clone().expect("pagination");
    let mut query = base;
    query["matchString"] = json_fixture(r#"["function createServer"]"#);
    query["block"] = json!(true);
    let first = read(&provider, query.clone()).await;
    let file = &first.file.content;
    assert!(
        !file
            .warnings
            .iter()
            .any(|warning| warning.starts_with("block:")),
        "{:?}",
        file.warnings
    );
    let page = file.pagination.as_ref().expect("pagination");
    assert_eq!(page.unit, ReadProgressUnit::Lines);
    assert!(
        page.length + 3 >= plain_page.length,
        "page 1 holds a full page of the body: {page:?} against {plain_page:?}"
    );
    assert_eq!(file.blocks.len(), 1, "{:?}", file.blocks);
    let pages = walk(&provider, query).await;
    assert!(pages.len() > 2);
    let text: String = pages.iter().map(|(_, _, text)| text.as_str()).collect();
    assert_eq!(text, declaration, "the declaration, every line once");
}

/// A match inside a line too long for a page is on page 1 with the bytes
/// around it, at the byte offset the page states. When page 1 holds every
/// match of the file's last line, the rest of that line is no required page
/// (Q4): the row ends there and offers the whole line as a related read.
#[tokio::test]
async fn a_match_in_a_long_last_line_is_on_page_one_and_the_rest_is_a_related_read() {
    let server = MockServer::builder().start().await;
    let filler: String = (0..12_000).map(|at| format!("f{at:06};")).collect();
    let line = format!(
        "{}TOKEN{}noConflict{}",
        &filler[..40_000],
        &filler[40_000..80_000],
        &filler[80_000..]
    );
    let source = format!("/*! banner */\n{line}\n");
    serve(&server, "lib.min.js", &source).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query = json!({
        "owner":"a","repo":"b","path":"lib.min.js","ref":SHA,"matchString":["noConflict"]
    });
    let view = format!("{line}\n").replace("TOKEN", "[REDACTED]");
    let hit = view.find("noConflict").expect("hit");
    let first = read(&provider, query.clone()).await;
    let file = &first.file.content;
    let page = file.pagination.as_ref().expect("pagination");
    let text = file.content.as_deref().expect("content");
    assert_eq!(page.unit, ReadProgressUnit::Bytes);
    assert!(text.contains("noConflict"), "{text}");
    assert!(text.len() <= 400, "{} bytes", text.len());
    assert!(
        page.offset < hit && hit < page.offset + page.length,
        "{page:?}"
    );
    assert_eq!(view.get(page.offset..page.offset + page.length), Some(text));
    // No required page follows; the whole line is one optional read.
    let pages = walk(&provider, query).await;
    assert_eq!(pages.len(), 1, "{pages:?}");
    let calls = first.file.calls.as_ref().expect("calls");
    assert!(calls.get("nextPage").is_none(), "{calls}");
    let read = calls
        .as_object()
        .into_iter()
        .flatten()
        .find_map(|(_, call)| {
            (call["tool"] == "ghGetFileContent" && call["kind"] == "related").then_some(call)
        })
        .unwrap_or_else(|| panic!("a related whole-line read: {calls}"));
    let input = &read["input"]["queries"][0];
    assert_eq!(input["path"], "lib.min.js", "{read}");
    assert_eq!(
        input["ranges"],
        json!([{"startLine": 2, "endLine": 2}]),
        "{read}"
    );
    assert!(input.get("matchString").is_none(), "{read}");
}

/// A plain read whose first line is short and whose second is too long
/// for a page fills page 1 with the long line's first bytes.
#[tokio::test]
async fn a_plain_read_fills_its_page_into_the_long_line() {
    let server = MockServer::builder().start().await;
    let filler: String = (0..11_000).map(|at| format!("f{at:06};")).collect();
    let source = format!("/*! banner | license */\n{filler}\n");
    serve(&server, "lib.min.js", &source).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query = json!({"owner":"a","repo":"b","path":"lib.min.js","ref":SHA});
    let pages = walk(&provider, query).await;
    let (unit, offset, text) = &pages[0];
    assert_eq!((*unit, *offset), (ReadProgressUnit::Bytes, 0));
    assert!(
        text.starts_with("/*! banner | license */\nf000000;"),
        "{text:.80}"
    );
    assert!(
        text.len() >= octocode_tools_shared::source_view::pagination::MIN_BYTE_PAGE,
        "a full page: {} bytes",
        text.len()
    );
    let joined: String = pages.iter().map(|(_, _, text)| text.as_str()).collect();
    assert_eq!(joined, source, "in order, every byte once");
}

/// G7: a walk through a line several pages long keeps one format as it is
/// published. Every row that starts a source line carries its `N\t` gutter,
/// on the byte pages as on the line page after them; a page that opens
/// inside the long line states the line it continues (`continuesLine`) and
/// leaves that first row bare; no page falls back to `sourceLineRanges`.
/// The rows without their gutters, joined in page order, are the file, and
/// each page's `length` counts source bytes, never a gutter.
#[tokio::test]
async fn a_long_line_walk_is_numbered_on_every_page_and_rebuilds_the_file() {
    use octocode_runtime_base::id::ToolId;
    let server = MockServer::builder().start().await;
    let filler: String = (0..40_000).map(|at| format!("f{at:06};")).collect();
    let source = format!("/*! banner | license */\n{filler}\nlast();\nend();\n");
    serve(&server, "lib.min.js", &source).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let mut query = json!({"owner":"a","repo":"b","path":"lib.min.js","ref":SHA});
    let (mut rebuilt, mut pages, mut continued) = (String::new(), 0, 0);
    let mut last_line = 0;
    loop {
        let result = read(&provider, query).await;
        let file = &result.file;
        let mut published = json!({"results": [{"index": 0,
            "data": serde_json::to_value(&file.content).expect("row")}]});
        octocode_runtime_base::response::numbered::number_read_rows(
            ToolId::GhGetFileContent,
            &mut published,
        );
        let data = &published["results"][0]["data"];
        assert!(
            data.get("sourceLineRanges").is_none(),
            "page {pages}: {data:.300}"
        );
        let content = data["content"].as_str().expect("content");
        let continues = data.get("continuesLine").and_then(Value::as_u64);
        let mut shown = 0;
        for (at, row) in content.split_inclusive('\n').enumerate() {
            let text = if at == 0 && continues.is_some() {
                assert_eq!(
                    continues,
                    Some(last_line),
                    "it continues the last line shown"
                );
                row
            } else {
                let (number, text) = row.split_once('\t').expect("a gutter");
                let line: u64 = number.parse().expect("a line number");
                assert_eq!(line, last_line + 1, "page {pages}: lines in order");
                last_line = line;
                text
            };
            shown += text.len();
            rebuilt.push_str(text);
        }
        continued += usize::from(continues.is_some());
        if data["pagination"]["unit"] == "bytes" {
            assert_eq!(data["pagination"]["length"], shown, "page {pages}");
        }
        pages += 1;
        assert!(pages < 200, "runaway paging");
        let next = file
            .calls
            .as_ref()
            .map(|next| next["nextPage"].clone())
            .filter(|next| !next.is_null());
        match next {
            Some(next) => query = next["input"]["queries"][0].clone(),
            None => break,
        }
    }
    assert_eq!(rebuilt, source, "in order, every byte once");
    assert!(
        continued >= 3,
        "the long line spans pages: {continued} of {pages}"
    );
    assert_eq!(last_line as usize, source.lines().count());
}

/// A `ranges` read that ends on a long line which is not the file's
/// last walks that line to its end and states `hasMore:false` there
/// ([`walk`] checks every page's wire); the pages are the line exactly once.
#[tokio::test]
async fn a_range_ending_on_a_long_line_ends_its_walk() {
    let server = MockServer::builder().start().await;
    let line: String = (0..6_000).map(|at| format!("f{at:06};")).collect();
    let source = format!("/*! banner */\n{line}\nshort two\nshort three\n");
    serve(&server, "lib.min.js", &source).await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let query = json!({
        "owner":"a","repo":"b","path":"lib.min.js","ref":SHA,
        "ranges":[{"startLine":2,"endLine":2}]
    });
    let pages = walk(&provider, query).await;
    assert!(pages.len() > 1, "a byte walk");
    let joined: String = pages.iter().map(|(_, _, text)| text.as_str()).collect();
    assert_eq!(joined, format!("{line}\n"), "in order, every byte once");
}
