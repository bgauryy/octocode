use super::*;

#[test]
fn every_view_delivers_each_matching_file_or_line_once_at_multiple_budgets() {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..400 {
        fs::write(
            root.path().join(format!("f{file:03}.txt")),
            "alpha needle\nbeta needle needle\nother\n",
        )
        .expect("file");
    }
    for view in [
        "content",
        "files",
        "filesWithout",
        "countLines",
        "countMatches",
        "matchOnly",
    ] {
        for budget in [2500, 9000] {
            let term = if view == "filesWithout" {
                "missing"
            } else {
                "needle"
            };
            let (walk, pages) = walk_search(
                root.path(),
                serde_json::json!({
                    "path":root.path(),"matchString":[term],"view":view,"sort":{"by":"path","order":"asc"}
                }),
                Some(budget),
            );
            let count = if view == "matchOnly" {
                1200
            } else if view == "content" {
                800
            } else {
                400
            };
            assert_eq!(walk.rows.len(), count, "{view} budget {budget}");
            assert!(pages.len() > 1, "fixture must exercise continuation");
            for page in &pages {
                for file in page["files"].as_array().expect("files") {
                    if view == "countLines" {
                        assert_eq!(file["matchedLineCount"], 2, "{file}");
                    }
                    if view == "countMatches" {
                        assert_eq!(file["matchCount"], 3, "{file}");
                    }
                }
            }
        }
    }
}

/// One file's spans span many pages, with no count per file bounding them:
/// walking its matchOnly pages reaches every span exactly once, across
/// every page edge inside the file.
#[test]
fn a_match_only_walk_reaches_every_span_of_a_file_across_its_pages() {
    let root = tempfile::tempdir().expect("fixture");
    let spans = 2_000;
    fs::write(root.path().join("many.txt"), "n\n".repeat(spans)).expect("file");
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["n"],"view":"matchOnly"}),
        Some(20_000),
    );
    assert!(pages.len() > 2, "the file's spans must cross page edges");
    let mut lines = walk
        .rows
        .iter()
        .map(|row| {
            row.split(':')
                .nth(1)
                .expect("line")
                .parse::<usize>()
                .expect("number")
        })
        .collect::<Vec<_>>();
    lines.sort_unstable();
    assert_eq!(lines, (1..=spans).collect::<Vec<_>>(), "each span once");
}

/// A scan whose matches pass what it may hold is refused, never shown in
/// part, with executable calls that answer within the bound: the per-file
/// counts of the same search, and a listing of the scope to split it by.
#[test]
fn a_scan_past_its_held_bound_is_refused_with_leads_that_answer() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("many.txt"), "n\n".repeat(2_000)).expect("file");
    let (policy, security) = policy_for(root.path());
    let bounded = crate::tools::local_search::executor::test_held_limit(Some(16 * 1024));
    for view in ["content", "matchOnly"] {
        let q = ls_query(
            serde_json::json!({"path":root.path(),"matchString":["n"],"view":view}),
            None,
        );
        let error = search_in(&q, &policy, &security, None).expect_err("past the bound");
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::ScopeTooBroad,
            "{view}: {}",
            error.message
        );
        let calls = error.calls.expect("leads");
        let count = &calls["viewFileCount"];
        assert_eq!(
            count["input"]["queries"][0]["view"], "countMatches",
            "{calls}"
        );
        assert_eq!(calls["listScope"]["tool"], "structureSearch", "{calls}");
        assert!(calls.get("readFile").is_none(), "{calls}");
        let counted = ls_query(count["input"]["queries"][0].clone(), None);
        let counted = search_in(&counted, &policy, &security, None).expect("a count fits");
        assert_eq!(counted.stats.total_occurrences, Some(2_000), "{view}");
        let listing: octocode_contract_types::StructureSearchQuery =
            serde_json::from_value(calls["listScope"]["input"]["queries"][0].clone())
                .expect("a structureSearch row");
        let listed = crate::tools::structure_search::execute_structure(
            &listing,
            &policy,
            &security,
            &crate::execution::cancel::NeverCancel,
            None,
        )
        .expect("the listing runs");
        assert!(listed.to_string().contains("many.txt"), "{view}: {listed}");
    }
    drop(bounded);
}

/// A walk of spans on one line judges that line's window at most twice,
/// however many pages and spans it has: the fresh page, which has no stored
/// digest to bind a judgment to, and the first continuation, whose judgment
/// the stored scan keeps for the rest (see `JudgedWindows`). So a page's
/// scanning does not grow with the line or the scan. Every span is
/// delivered once.
#[test]
fn a_walk_of_spans_on_one_line_judges_the_line_at_most_twice() {
    for spans in [3_000usize, 12_000] {
        let root = tempfile::tempdir().expect("fixture");
        fs::write(
            root.path().join("one.txt"),
            format!("{}\n", "a".repeat(spans)),
        )
        .expect("file");
        settle();
        let before = crate::tools::local_search::verify::judged_on_this_thread();
        let (walk, pages) = walk_search(
            root.path(),
            serde_json::json!({"path":root.path(),"matchString":["a"],"view":"matchOnly"}),
            None,
        );
        let judged = crate::tools::local_search::verify::judged_on_this_thread() - before;
        assert!(pages.len() > 1, "{spans}: one page");
        assert_eq!(walk.rows.len(), spans, "{spans}: each span once");
        assert!(pages.len() > 2, "{spans}: {} pages", pages.len());
        assert_eq!(judged, 2 * spans, "{spans}: {} pages", pages.len());
    }
}

/// A continuation of a stored scan prices about the rows it shows, not
/// every row of the scan: its cut is the one the scan's first page made.
/// The bound holds alike for a small and a four times larger scan.
#[test]
fn a_continuation_prices_its_page_not_the_scan() {
    for spans in [3_000usize, 12_000] {
        let root = tempfile::tempdir().expect("fixture");
        fs::write(
            root.path().join("one.txt"),
            format!("{}\n", "a".repeat(spans)),
        )
        .expect("file");
        settle();
        let (policy, security) = policy_for(root.path());
        let q = ls_query(
            serde_json::json!({"path":root.path(),"matchString":["a"],"view":"matchOnly"}),
            None,
        );
        let first = search_in(&q, &policy, &security, None).expect("page 1");
        let snapshot = first.source_snapshot.expect("snapshot");
        for page in 2..=4u32 {
            let next = ls_query(
                serde_json::json!({"snapshot":snapshot,"page":page}),
                Some(&q),
            );
            let before = crate::tools::local_search::layout::priced_on_this_thread();
            let shown = search_in(&next, &policy, &security, None).expect("page");
            let priced = crate::tools::local_search::layout::priced_on_this_thread() - before;
            let shown = serde_json::to_value(&shown).expect("page");
            let rows = shown["files"]
                .as_array()
                .expect("files")
                .iter()
                .map(|file| file["matches"].as_array().map_or(0, Vec::len))
                .sum::<usize>();
            assert!(rows > 0, "{spans}: page {page} empty");
            assert!(
                priced <= 2 * rows + 1,
                "{spans}: page {page} priced {priced} for {rows} rows"
            );
        }
    }
}

/// A stored scan of many files, cut in small content pages, walks every hit
/// once: pages taken from the scan's kept cut join with no gap or repeat.
#[test]
fn a_stored_many_file_content_walk_shows_each_hit_once() {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..12 {
        let body = (0..20)
            .map(|line| format!("let needle_{file}_{line} = \"λ{line}\";\n"))
            .collect::<String>();
        fs::write(root.path().join(format!("f{file:02}.rs")), body).expect("file");
    }
    settle();
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["needle"]}),
        Some(2500),
    );
    assert!(pages.len() > 3, "{} pages", pages.len());
    let mut seen = std::collections::BTreeSet::new();
    for page in &pages {
        for file in page["files"].as_array().expect("files") {
            for row in file["matches"].as_array().expect("matches") {
                let key = (
                    file["path"].as_str().expect("path").to_owned(),
                    row["line"].as_u64().expect("line"),
                );
                assert!(seen.insert(key.clone()), "{key:?} twice");
            }
        }
    }
    assert_eq!(seen.len(), 12 * 20);
    assert_eq!(walk.rows.len(), 12 * 20);
}

/// Spans that cross a secret's edge, on the first page and on later pages
/// whose windows the stored scan already judged, each show the secret's
/// part masked; spans clear of it show their own bytes. Every span once.
#[test]
fn spans_across_a_secret_edge_stay_masked_on_every_page() {
    let token = "ghp_00ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567";
    let line = format!("aaa {token} {} {token} aaaa", "a".repeat(20_000));
    let secrets = line
        .match_indices(token)
        .map(|(at, _)| at..at + token.len())
        .collect::<Vec<_>>();
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("one.txt"), format!("{line}\n")).expect("file");
    settle();
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":[".{5}"],"regexEngine":"rust","view":"matchOnly"}),
        Some(2500),
    );
    assert_eq!(walk.rows.len(), line.len() / 5);
    let mut masked_on = std::collections::BTreeSet::new();
    for (index, page) in pages.iter().enumerate() {
        for row in page["files"][0]["matches"].as_array().expect("matches") {
            let at = row["column"].as_u64().expect("column") as usize - 1;
            let source = &line[at..at + 5];
            let value = row["value"].as_str().expect("value");
            if secrets
                .iter()
                .any(|secret| secret.start < at + 5 && at < secret.end)
            {
                assert_ne!(value, source, "page {index}: a secret's bytes shown");
                assert!(!token.contains(value), "page {index}: {value}");
                masked_on.insert(index);
            } else {
                assert_eq!(value, source, "page {index}");
            }
        }
    }
    assert!(masked_on.contains(&0), "{masked_on:?}");
    assert!(masked_on.iter().any(|&page| page > 1), "{masked_on:?}");
}

/// A stored file rewritten between pages, with its size and time kept, is
/// refused by the next page's digest check: what an earlier page judged of
/// its old bytes never settles the new ones, and the new secret is not shown.
#[test]
fn a_file_rewritten_between_pages_reuses_no_earlier_judgment() {
    let root = tempfile::tempdir().expect("fixture");
    let path = root.path().join("one.txt");
    let line = "a".repeat(3_000);
    fs::write(&path, format!("{line}\n")).expect("file");
    settle();
    let (policy, security) = policy_for(root.path());
    let q = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["a"],"view":"matchOnly"}),
        None,
    );
    let first = search_in(&q, &policy, &security, None).expect("page 1");
    let snapshot = first.source_snapshot.expect("snapshot");
    let page = |n: u32| ls_query(serde_json::json!({"snapshot":snapshot,"page":n}), Some(&q));
    search_in(&page(2), &policy, &security, None).expect("page 2");
    let token = " ghp_00ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567 ";
    let rewritten = format!(
        "{}{token}{}\n",
        &line[..1_000],
        &line[1_000 + token.len()..]
    );
    rewrite_keeping_stamp(&path, &rewritten);
    let error = search_in(&page(3), &policy, &security, None).expect_err("changed bytes");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::StaleSnapshot,
        "{error:?}"
    );
    assert!(!format!("{error:?}").contains("ghp_"), "{error:?}");
}

/// A one-file scope past the held bound is refused with a read of that file,
/// which localFetch pages whole; the count and listing leads, which would
/// only name the same file again, are not offered.
#[test]
fn a_one_file_scan_past_its_held_bound_leads_to_a_read_of_that_file() {
    let root = tempfile::tempdir().expect("fixture");
    let file = root.path().join("many.txt");
    fs::write(&file, "n\n".repeat(2_000)).expect("file");
    let (policy, security) = policy_for(root.path());
    let bounded = crate::tools::local_search::executor::test_held_limit(Some(16 * 1024));
    for view in ["content", "matchOnly"] {
        let q = ls_query(
            serde_json::json!({"path":file,"matchString":["n"],"view":view}),
            None,
        );
        let error = search_in(&q, &policy, &security, None).expect_err("past the bound");
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::ScopeTooBroad,
            "{view}: {}",
            error.message
        );
        assert!(
            error
                .hints
                .iter()
                .any(|hint| hint.contains("related[] localFetch call")),
            "{view}: {:?}",
            error.hints
        );
        let calls = error.calls.expect("leads");
        assert!(calls.get("viewFileCount").is_none(), "{calls}");
        assert!(calls.get("listScope").is_none(), "{calls}");
        let read = &calls["readFile"];
        assert_eq!(read["tool"], "localFetch", "{calls}");
        let read: octocode_contract_types::LocalFetchQuery =
            serde_json::from_value(read["input"]["queries"][0].clone()).expect("a localFetch row");
        let page = crate::tools::local_fetch::execute_local_fetch(
            &read,
            &policy,
            &security,
            &crate::execution::cancel::NeverCancel,
            None,
            None,
        );
        assert_eq!(page.error_code, None, "{view}: {page:?}");
        let content = page.content.as_deref().expect("page 1");
        assert!(content.starts_with("n\nn\n"), "{view}: {content:?}");
    }
    drop(bounded);
}

#[test]
fn match_pages_continue_from_their_file_pagination() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(
        root.path().join("a.txt"),
        (1..=400)
            .map(|line| format!("needle {line}\n"))
            .collect::<String>(),
    )
    .expect("source");
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["needle"],"matchPage":2}),
        Some(500),
    );
    assert!(!walk.rows.is_empty());
    assert!(pages.len() > 1);
    assert!(
        pages
            .iter()
            .any(|page| { page["files"][0]["pagination"]["next"]["nextPage"].is_object() })
    );
    assert!(pages.iter().all(|page| page["next"]["nextPage"].is_null()));
}

/// A file whose rows continue on later pages reports how many rows it has
/// in all, never a preview of their line numbers: the cursor reaches them.
#[test]
fn a_continuing_file_reports_its_total_without_a_line_preview() {
    let root = tempfile::tempdir().expect("fixture");
    let rows = 90_usize;
    fs::write(
        root.path().join("a.txt"),
        (1..=rows)
            .map(|line| format!("needle {line}\n"))
            .collect::<String>(),
    )
    .expect("source");
    let (_, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["needle"]}),
        Some(1500),
    );
    assert!(pages.len() > 1, "fixture must continue");
    for page in &pages[..pages.len() - 1] {
        let pagination = &page["files"][0]["pagination"];
        assert_eq!(pagination["hasMore"], true, "{pagination}");
        assert_eq!(pagination["totalItems"], rows, "{pagination}");
        for key in ["moreLines", "moreLinesUnlisted"] {
            assert!(pagination.get(key).is_none(), "{key} in {pagination}");
        }
    }
}

/// The count request a large result offers is a `related[]` call that names
/// itself; no hint restates it.
#[test]
fn a_large_result_offers_its_count_call_without_a_hint_restating_it() {
    let root = tempfile::tempdir().expect("fixture");
    let rows = crate::tools::local_search::leads::LARGE_RESULT_MATCHES as usize + 1;
    fs::write(root.path().join("a.txt"), "needle\n".repeat(rows)).expect("source");
    let (_, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["needle"]}),
        None,
    );
    let first = pages[0].to_string();
    assert!(!first.contains("related[]"), "{first}");
}

/// The read a search hands off windows its hits by the hit lines and the
/// handoff context alone: it never reads the file to learn its length, and
/// localFetch clips a window that runs past the end.
#[test]
fn the_read_handoff_windows_hits_without_reading_the_file() {
    use crate::tools::local_search::handoff::{READ_HANDOFF_CONTEXT, read_handoff};
    let root = tempfile::tempdir().expect("fixture");
    let hit = 9_u32;
    // A file shorter than the window: a clamp would end it at its last line.
    fs::write(root.path().join("a.txt"), "x\n".repeat(hit as usize)).expect("source");
    let query = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["x"]}),
        None,
    );
    let file = crate::tools::local_search::types::SearchFile {
        path: "a.txt".into(),
        match_keys: Vec::new(),
        declarations: Vec::new(),
        matches: Some(vec![crate::tools::local_search::types::SearchMatch {
            line: hit,
            column: None,
            value: "x".into(),
            byte_offset: 0,
            first_line: hit,
            match_lines: None,
            count: None,
            enclosing: None,
            declaration: None,
        }]),
        total_occurrences: None,
        total_matched_lines: None,
        term_counts: None,
        pagination: None,
    };
    let read = read_handoff(&query, root.path(), &file).expect("a read lead");
    let range = &read["input"]["queries"][0]["ranges"][0];
    assert_eq!(range["startLine"], hit - READ_HANDOFF_CONTEXT, "{read}");
    assert_eq!(range["endLine"], hit + READ_HANDOFF_CONTEXT, "{read}");
}

#[test]
fn overlapping_context_windows_reconstruct_every_requested_source_line_once() {
    let root = tempfile::tempdir().expect("fixture");
    let source: Vec<String> = (1..=240)
        .map(|line| {
            if matches!(line % 11, 0 | 3 | 5 | 7) {
                format!("let needle_{line} = {line};")
            } else {
                format!("let other_{line} = {line};")
            }
        })
        .collect();
    fs::write(root.path().join("a.rs"), source.join("\n") + "\n").expect("source");
    for (before, after) in [(3, 3), (4, 0), (0, 4), (1, 5)] {
        let (_, pages) = walk_search(
            root.path(),
            serde_json::json!({"path":root.path(),"matchString":["needle"],"contextLines":{"before":before,"after":after}}),
            Some(2500),
        );
        assert!(pages.len() > 1);
        let mut seen = std::collections::BTreeMap::new();
        for page in &pages {
            for file in page["files"].as_array().expect("files") {
                for row in file["matches"].as_array().expect("matches") {
                    for line in row["value"].as_str().expect("value").lines() {
                        let (number, text) = line.split_once('\t').expect("source coordinate");
                        let number = number.parse::<usize>().expect("line number");
                        assert_eq!(text, source[number - 1]);
                        assert!(
                            seen.insert(number, text.to_owned()).is_none(),
                            "line {number} duplicated"
                        );
                    }
                }
            }
        }
        let expected: std::collections::BTreeSet<_> = source
            .iter()
            .enumerate()
            .filter(|(_, text)| text.contains("needle"))
            .flat_map(|(index, _)| {
                (index + 1).saturating_sub(before).max(1)..=(index + 1 + after).min(source.len())
            })
            .collect();
        assert_eq!(
            seen.keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
    }
}

#[test]
fn complete_unicode_values_are_retained_even_when_larger_than_the_page_budget() {
    let root = tempfile::tempdir().expect("fixture");
    let source = format!("needle {} END_OF_EVIDENCE\n", "λ🦀text ".repeat(3000));
    fs::write(root.path().join("a.txt"), &source).expect("source");
    let (policy, security) = policy_for(root.path());
    let q = ls_query(
        serde_json::json!({"path":root.path(),"matchString":["needle"]}),
        None,
    );
    let found = search_in(&q, &policy, &security, Some(2500)).expect("search");
    let serialized = serde_json::to_value(&found).expect("result");
    crate::contracts::validate_output(
        "localSearch",
        &serde_json::json!({"results": [{"index": 0, "data": {"files": serialized["files"]}}]}),
    )
    .expect("complete match evidence satisfies the core contract");
    let shown = &serialized["files"][0]["matches"][0];
    assert_eq!(shown["value"], source.trim_end());
    assert!(shown.get("clipped").is_none());
    assert!(serialized["next"].get("expandValues").is_none());
}

#[test]
fn long_match_only_spans_page_once_with_complete_values_and_distinct_columns() {
    let root = tempfile::tempdir().expect("fixture");
    let first = "λ".repeat(3_000);
    let second = "🦀".repeat(1_000);
    fs::write(
        root.path().join("spans.txt"),
        format!("{first} gap {second}\n"),
    )
    .expect("source");
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["[λ🦀]+"],"regexEngine":"rust","view":"matchOnly"}),
        Some(2500),
    );
    assert_eq!(walk.rows.len(), 2);
    assert!(pages.len() > 1);
    let mut spans = pages
        .iter()
        .flat_map(|page| page["files"].as_array().expect("files"))
        .flat_map(|file| file["matches"].as_array().expect("matches"))
        .map(|row| {
            (
                row["column"].as_u64().expect("column"),
                row["value"].as_str().expect("value").to_owned(),
            )
        })
        .collect::<Vec<_>>();
    spans.sort_by_key(|(column, _)| *column);
    assert_eq!(spans, vec![(1, first), (3006, second)]);
}

#[test]
fn every_page_keeps_enclosing_declaration_coordinates() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.rs"), let_heavy_rust("handler", 60)).expect("source");
    let (walk, pages) = walk_search(
        root.path(),
        serde_json::json!({"path":root.path(),"matchString":["let "]}),
        Some(3000),
    );
    assert_eq!(walk.rows.len(), 300);
    for page in &pages {
        for file in page["files"].as_array().expect("files") {
            for index in 0..file["matches"].as_array().expect("matches").len() {
                let owner = owner_of(file, index).expect("declaration");
                let line = file["matches"][index]["line"].as_u64().expect("hit line");
                assert!(owner["line"].as_u64().expect("declaration start") <= line);
                assert!(owner["endLine"].as_u64().expect("declaration end") >= line);
                assert!(
                    owner["symbolName"]
                        .as_str()
                        .expect("name")
                        .starts_with("handler_")
                );
            }
        }
    }
}

/// Hits in one declaration share its entry, and the entry states it whole.
#[test]
fn a_file_lists_each_declaration_once_and_hits_name_it_by_index() {
    let files = [(
        "a.rs",
        "fn first() {\n    needle();\n    needle();\n}\nfn second() {\n    needle();\n}\n",
    )];
    let result = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString":["needle"]}), None),
    );
    let file = &result["files"][0];
    let declarations = file["declarations"].as_array().expect("declarations");
    assert_eq!(declarations.len(), 2, "{result}");
    assert_eq!(declarations[0]["symbolName"], "first");
    assert_eq!(declarations[0]["line"], 1);
    assert_eq!(declarations[0]["endLine"], 4);
    assert_eq!(declarations[1]["symbolName"], "second");
    let owners = file["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .map(|row| row["enclosing"].as_u64().expect("index"))
        .collect::<Vec<_>>();
    assert_eq!(owners, [0, 0, 1]);
}

/// A declaration lists the same start lines in localSearch `declarations[]`
/// as in astSearch `symbols`: `docStartLine` (its doc comment) and
/// `startLine` (its attributes) when they come before the name line, so a
/// read from either starts at the same line. A declaration with neither
/// lists neither.
#[test]
fn declarations_carry_the_start_lines_astsearch_symbols_print() {
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("a.rs");
    fs::write(
        &source,
        "/// Checks the needle.\n/// Twice.\n#[test]\n#[allow(dead_code)]\nfn checked() {\n    needle();\n}\nfn plain() {\n    needle();\n}\n",
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy(), "matchString":["needle"]}),
        None,
    );
    let result = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("serialize");
    let declarations = result["files"][0]["declarations"]
        .as_array()
        .expect("declarations")
        .clone();
    let symbols_row = crate::tools::test_support::opened(
        crate::id::ToolId::AstSearch,
        &serde_json::json!({"operation":"symbols","path":source}),
    );
    let symbols = crate::tools::ast_search::execute_ast(
        &serde_json::from_value(symbols_row).expect("astSearch row"),
        &policy,
        &security,
        &NeverCancel,
        None,
    )
    .expect("symbols");
    let symbol = |name: &str| {
        symbols["symbols"]
            .as_array()
            .expect("symbols rows")
            .iter()
            .find(|row| row["symbolName"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("{name} in {symbols}"))
    };
    let lines = |row: &serde_json::Value| {
        (
            row["line"].as_u64(),
            row["startLine"].as_u64(),
            row["docStartLine"].as_u64(),
        )
    };
    assert_eq!(declarations.len(), 2, "{result}");
    for declaration in &declarations {
        let name = declaration["symbolName"].as_str().expect("name");
        assert_eq!(lines(declaration), lines(&symbol(name)), "{declaration}");
    }
    assert_eq!(
        lines(&declarations[0]),
        (Some(5), Some(3), Some(1)),
        "{result}"
    );
    assert_eq!(lines(&declarations[1]), (Some(8), None, None), "{result}");
}

/// `matchOnly` selects each hit's text and place; the content view is the one
/// that names its enclosing declaration.
#[test]
fn match_only_rows_name_no_enclosing_declaration() {
    let files = [("a.rs", "fn owner() {\n    needle();\n}\n")];
    let row = |view: &str| {
        let result = search_fixture(
            &files,
            ls_query(
                serde_json::json!({"matchString":["needle"],"view":view}),
                None,
            ),
        );
        result["files"][0]["matches"][0].clone()
    };
    assert_eq!(row("content")["enclosing"], 0);
    let hit = row("matchOnly");
    assert_eq!(hit["value"], "needle");
    assert_eq!(hit["line"], 2);
    assert!(hit.get("enclosing").is_none(), "{hit}");
}

#[test]
fn match_only_uniqueness_counts_complete_values_per_file() {
    let files = [
        ("a.txt", "alpha alpha\nbeta\nalpha\n"),
        ("b.txt", "alpha\n"),
    ];
    let result = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString":["alpha","beta"],"view":"matchOnly","unique":"count"}),
            None,
        ),
    );
    let first = result["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|file| file["path"] == "a.txt")
        .expect("a");
    let rows = first["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{result}");
    let counts: std::collections::BTreeMap<_, _> = rows
        .iter()
        .map(|row| {
            (
                row["value"].as_str().expect("value"),
                row["count"].as_u64().unwrap_or(1),
            )
        })
        .collect();
    assert_eq!(counts["alpha"], 3);
    assert_eq!(counts["beta"], 1);
}

mod large_scan;
