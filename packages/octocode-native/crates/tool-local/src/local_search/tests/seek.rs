//! A stored scan is read by page: a continuation decodes the rows it
//! shows and inflates its own page's frames, never the whole scan, so its
//! cost stays flat as the scan grows.
use super::*;
use octocode_test_support::json_fixture;

/// Hits decoded and raw bytes inflated on this thread so far.
fn read_so_far() -> (usize, usize) {
    (
        super::super::packed::decoded_hits_on_this_thread(),
        super::super::packed::inflated_on_this_thread(),
    )
}

/// Rows a page shows.
fn shown_rows(page: &LocalSearchResult) -> usize {
    page.files
        .iter()
        .map(|file| file.matches.as_ref().map_or(0, Vec::len))
        .sum()
}

/// Page 1 of `fields` under `root`, then pages 2, 3 and the middle page of
/// its snapshot: for each continuation, the rows it decoded, the rows it
/// showed and the bytes it inflated.
fn continuations(
    root: &std::path::Path,
    fields: serde_json::Value,
) -> Vec<(u32, usize, usize, usize)> {
    let (policy, security) = policy_for(root);
    let mut fields = fields;
    fields["path"] = root.to_string_lossy().into_owned().into();
    let q = ls_query(fields, None);
    let first = search_in(&q, &policy, &security, Some(2500)).expect("page 1");
    // The first page holds `length` of `total` rows: a page in the middle
    // of the walk lies about `total / length / 2` pages in.
    let paging = serde_json::to_value(first.pagination.as_ref().expect("paged")).expect("facts");
    let (length, total) = (
        paging["length"].as_u64().expect("length"),
        paging["total"].as_u64().expect("total"),
    );
    let middle = u32::try_from(total / length.max(1) / 2).expect("page");
    assert!(middle > 3, "{paging}");
    let snapshot = first.source_snapshot.expect("snapshot");
    [2, 3, middle]
        .into_iter()
        .map(|page| {
            let continued = ls_query(
                serde_json::json!({"snapshot":snapshot,"page":page}),
                Some(&q),
            );
            let (hits, inflated) = read_so_far();
            let result = search_in(&continued, &policy, &security, Some(2500))
                .unwrap_or_else(|error| panic!("page {page}: {error:?}"));
            let (hits_after, inflated_after) = read_so_far();
            (
                page,
                hits_after - hits,
                shown_rows(&result),
                inflated_after - inflated,
            )
        })
        .collect()
}

#[test]
fn a_continuation_inflates_its_page_not_the_scan() {
    let mut third_pages = Vec::new();
    for spans in [20_000usize, 200_000] {
        let root = tempfile::tempdir().expect("fixture");
        fs::write(root.path().join("one.txt"), "a".repeat(spans) + "\n").expect("line");
        settle();
        let pages = continuations(
            root.path(),
            json_fixture(
                r#"{
                    "matchString": ["a"],
                    "view": "matchOnly",
                    "sort": {"by": "path", "order": "asc"}
                }"#,
            ),
        );
        for &(page, decoded, shown, _) in &pages {
            assert!(shown > 0, "{spans} spans, page {page} shows rows");
            assert_eq!(
                decoded, shown,
                "{spans} spans, page {page}: decoded {decoded} rows to show {shown}"
            );
        }
        third_pages.push(pages[1].3);
    }
    // Wider column numbers may take a byte more each; nothing else grows.
    assert!(
        third_pages[1] * 4 <= third_pages[0] * 5,
        "page 3 inflated {} bytes of a 200k-span scan, {} of a 20k one",
        third_pages[1],
        third_pages[0]
    );

    let root = tempfile::tempdir().expect("fixture");
    for file in 0..2_000 {
        fs::write(
            root.path().join(format!("f{file:04}.ts")),
            format!("export const needle_{file} = {file};\n"),
        )
        .expect("file");
    }
    settle();
    for (page, decoded, shown, _) in continuations(
        root.path(),
        json_fixture(
            r#"{
                "matchString": ["needle"],
                "sort": {"by": "path", "order": "asc"}
            }"#,
        ),
    ) {
        assert!(shown > 0, "page {page} shows rows");
        assert_eq!(
            decoded, shown,
            "page {page}: decoded {decoded} to show {shown}"
        );
    }
}

/// The query a continuation call runs.
fn query_of(call: &serde_json::Value) -> LocalSearchQuery {
    serde_json::from_value(call["input"]["queries"][0].clone()).expect("continuation row")
}

/// Every call a page names to continue: its next page and each file's next
/// page.
fn continuations_of(page: &LocalSearchResult) -> Vec<LocalSearchQuery> {
    let row_next = page.next.iter().filter_map(|next| next.get("nextPage"));
    let file_next = page
        .files
        .iter()
        .filter_map(|file| file.next.as_ref()?.get("nextPage"));
    row_next.chain(file_next).map(query_of).collect()
}

/// Walk `fields` under `root` by following every continuation each page
/// names, each call once. Every continuation is checked against the oracle:
/// the same call after its stored scan is evicted, served from a fresh walk
/// through the same page view, must return the same row. Returns each
/// page's published row, in walk order. The walk is bounded by the data:
/// at most one call per row and file, plus the first.
fn walk_against_fresh(
    root: &std::path::Path,
    fields: serde_json::Value,
    window: Option<usize>,
) -> Vec<serde_json::Value> {
    walk_against_fresh_pages(root, fields, window, 1)
}

/// [`walk_against_fresh`] checking one page in `every` against its fresh
/// page: the first two, the last, and each `every`th between. Every page is
/// still served and walked, so the rows of the whole walk are the ones
/// checked for repeats and gaps.
fn walk_against_fresh_pages(
    root: &std::path::Path,
    fields: serde_json::Value,
    window: Option<usize>,
    every: usize,
) -> Vec<serde_json::Value> {
    let (policy, security) = policy_for(root);
    let mut fields = fields;
    fields["path"] = root.to_string_lossy().into_owned().into();
    let mut queue = std::collections::VecDeque::from([ls_query(fields, None)]);
    let mut called = std::collections::HashSet::new();
    let mut pages = Vec::new();
    let mut bound = None;
    while let Some(query) = queue.pop_front() {
        if !called.insert(serde_json::to_string(&query).expect("query")) {
            continue;
        }
        let page = search_in(&query, &policy, &security, window)
            .unwrap_or_else(|error| panic!("{query:?}: {error:?}"));
        let row = page.to_row().expect("row");
        let continued = continuations_of(&page);
        let checked = pages.len() < 2 || pages.len() % every == 0 || continued.is_empty();
        if let Some(snapshot) =
            super::super::local_search_query::snapshot(&query).filter(|_| checked)
        {
            super::super::manifest::evict(snapshot);
            let fresh = search_in(&query, &policy, &security, window)
                .unwrap_or_else(|error| panic!("fresh {query:?}: {error:?}"));
            assert_eq!(
                row,
                fresh.to_row().expect("row"),
                "a stored page differs from its fresh page: {query:?}"
            );
        }
        let bound = *bound.get_or_insert_with(|| {
            let total = page
                .pagination
                .as_ref()
                .and_then(|paging| serde_json::to_value(paging).ok())
                .and_then(|paging| paging["total"].as_u64());
            total.map_or(1, |total| total as usize) + page.stats.files_matched as usize + 1
        });
        queue.extend(continued);
        pages.push(row);
        assert!(pages.len() <= bound, "the walk passed {bound} calls");
    }
    pages
}

/// Each shown row's `(path, line, column)`, every one once.
fn rows_once(pages: &[serde_json::Value]) -> std::collections::BTreeSet<(String, u64, u64)> {
    let mut seen = std::collections::BTreeSet::new();
    for page in pages {
        for file in page["files"].as_array().into_iter().flatten() {
            for row in file["matches"].as_array().into_iter().flatten() {
                let id = (
                    file["path"].as_str().expect("path").to_owned(),
                    row["line"].as_u64().expect("line"),
                    row["column"].as_u64().unwrap_or(0),
                );
                assert!(seen.insert(id.clone()), "{id:?} shown twice");
            }
        }
    }
    seen
}

/// Each numbered source line context rows show, every one once and equal
/// to its source line.
fn lines_once(pages: &[serde_json::Value], source: &[String]) -> std::collections::BTreeSet<usize> {
    let mut seen = std::collections::BTreeSet::new();
    for page in pages {
        for file in page["files"].as_array().into_iter().flatten() {
            for row in file["matches"].as_array().into_iter().flatten() {
                for line in row["value"].as_str().expect("value").lines() {
                    let (number, text) = line.split_once('\t').expect("numbered line");
                    let number = number.parse::<usize>().expect("line number");
                    assert_eq!(text, source[number - 1]);
                    assert!(seen.insert(number), "line {number} shown twice");
                }
            }
        }
    }
    seen
}

/// One page in `every` is checked against its fresh page.
fn assert_seek_pages_equal_fresh_pages(every: usize) {
    // One long line of spans, ASCII and multi-byte, at a page of a row or
    // two and at a fuller one.
    for (line, pattern, window) in [
        ("a".repeat(3_000), "a", Some(2_500)),
        ("λ🦀".repeat(800), "🦀", Some(2_500)),
        ("xλ🦀y".repeat(300), "λ🦀", Some(150)),
    ] {
        let root = tempfile::tempdir().expect("fixture");
        fs::write(root.path().join("one.txt"), format!("{line}\n")).expect("line");
        settle();
        let pages = walk_against_fresh_pages(
            root.path(),
            serde_json::json!({"matchString":[pattern],"view":"matchOnly","sort":{"by":"path","order":"asc"}}),
            window,
            every,
        );
        assert!(pages.len() > 2, "{pattern}: {} pages", pages.len());
        assert_eq!(
            rows_once(&pages).len(),
            line.matches(pattern).count(),
            "{pattern}: every span once"
        );
    }

    // Long content lines, and many small files walked in reverse path order.
    let root = tempfile::tempdir().expect("fixture");
    let long = (0..120)
        .map(|n| format!("let needle_{n} = \"{}\";", "λ".repeat(200)))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.path().join("long.rs"), long + "\n").expect("long");
    for file in 0..300 {
        fs::write(
            root.path().join(format!("f{file:03}.ts")),
            format!("export const needle_{file} = {file};\n// needle again {file}\n"),
        )
        .expect("small");
    }
    settle();
    for fields in [
        json_fixture(
            r#"{
                "matchString": ["needle"],
                "sort": {"by": "path", "order": "asc"}
            }"#,
        ),
        json_fixture(
            r#"{
                "matchString": ["needle"],
                "sort": {"by": "path", "order": "desc"}
            }"#,
        ),
    ] {
        let pages = walk_against_fresh_pages(root.path(), fields.clone(), Some(2_500), every);
        assert!(pages.len() > 2, "{fields}");
        assert_eq!(rows_once(&pages).len(), 120 + 300 * 2, "{fields}");
    }
}

#[test]
fn every_seek_page_equals_its_fresh_page_and_the_walk_joins_once() {
    assert_seek_pages_equal_fresh_pages(6);
}

#[test]
#[ignore = "every page against its fresh page: cargo test -- --ignored every_page"]
fn every_seek_page_equals_its_fresh_page_for_every_page() {
    assert_seek_pages_equal_fresh_pages(1);
}

/// Thousands of hits in a few files, walked by small pages through the
/// cursors the pages emit alone: every hit is shown once, in walk order,
/// across the file boundaries, with no gap.
#[test]
fn a_cursor_walk_over_thousands_of_hits_in_few_files_shows_each_once_in_order() {
    let root = tempfile::tempdir().expect("fixture");
    let files = ["a.txt", "b.txt", "c.txt"];
    let lines = 1_500;
    for name in files {
        let text = (1..=lines)
            .map(|line| format!("needle {line}\n"))
            .collect::<String>();
        fs::write(root.path().join(name), text).expect("source");
    }
    settle();
    for window in [1_500, 4_000] {
        let (policy, security) = policy_for(root.path());
        let mut query = ls_query(
            serde_json::json!({
                "path": root.path().to_string_lossy(),
                "matchString": ["needle"],
                "sort": {"by": "traversal", "order": "asc"},
            }),
            None,
        );
        let mut pages = Vec::new();
        loop {
            let page = search_in(&query, &policy, &security, Some(window))
                .unwrap_or_else(|error| panic!("{query:?}: {error:?}"));
            pages.push(page.to_row().expect("row"));
            // Only the cursor a page emits goes into the next call.
            let mut next = continuations_of(&page);
            assert!(next.len() <= 1, "one cursor per page: {}", next.len());
            let Some(cursor) = next.pop() else { break };
            query = cursor;
            assert!(
                pages.len() < 2_000,
                "window {window}: the walk does not end"
            );
        }
        assert!(pages.len() > 10, "window {window}: {} pages", pages.len());
        let shown = pages
            .iter()
            .flat_map(|page| page["files"].as_array().into_iter().flatten())
            .flat_map(|file| {
                let path = file["path"].as_str().expect("path").to_owned();
                file["matches"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(move |row| (path.clone(), row["line"].as_u64().expect("line")))
            })
            .collect::<Vec<_>>();
        let wanted = files
            .iter()
            .flat_map(|name| (1..=lines as u64).map(move |line| ((*name).to_owned(), line)))
            .collect::<Vec<_>>();
        assert_eq!(shown.len(), wanted.len(), "window {window}");
        assert_eq!(shown, wanted, "window {window}: order, gaps or repeats");
    }
}

/// Context windows split between pages exactly as a fresh page splits
/// them: ranked rows put declarations first, so a page's neighbours sit on
/// pages before and after it.
#[test]
fn context_pages_from_the_stored_scan_show_every_line_once() {
    let source: Vec<String> = (1..=300)
        .map(|line| match line % 11 {
            0 => format!("fn needle_{line}() {{}}"),
            3 | 5 | 7 => format!("let needle_{line} = {line};"),
            _ => format!("let other_{line} = {line};"),
        })
        .collect();
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), source.join("\n") + "\n").expect("source");
    settle();
    for (before, after) in [(3, 3), (0, 4), (4, 0)] {
        let pages = walk_against_fresh(
            root.path(),
            serde_json::json!({"matchString":["needle"],"contextLines":{"before":before,"after":after},"sort":{"by":"path","order":"asc"}}),
            Some(2_500),
        );
        assert!(pages.len() > 2, "({before},{after})");
        let expected = source
            .iter()
            .enumerate()
            .filter(|(_, text)| text.contains("needle"))
            .flat_map(|(index, _)| {
                (index + 1).saturating_sub(before).max(1)..=(index + 1 + after).min(source.len())
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(lines_once(&pages, &source), expected, "({before},{after})");
    }
}

/// Values redaction changed on later pages stay redacted and counted as
/// on a fresh page: the frames hold what the first page redacted.
#[test]
fn redacted_rows_on_later_pages_match_their_fresh_pages() {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..20 {
        let body = (0..12)
            .map(|n| format!("export const needle_{file}_{n} = {n};\n"))
            .collect::<String>();
        fs::write(root.path().join(format!("a{file:02}.ts")), body).expect("file");
    }
    fs::write(
        root.path().join("z_key.ts"),
        "export const pem = `\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAneedleKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\nMIIEneedleSecondKeyLineBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\n-----END RSA PRIVATE KEY-----\n`;\nconst needle_token = \"ghp_00ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567\";\n",
    )
    .expect("key file");
    settle();
    let pages = walk_against_fresh(
        root.path(),
        json_fixture(
            r#"{
                "matchString": ["needle"],
                "sort": {"by": "path", "order": "asc"}
            }"#,
        ),
        Some(2_500),
    );
    let on = pages
        .iter()
        .enumerate()
        .flat_map(|(at, page)| {
            page["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|file| {
                    file["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with("z_key.ts"))
                })
                .map(move |file| (at, file))
        })
        .collect::<Vec<_>>();
    assert!(
        on.iter().all(|(at, _)| *at > 0),
        "the key file is on later pages"
    );
    let shown =
        serde_json::to_string(&on.iter().map(|(_, file)| file).collect::<Vec<_>>()).expect("file");
    assert!(shown.contains("needle"), "{shown}");
    assert!(
        !shown.contains("KeyBody") && !shown.contains("SecondKeyLine"),
        "{shown}"
    );
    assert!(!shown.contains("ghp_00ABCDEF"), "{shown}");
    // The key's body lines are text a read withholds: no hit. The token's
    // line is one, redacted.
    assert_eq!(rows_once(&pages).len(), 20 * 12 + 1);
}

/// A list view keeps its scan whole: its continuation is cut again from it,
/// and equals the fresh page.
#[test]
fn list_pages_from_a_whole_scan_match_their_fresh_pages() {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..200 {
        fs::write(
            root.path().join(format!("f{file:03}.ts")),
            format!("export const needle_{file} = {file};\n"),
        )
        .expect("file");
    }
    settle();
    let pages = walk_against_fresh(
        root.path(),
        json_fixture(
            r#"{
                "matchString": ["needle"],
                "view": "files",
                "sort": {"by": "path", "order": "asc"}
            }"#,
        ),
        Some(1_000),
    );
    assert!(pages.len() > 2);
    let listed = pages
        .iter()
        .flat_map(|page| page["files"].as_array().into_iter().flatten())
        .map(|file| file["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    let distinct = listed.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!((listed.len(), distinct.len()), (200, 200));
}

/// A seekable scan is kept only when every key-block read its first page
/// made read the bytes it stores: a file whose hit looks like key material
/// (a body line no armor encloses, which a read shows) and whose key-block
/// read failed (here past the key-scan limit) leaves the scan unstored, and
/// its continuation walks again, so no page serves redaction flags another
/// read decided.
#[test]
fn a_scan_whose_key_read_failed_is_not_stored_by_page() {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..20 {
        let body = (0..12)
            .map(|n| format!("export const needle_{file}_{n} = {n};\n"))
            .collect::<String>();
        fs::write(root.path().join(format!("a{file:02}.ts")), body).expect("file");
    }
    fs::write(
        root.path().join("z_key.ts"),
        "export const pem = `\nMIIEpQIBAAKCAQEAneedleKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n`;\n",
    )
    .expect("key file");
    settle();
    let (policy, _) = policy_for(root.path());
    let security = ContentSecurity::with_limits(octocode_runtime_base::security::SourceLimits {
        key_scan: 64,
        ..octocode_runtime_base::security::SourceLimits::REAL
    });
    let q = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"],"sort":{"by":"path","order":"asc"}}),
        None,
    );
    let first = search_in(&q, &policy, &security, Some(2_500)).expect("page 1");
    let snapshot = first.source_snapshot.expect("snapshot");
    assert!(
        first.pagination.is_some_and(|paging| paging.has_more),
        "page 2 is a page of the walk"
    );
    assert!(
        super::super::manifest::get(&snapshot, &policy.identity()).is_none(),
        "a scan whose key read failed is stored"
    );
    let continued = ls_query(serde_json::json!({"snapshot":snapshot,"page":2}), Some(&q));
    let page = search_in(&continued, &policy, &security, Some(2_500)).expect("page 2 walks again");
    assert!(!page.files.is_empty());
}
