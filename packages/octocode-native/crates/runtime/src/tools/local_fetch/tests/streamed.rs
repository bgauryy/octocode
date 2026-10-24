//! Sources past the whole-file ceiling through the canonical input and
//! output contracts: byte pages, multi-range reads, redaction.

use super::*;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};

fn checked_streamed_page(query: &LocalFetchQuery, paths: &Paths) -> LocalFetchResult {
    let mut rows =
        crate::contracts::prepare_many_built("localFetch", serde_json::json!({"queries":[query]}))
            .expect("canonical input contract");
    let query_json = rows.remove(0);
    let admitted = serde_json::from_value(query_json.clone()).expect("admitted query");
    let result = fetch(&admitted, paths);
    crate::tools::test_support::published(
        crate::id::ToolId::LocalFetch,
        &query_json,
        &serde_json::to_value(&result).expect("output"),
    );
    result
}

#[test]
fn streamed_oversized_line_pages_its_bytes_and_later_selected_lines() {
    let fixture = Fixture::new();
    let path = fixture.0.join("large.log");
    let longest = "x".repeat(4 * 1024 * 1024);
    let suffix = "suffix past the streamed window\n";
    let selected_tail = "selected tail\n";
    // Streamed sources are those over the in-memory ceiling: the lines
    // after the oversized one only take the file just past it.
    let outside = (super::super::acquire::MAX_SOURCE_BYTES as usize).saturating_sub(longest.len())
        / "outside\n".len()
        + 1_000;
    fixture.write(
        "large.log",
        format!(
            "{longest}{suffix}{selected_tail}{}",
            "outside\n".repeat(outside)
        ),
    );
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({"ranges":[{"startLine":1,"endLine":2}]}),
    );
    let view = format!("{longest}{suffix}{selected_tail}");
    let mut collected = String::new();
    let mut pages = 0usize;
    loop {
        let result = checked_streamed_page(&query, &paths);
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        let content = result.content.as_deref().expect("page content");
        let page = result.pagination.as_ref().expect("pagination");
        if page.unit == ReadProgressUnit::Bytes {
            assert_eq!(page.offset, collected.len(), "pages are contiguous");
            assert_eq!(page.total_bytes, view.len(), "the range's own size");
        }
        assert!(result.terminal_limit.is_none(), "{result:?}");
        collected.push_str(content);
        pages += 1;
        assert!(pages < 300, "paging must terminate");
        let Some(next) = result.next.and_then(|next| next.continue_window) else {
            break;
        };
        let wire = serde_json::to_value(next).expect("executable continuation");
        query = serde_json::from_value(wire["input"]["queries"][0].clone()).expect("copied query");
        assert!(
            local_fetch_query::snapshot(&query).is_some(),
            "source identity persists"
        );
        if collected.len() < longest.len() {
            assert_eq!(
                Some(local_fetch_query::unit(&query)),
                Some(ReadProgressUnit::Bytes),
                "byte cursor stays in bytes"
            );
        }
    }
    assert!(pages > 1);
    assert_eq!(collected, view);
    assert!(!collected.contains("outside"));
}

/// A streamed multi-range page stops inside the first span that needs
/// several window pages: it serves every span before it and that span's
/// first page, then continues that span alone (`continueWindow`) and reads
/// the ranges after it once (`readOtherRanges`, L2-F15). Following them
/// reaches every requested line exactly once.
#[test]
fn a_streamed_multi_range_read_serves_what_fits_and_continues_the_rest() {
    let fixture = Fixture::new();
    let source = (1..=1_500_000)
        .map(|n| format!("line {n}\n"))
        .collect::<String>();
    let path = fixture.write("large.log", &source);
    let paths = Paths(fixture.0.clone());
    let first = checked_streamed_page(
        &qj(
            &path,
            serde_json::json!({"ranges":[{"startLine":100,"endLine":150},{"startLine":150000,"endLine":157500},{"startLine":300000,"endLine":300010}]}),
        ),
        &paths,
    );
    assert_eq!(first.status, ReadStatus::Success, "{first:?}");
    assert_eq!(first.is_partial, Some(true), "{first:?}");
    let mut seen = std::collections::BTreeMap::<usize, usize>::new();
    let mut count = |content: &str| {
        for line in content.lines() {
            if let Some(n) = line.strip_prefix("line ").and_then(|n| n.parse().ok()) {
                *seen.entry(n).or_default() += 1;
            }
        }
    };
    let content = first.content.as_deref().expect("content");
    assert!(content.contains("line 100\nline 101\n") && content.contains("line 150000\n"));
    count(content);
    let next = first.next.expect("continuations");
    assert!(next.read_bounded_lines.is_none(), "{next:?}");
    let others = next
        .read_other_ranges
        .expect("the ranges after the cut")
        .query;
    assert_eq!(
        serde_json::to_value(&others).expect("query")["ranges"],
        serde_json::json!([{"startLine":300000,"endLine":300010}])
    );
    let mut page = next.continue_window;
    let mut pages = 1;
    while let Some(call) = page {
        let wire = serde_json::to_value(call).expect("executable continuation");
        let query: LocalFetchQuery =
            serde_json::from_value(wire["input"]["queries"][0].clone()).expect("copied query");
        assert_eq!(
            serde_json::to_value(&query).expect("query")["ranges"],
            serde_json::json!([{"startLine":150000,"endLine":157500}])
        );
        let result = checked_streamed_page(&query, &paths);
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        count(result.content.as_deref().expect("page content"));
        page = result.next.and_then(|next| next.continue_window);
        pages += 1;
        assert!(pages < 100, "paging must terminate");
    }
    assert!(pages > 1);
    let rest = checked_streamed_page(&others, &paths);
    assert_eq!(rest.status, ReadStatus::Success, "{rest:?}");
    assert!(rest.next.is_none(), "{rest:?}");
    count(rest.content.as_deref().expect("content"));
    let requested = (100..=150)
        .chain(150_000..=157_500)
        .chain(300_000..=300_010);
    for n in requested {
        assert_eq!(seen.remove(&n), Some(1), "line {n}");
    }
    assert!(seen.is_empty(), "unrequested lines: {seen:?}");
}

#[test]
fn streamed_byte_pages_reject_source_changes_and_start_from_private_progress() {
    let fixture = Fixture::new();
    let line = "x".repeat(32 * 1024);
    let source = format!("{line}\nselected tail\n{}", "outside\n".repeat(1_400_000));
    let path = fixture.write("large.log", &source);
    let paths = Paths(fixture.0.clone());
    let first = checked_streamed_page(
        &qj(
            &path,
            serde_json::json!({"ranges":[{"startLine":1,"endLine":2}]}),
        ),
        &paths,
    );
    assert_eq!(first.terminal_limit, None);
    let next = first
        .next
        .expect("next")
        .continue_window
        .expect("byte page");
    assert_eq!(
        Some(local_fetch_query::unit(&next.query)),
        Some(ReadProgressUnit::Bytes)
    );
    assert_eq!(local_fetch_query::offset(&next.query), Some(16_384));
    let second = checked_streamed_page(&next.query, &paths);
    assert_eq!(second.status, ReadStatus::Success, "{:?}", second.error);
    assert_eq!(second.content.as_deref(), Some(&line[16_384..]));
    fixture.write("large.log", source.replacen('x', "y", 1));
    let stale = checked_streamed_page(&next.query, &paths);
    assert_eq!(stale.error_code, Some(ErrorCode::StaleSnapshot));
    let unbounded = checked_streamed_page(
        &qj(
            &path,
            serde_json::json!({"readProgress":{"unit":"bytes","offset":0}}),
        ),
        &paths,
    );
    assert_eq!(unbounded.error_code, None, "{unbounded:?}");
    assert!(
        unbounded
            .content
            .as_deref()
            .is_some_and(|text| text.starts_with('y'))
    );
    assert!(
        unbounded
            .next
            .and_then(|next| next.continue_window)
            .is_some()
    );
}

#[test]
fn a_streamed_clipped_private_key_body_stays_redacted_across_windows() {
    let fixture = Fixture::new();
    let path = fixture.write(
        "large.log",
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\nselected tail\n{}",
            "S".repeat(4 * 1024 * 1024 + 17),
            "outside\n".repeat(900_000)
        ),
    );
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({"ranges":[{"startLine":2,"endLine":4}]}),
    );
    let mut collected = String::new();
    for _ in 0..4 {
        let result = checked_streamed_page(&query, &paths);
        assert_eq!(result.status, ReadStatus::Success, "{:?}", result.error);
        assert_eq!(result.terminal_limit, None);
        collected.push_str(result.content.as_deref().expect("content"));
        if let Some(next) = result.next.and_then(|next| next.continue_window) {
            query = next.query;
        } else {
            assert!(collected.ends_with("selected tail\n"));
            assert!(!collected.contains("SSSS"));
            assert!(!collected.contains("BEGIN"));
            assert!(!collected.contains("outside"));
            assert!(collected.contains("[REDACTED"));
            return;
        }
    }
    panic!("redacted windows must terminate");
}
