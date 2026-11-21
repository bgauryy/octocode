//! A whole read redacts its view before any window is cut: byte offsets count
//! the redacted view, each window ends on a clean cut (never inside a
//! character, a CRLF pair or a redaction marker), a cursor naming any other
//! byte is refused, and only a page that shows a redaction says so.
use super::*;
use octocode_contract_types::{ErrorCode, ReadProgressUnit};
use octocode_test_support::json_fixture;

const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
const WINDOW: usize = 4_000;

/// One minified line: multi-byte text, quotes to escape, and `token` at each
/// of `at` (byte offsets of the filler, snapped to a character).
fn minified(test: &str, at: &[usize]) -> String {
    let mut line = format!("{test};");
    let mut index = 0;
    while line.len() < 120_000 {
        if at
            .iter()
            .any(|at| line.len() <= *at && *at < line.len() + 40)
        {
            line.push_str(&format!("var token=\"{TOKEN}\";"));
        }
        line.push_str(&format!("function f{index}(a){{return \"é{index}\"+a;}}"));
        index += 1;
    }
    line.push('\n');
    line
}

/// Every page of `query`, each with the query that read it.
fn walk(query: &LocalFetchQuery, paths: &Paths) -> Vec<(LocalFetchQuery, LocalFetchResult)> {
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let mut pages = Vec::new();
    let mut query = query.clone();
    loop {
        let page = execute_local_fetch(&query, paths, &security, &NeverCancel, Some(WINDOW), false);
        assert_eq!(page.error_code, None, "{page:?}");
        let next = page
            .next
            .as_ref()
            .and_then(|next| next.continue_window.clone());
        pages.push((query, page));
        assert!(pages.len() < 2_000, "walk does not end");
        let Some(next) = next else {
            return pages;
        };
        query = next.query;
    }
}

/// One read under the real content scan and no response window.
fn real(query: &LocalFetchQuery, paths: &Paths) -> LocalFetchResult {
    fetch_with(
        query,
        paths,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
}

fn redacted_view(source: &str, path: &Path) -> String {
    octocode_runtime_base::security::ContentSecurity::new()
        .redact_source_lines(source, path)
        .expect("scan")
        .0
        .into_owned()
}

/// Byte offsets count the redacted view: every page starts where the pages
/// before it end, and the pages join to that view exactly once.
#[test]
fn byte_pages_count_and_rejoin_the_redacted_view() {
    let fixture = Fixture::new();
    let source = minified("rejoin", &[30_000, 70_000]);
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let view = redacted_view(&source, &path);
    assert_ne!(view, source, "the fixture holds a secret");
    let pages = walk(&q(&path), &paths);
    assert!(pages.len() > 10, "{} pages", pages.len());
    let mut joined = String::new();
    for (_, page) in &pages {
        let pagination = page.pagination.as_ref().expect("pagination");
        assert_eq!(pagination.unit, ReadProgressUnit::Bytes, "{pagination:?}");
        assert_eq!(pagination.offset, joined.len(), "pages are contiguous");
        assert_eq!(pagination.total_bytes, view.len(), "{pagination:?}");
        let content = page.content.as_deref().expect("content");
        assert_eq!(pagination.length, content.len(), "{pagination:?}");
        joined.push_str(content);
    }
    assert_eq!(joined, view);
    assert!(!joined.contains(TOKEN));
}

/// A window sized from the page budget fits it as JSON: escapes and
/// multi-byte characters count, so a default page never needs `rowPart`.
#[test]
fn a_byte_page_fits_its_room_as_serialized_text() {
    let fixture = Fixture::new();
    let source = minified("fits", &[]);
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let room = octocode_tools_shared::source_view::pagination::LINE_PAGE_BYTES;
    for (_, page) in walk(&q(&path), &paths) {
        let content = page.content.as_deref().expect("content");
        assert!(
            octocode_runtime_base::response::cut::escaped_len(content) <= room,
            "{} units",
            octocode_runtime_base::response::cut::escaped_len(content)
        );
    }
}

/// Only a page that shows a placeholder warns that it is redacted: the
/// pages of the same line before and after the secret are verbatim.
#[test]
fn only_the_page_showing_a_redaction_warns() {
    let fixture = Fixture::new();
    let source = minified("warns", &[60_000]);
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let pages = walk(&q(&path), &paths);
    let warned = |page: &LocalFetchResult| {
        page.warnings
            .iter()
            .any(|warning| warning == octocode_runtime_base::security::REDACTED_MARK)
    };
    for (index, (_, page)) in pages.iter().enumerate() {
        let shows = page
            .content
            .as_deref()
            .is_some_and(|text| text.contains(octocode_engine::security::REDACTION_PREFIX));
        assert_eq!(warned(page), shows, "page {index}: {:?}", page.warnings);
    }
    assert!(
        pages.iter().any(|(_, page)| warned(page)),
        "one page shows it"
    );
}

/// A byte offset no window emits (inside a character, a CRLF pair or a
/// redaction marker of the view) is refused with a restart, never snapped.
#[test]
fn a_byte_cursor_off_a_window_edge_is_refused() {
    let fixture = Fixture::new();
    let source = minified("refused", &[20_000]);
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let view = redacted_view(&source, &path);
    let marker = view
        .find(octocode_engine::security::REDACTION_PREFIX)
        .expect("a marker");
    let char_inside = view.find('é').expect("a two-byte character") + 1;
    for offset in [char_inside, marker + 3] {
        let read = real(
            &qj(
                &path,
                serde_json::json!({"readProgress":{"unit":"bytes","offset":offset}}),
            ),
            &paths,
        );
        assert_eq!(
            read.error_code,
            Some(ErrorCode::InvalidPagination),
            "{offset}: {read:?}"
        );
        let restart = read
            .next
            .as_ref()
            .and_then(|next| next.restart.as_ref())
            .expect("a restart");
        assert!(
            local_fetch_query::read_progress(&restart.query).is_none(),
            "{restart:?}"
        );
    }
    let crlf = fixture.write("crlf.min.js", format!("{}\r\nend\r\n", "x".repeat(40_000)));
    let read = real(
        &qj(
            &crlf,
            json_fixture(r#"{"readProgress": {"unit": "bytes", "offset": 40001}}"#),
        ),
        &paths,
    );
    assert_eq!(
        read.error_code,
        Some(ErrorCode::InvalidPagination),
        "{read:?}"
    );
}

/// A UTF-8 byte-order mark the decode drops is disclosed: byte offsets and
/// totals count the text without it.
#[test]
fn a_stripped_utf8_bom_is_disclosed() {
    let fixture = Fixture::new();
    let path = fixture.write("bom.txt", "\u{feff}first\nsecond\n");
    let paths = Paths(fixture.0.clone());
    let read = real(&q(&path), &paths);
    assert_eq!(read.content.as_deref(), Some("first\nsecond\n"));
    assert!(
        read.warnings
            .iter()
            .any(|warning| warning.contains("byte-order mark")),
        "{:?}",
        read.warnings
    );
    let plain = fixture.write("plain.txt", "first\nsecond\n");
    assert!(
        real(&q(&plain), &paths)
            .warnings
            .iter()
            .all(|warning| !warning.contains("byte-order mark")),
        "no BOM, no note"
    );
}

/// The compact view is cut from the redacted source: it shows each secret
/// as the content view does, as a placeholder, never its value.
#[test]
fn compact_and_content_redact_the_same_secrets() {
    let fixture = Fixture::new();
    let source = format!(
        "// header\nconst a = 1;\n\n\nconst token = \"{TOKEN}\";\n  // trailing\nconst b = 2;\n"
    );
    let path = fixture.write("parity.js", &source);
    let paths = Paths(fixture.0.clone());
    let read = |view: &str| {
        let page = real(&qj(&path, serde_json::json!({ "view": view })), &paths);
        page.content.expect("content")
    };
    let (content, compact) = (read("content"), read("compact"));
    for text in [&content, &compact] {
        assert!(!text.contains(TOKEN), "{text}");
    }
    assert_eq!(
        octocode_runtime_base::security::placeholders(&content),
        octocode_runtime_base::security::placeholders(&compact),
        "{content}\n{compact}"
    );
}

/// A match read states how many masked spans its source holds, the same for
/// any match string; a read without one, or of a source with none, states
/// none. The row keeps the published output contract.
#[test]
fn a_match_read_states_the_masked_spans_whatever_it_matched() {
    let fixture = Fixture::new();
    let source = format!("fn one() {{}}\nlet a = \"{TOKEN}\";\nlet b = \"{TOKEN}\";\n");
    let path = fixture.write("held.rs", &source);
    let clean = fixture.write("clean.rs", "fn one() {}\n");
    let paths = Paths(fixture.0.clone());
    let spans = |path: &Path, fields: serde_json::Value| {
        let row = real(&qj(path, fields), &paths);
        assert_eq!(row.error_code, None, "{row:?}");
        let wire = serde_json::to_value(&row).expect("row");
        octocode_runtime_base::test_support::assert_row_contract(
            octocode_runtime_base::id::ToolId::LocalFetch,
            &wire,
        );
        wire.get("maskedSpans").cloned()
    };
    for pattern in ["fn one", "ghp_abcdef", "zz-not-anywhere"] {
        assert_eq!(
            spans(&path, serde_json::json!({"matchString": [pattern]})),
            Some(serde_json::json!(2)),
            "{pattern}"
        );
    }
    assert_eq!(spans(&path, serde_json::json!({})), None);
    assert_eq!(
        spans(&clean, serde_json::json!({"matchString": ["one"]})),
        None
    );
}
