//! A later match page reads only its windows, is the page a fresh scan gives,
//! and never outlives the bytes it was scanned from.
use super::*;
use octocode_test_support::json_fixture;

const LINES: usize = 40_000;
const EVERY: usize = 400;

/// Numbered lines with a hit every [`EVERY`] and a private-key block whose
/// lines hit too, distinct per `seed`: the stores are shared by tests running
/// at once.
fn source(seed: char) -> Vec<u8> {
    let mut text = String::new();
    for index in 0..LINES {
        match index {
            1_000 => text.push_str("-----BEGIN RSA PRIVATE KEY-----\n"),
            1_001..=1_003 => text.push_str("needleSECRETBODY\n"),
            1_004 => text.push_str("-----END RSA PRIVATE KEY-----\n"),
            _ if index % EVERY == 0 => text.push_str(&format!("{seed} {index:070} needle\n")),
            _ => text.push_str(&format!("{seed} {index:077}\n")),
        }
    }
    text.into_bytes()
}

fn matching(pattern: &str) -> LocalFetchQuery {
    query(
        serde_json::json!({"path":"f","matchString":[pattern],"contextLines":{"before":1,"after":1}}),
    )
}

/// Every page of `q`'s read, following `nextPage`.
fn walk(
    file: &tempfile::NamedTempFile,
    q: &LocalFetchQuery,
    share: usize,
) -> Vec<serde_json::Value> {
    let (mut pages, mut call) = (Vec::new(), Some(q.clone()));
    while let Some(query) = call {
        let page = fetch(file, &query, Some(share));
        call = next_page(&page);
        pages.push(serde_json::to_value(&page).expect("page"));
        assert!(pages.len() < 5_000, "the walk ends");
    }
    pages
}

fn next_page(page: &LocalFetchResult) -> Option<LocalFetchQuery> {
    let next = page.next.as_ref()?;
    next.continue_window.as_ref().map(|call| call.query.clone())
}

/// The call that continues the matches past the page.
fn next_matches(page: &LocalFetchResult) -> LocalFetchQuery {
    next_page(page).expect("a later match page")
}

/// Pages from the kept scan are byte-identical to the pages fresh scans give,
/// at either page size, and the key block stays replaced on all of them.
#[test]
fn kept_pages_are_the_pages_a_fresh_scan_gives() {
    for (seed, share) in [('g', 1_500), ('h', 9_000)] {
        let settled = temp(&source(seed));
        octocode_engine::stamp::settle_written_so_far();
        let unsettled = temp(&source(seed));
        let q = matching("needle");
        let kept = walk(&settled, &q, share);
        let fresh = walk(&unsettled, &q, share);
        assert!(kept.len() > 3, "{} pages", kept.len());
        assert_eq!(kept.len(), fresh.len(), "share {share}");
        for (index, (kept, fresh)) in kept.iter().zip(&fresh).enumerate() {
            assert_eq!(kept, fresh, "share {share}, page {index}");
        }
        let text = serde_json::to_string(&fresh).expect("pages");
        assert!(!text.contains("SECRETBODY"), "share {share}");
    }
}

/// A later match page reads about its windows, not the source again.
#[test]
fn a_later_match_page_reads_only_its_windows() {
    let bytes = source('i');
    let file = temp(&bytes);
    octocode_engine::stamp::settle_written_so_far();
    let first = fetch(&file, &matching("needle"), Some(1_500));
    let next = next_matches(&first);
    let before = source_bytes_read();
    let page = fetch(&file, &next, Some(1_500));
    let read = source_bytes_read() - before;
    assert_eq!(page.error_code, None, "{page:?}");
    assert!(read < bytes.len() / 2, "read {read} of {}", bytes.len());
}

/// A plain window after a match page reuses the match scan's pass.
#[test]
fn a_plain_page_after_a_match_page_reads_only_its_window() {
    let bytes = source('j');
    let file = temp(&bytes);
    octocode_engine::stamp::settle_written_so_far();
    let first = fetch(&file, &matching("needle"), Some(1_500));
    let digest = first.source_sha256.clone().expect("digest");
    let plain = query(serde_json::json!({
        "path":"f","snapshot":digest,"ranges":[{"startLine":100,"endLine":110}]
    }));
    let before = source_bytes_read();
    let page = fetch(&file, &plain, Some(1_500));
    let read = source_bytes_read() - before;
    assert_eq!(page.error_code, None, "{page:?}");
    assert!(read < bytes.len() / 2, "read {read} of {}", bytes.len());
}

/// A changed file restarts the read, also when it holds no hit past the page.
#[test]
fn a_changed_file_is_stale_even_with_no_hit_left() {
    for (seed, changed) in [('k', source('x')), ('l', b"nothing to find\n".to_vec())] {
        let file = temp(&source(seed));
        octocode_engine::stamp::settle_written_so_far();
        let next = next_matches(&fetch(&file, &matching("needle"), Some(1_500)));
        std::fs::write(file.path(), &changed).expect("rewrite");
        let page = fetch(&file, &next, Some(1_500));
        assert_eq!(page.error_code, Some(ErrorCode::StaleSnapshot), "{page:?}");
    }
}

/// A write that keeps the size and restores the modification time is seen.
#[cfg(unix)]
#[test]
fn a_write_restoring_its_time_between_match_pages_is_stale() {
    let file = temp(&source('m'));
    octocode_engine::stamp::settle_written_so_far();
    let next = next_matches(&fetch(&file, &matching("needle"), Some(1_500)));
    let meta = std::fs::metadata(file.path()).expect("meta");
    let mut writer = std::fs::OpenOptions::new()
        .write(true)
        .open(file.path())
        .expect("reopen");
    std::io::Write::write_all(&mut writer, b"Z").expect("write");
    writer
        .set_modified(meta.modified().expect("modified"))
        .expect("restore time");
    let page = fetch(&file, &next, Some(1_500));
    assert_eq!(page.error_code, Some(ErrorCode::StaleSnapshot), "{page:?}");
}

/// Everything that defines a match keeps its own hits: a later page of another
/// match read of the same bytes is that read's own page.
#[test]
fn another_match_read_of_the_same_bytes_gets_its_own_hits() {
    let settled = temp(&source('n'));
    octocode_engine::stamp::settle_written_so_far();
    let unsettled = temp(&source('n'));
    let needle = next_matches(&fetch(&settled, &matching("needle"), Some(1_500)));
    let variants = [
        json_fixture(r#"{"matchString": ["NEEDLE"], "caseMode": "sensitive"}"#),
        json_fixture(r#"{"matchString": ["NEEDLE"], "caseMode": "insensitive"}"#),
        json_fixture(r#"{"matchString": ["needl"], "wholeWord": true}"#),
        json_fixture(r#"{"matchString": ["needl"]}"#),
        json_fixture(r#"{"matchString": ["need[l]e$"], "regexEngine": "rust"}"#),
        json_fixture(r#"{"contextLines": {"before": 0, "after": 3}}"#),
        json_fixture(r#"{"ranges": [{"startLine": 500, "endLine": 900}]}"#),
    ];
    for variant in variants {
        let other = local_fetch_query::updated(&needle, variant.clone()).expect("variant");
        let kept = serde_json::to_value(fetch(&settled, &other, Some(1_500))).expect("page");
        let fresh = serde_json::to_value(fetch(&unsettled, &other, Some(1_500))).expect("page");
        assert_eq!(kept, fresh, "{variant}");
    }
}

/// A regex limit stops the scan's reading of text, so a NUL past it refuses
/// no match page; a plain read of those bytes must still be refused, so that
/// pass is not kept for one.
#[test]
fn a_nul_past_a_regex_limit_keeps_plain_reads_refused() {
    let budget = crate::page_memo::test_budget!(16 * 1024);
    let _scope = crate::source_view::extraction::SpanBudgetOverride::of(budget);
    let hits = budget.limit() / std::mem::size_of::<(usize, usize)>() + 5;
    let mut bytes = "hit\n".repeat(hits).into_bytes();
    bytes.extend(std::iter::repeat_n(b'.', 4 * 1024 * 1024));
    bytes.extend_from_slice(b"\n\0\n");
    let file = temp(&bytes);
    octocode_engine::stamp::settle_written_so_far();
    let q = query(json_fixture(
        r#"{
            "path": "f",
            "matchString": ["hit"],
            "regexEngine": "rust",
            "contextLines": {"before": 0, "after": 0}
        }"#,
    ));
    let first = fetch(&file, &q, Some(1_500));
    assert!(
        first.warnings.iter().any(|w| w.contains("match limit")),
        "{:?}",
        first.warnings
    );
    let plain = query(serde_json::json!({
        "path":"f","snapshot":first.source_sha256.clone().expect("digest"),
        "ranges":[{"startLine":1,"endLine":2}]
    }));
    let page = fetch(&file, &plain, Some(1_500));
    assert!(page.error.is_some(), "{page:?}");
}

/// A streamed list read names each entry that matched nothing on its first
/// page, fresh or replayed from the kept scan, and only there.
#[test]
fn a_streamed_list_read_names_its_unmatched_entries_on_its_first_page() {
    let file = temp(&source('k'));
    octocode_engine::stamp::settle_written_so_far();
    let q = query(json_fixture(
        r#"{
            "path": "f",
            "matchString": ["needle", "absent term"],
            "contextLines": {"before": 1, "after": 1}
        }"#,
    ));
    let named = |page: &LocalFetchResult| {
        page.warnings
            .iter()
            .filter(|warning| warning.contains("matchString entries matched nothing"))
            .cloned()
            .collect::<Vec<_>>()
    };
    for _ in ["fresh", "kept"] {
        let first = fetch(&file, &q, Some(1_500));
        let warned = named(&first);
        assert_eq!(warned.len(), 1, "{:?}", first.warnings);
        assert!(warned[0].contains("\"absent term\""), "{warned:?}");
        assert!(!warned[0].contains("\"needle\""), "{warned:?}");
        let later = fetch(&file, &next_matches(&first), Some(1_500));
        assert!(named(&later).is_empty(), "{:?}", later.warnings);
    }
}
