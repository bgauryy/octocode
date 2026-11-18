use super::*;
use octocode_test_support::json_fixture;

/// A row of hit `line` whose value the search joined from `first_line` on.
fn row(first_line: u32, line: u32, value: &str) -> SearchMatch {
    SearchMatch {
        line,
        column: None,
        value: value.into(),
        window_start: None,
        byte_offset: 0,
        first_line,
        end_line: None,
        match_lines: None,
        match_bytes: Vec::new(),
        count: None,
        enclosing: None,
        declaration: None,
    }
}

/// A window is numbered from the line the search says its value starts at.
/// When its before-context stopped at a line the search left out (test code
/// under `testCode`), its lines are numbered from there, not from the
/// requested reach; the after-context stopping early is numbered likewise.
#[test]
fn a_window_beside_a_left_out_line_is_numbered_at_its_own_lines() {
    // Hit 5, ±2, line 3 left out: the value joins lines 4..=7.
    let before_gap = merge_context_windows(
        vec![row(4, 5, "l4\nl5\nl6\nl7")],
        SearchContext::symmetric(2),
        &[w(5, 4, 7)],
    );
    assert_eq!(before_gap[0].value, "4\tl4\n5\tl5\n6\tl6\n7\tl7");
    // Hit 5, ±2, line 6 left out: the value joins lines 3..=5.
    let after_gap = merge_context_windows(
        vec![row(3, 5, "l3\nl4\nl5")],
        SearchContext::symmetric(2),
        &[w(5, 3, 5)],
    );
    assert_eq!(after_gap[0].value, "3\tl3\n4\tl4\n5\tl5");
    // Both sides cut: hit 5, ±2, lines 3 and 7 left out.
    let both = merge_context_windows(
        vec![row(4, 5, "l4\nl5\nl6")],
        SearchContext::symmetric(2),
        &[w(5, 4, 6)],
    );
    assert_eq!(both[0].value, "4\tl4\n5\tl5\n6\tl6");
    // A neighbouring hit's window still merges at its true lines.
    let merged = merge_context_windows(
        vec![row(4, 5, "l4\nl5\nl6\nl7"), row(5, 7, "l5\nl6\nl7\nl8\nl9")],
        SearchContext::symmetric(2),
        &[w(5, 4, 7), w(7, 5, 9)],
    );
    assert_eq!(merged.len(), 1, "{merged:?}");
    assert_eq!(merged[0].value, "4\tl4\n5\tl5\n6\tl6\n7\tl7\n8\tl8\n9\tl9");
    assert_eq!(merged[0].match_lines, Some(vec![5, 7]));
    // A value whose lines disagree with where it starts is shown verbatim.
    let withheld = merge_context_windows(
        vec![row(4, 5, "[REDACTED: withheld]")],
        SearchContext::symmetric(2),
        &[w(5, 5, 5)],
    );
    assert_eq!(withheld[0].value, "[REDACTED: withheld]");
}

/// Hit `line`'s window, holding source lines `first..=last`.
fn w(line: u32, first: u32, last: u32) -> HitWindow {
    HitWindow {
        line,
        matched_last: line,
        first,
        last,
    }
}

/// Every numbered line of `rows`, checked: each number once, each inside the
/// window of the row showing it (`windows`: first and last line per row
/// head), and the set of shown numbers is `expected`. Returns the lines.
fn shown_once(
    rows: &[SearchMatch],
    windows: &[(u32, u32, u32)],
    expected: &[u32],
) -> std::collections::BTreeMap<u32, String> {
    let mut shown = std::collections::BTreeMap::new();
    for row in rows {
        let lines = crate::local_search::enclosing::row_lines(row);
        for numbered in row.value.split('\n') {
            let (number, text) = numbered.split_once('\t').expect("a numbered line");
            let number: u32 = number.parse().expect("a line number");
            assert!(
                windows
                    .iter()
                    .any(|&(hit, first, last)| lines.contains(&hit)
                        && (first..=last).contains(&number)),
                "line {number} shown outside its window: {rows:?}"
            );
            assert!(
                shown.insert(number, text.to_owned()).is_none(),
                "line {number} shown twice: {rows:?}"
            );
        }
    }
    assert_eq!(
        shown.keys().copied().collect::<Vec<_>>(),
        expected,
        "{rows:?}"
    );
    shown
}

/// A line a window holds is shown by a row that holds it, even when the
/// requested reach of a nearer hit covers it: hits 2 and 7 with ±3, line 6
/// left out (`testCode`), so 7's window starts at 7 and line 5 is only 2's.
/// Every held line shows once, inside its window, on one page or split
/// across pages.
#[test]
fn a_line_only_one_window_holds_is_shown_by_that_window() {
    let context = SearchContext::symmetric(3);
    let windows = [(2, 1, 5), (7, 7, 10)];
    let early = row(1, 2, "l1\nl2\nl3\nl4\nl5");
    let late = row(7, 7, "l7\nl8\nl9\nl10");
    let hits = [w(2, 1, 5), w(7, 7, 10)];
    let expected = [1, 2, 3, 4, 5, 7, 8, 9, 10];
    let together = merge_context_windows(vec![early.clone(), late.clone()], context, &hits);
    let shown = shown_once(&together, &windows, &expected);
    assert!(
        shown.iter().all(|(n, text)| *text == format!("l{n}")),
        "{shown:?}"
    );
    let mut paged = merge_context_windows(vec![early], context, &hits);
    paged.extend(merge_context_windows(vec![late], context, &hits));
    shown_once(&paged, &windows, &expected);
}

/// End to end, under `testCode` exclude and only: two kept hits whose
/// windows a left-out match line between them cuts show every line they
/// hold, once, numbered at its own line with its own text.
#[test]
fn a_search_beside_a_left_out_line_shows_every_held_line_once() {
    let excluded = "l1\n// marker p\nl3\nl4\n#[cfg(test)]\nmod t { /* marker t */ }\n// marker p\nl8\nl9\nl10\n";
    let only = "#[cfg(test)]\nmod a { /* marker t */ }\nl3\nl4\n// marker p\n#[cfg(test)]\nmod b { /* marker t */ }\nl8\nl9\nl10\n";
    // exclude: test line 6 left out between hits 2 and 7. only: production
    // line 5 left out; hit 7's window starts at its item's attribute line.
    // (testCode, source, (hit, first, last) per window, lines shown)
    type Case<'a> = (&'a str, &'a str, [(u32, u32, u32); 2], &'a [u32]);
    let cases: [Case<'_>; 2] = [
        (
            "exclude",
            excluded,
            [(2, 1, 5), (7, 7, 10)],
            &[1, 2, 3, 4, 5, 7, 8, 9, 10],
        ),
        (
            "only",
            only,
            [(2, 1, 4), (7, 6, 10)],
            &[1, 2, 3, 4, 6, 7, 8, 9, 10],
        ),
    ];
    for (test_code, text, windows, expected) in cases {
        let root = tempfile::tempdir().expect("fixture");
        let path = root.path().join("a.rs");
        std::fs::write(&path, text).expect("fixture");
        let query: octocode_contract_types::LocalSearchQuery =
            serde_json::from_value(serde_json::json!({
                "path": path, "matchString": ["marker"], "view": "content",
                "testCode": test_code, "contextLines": {"before": 3, "after": 3}
            }))
            .expect("query");
        let result = crate::local_search::execute_local_search(
            octocode_runtime_base::test_support::BUILD_IDENTITY,
            &query,
            &octocode_runtime_base::test_support::workspace_policy(root.path()),
            &octocode_runtime_base::security::ContentSecurity::new(),
            &octocode_runtime_base::execution::cancel::NeverCancel,
            None,
            None,
        )
        .expect("search");
        let rows = result
            .files
            .iter()
            .flat_map(|file| file.matches.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        let source = text.lines().collect::<Vec<_>>();
        let shown = shown_once(&rows, &windows, expected);
        for (number, text) in &shown {
            assert_eq!(
                text,
                source[*number as usize - 1],
                "{test_code}: line {number}"
            );
        }
    }
}

/// A match spanning lines 3–4 with one line of context is numbered as
/// rg prints it, every line it covers a matched line.
#[test]
fn a_multiline_match_window_is_numbered_from_its_first_context_line() {
    let mut spanned = row(2, 3, "two\nfn new()\n  -> Self\nfive");
    spanned.end_line = Some(4);
    let merged = merge_context_windows(vec![spanned], SearchContext::symmetric(1), &[w(3, 2, 5)]);
    assert_eq!(
        merged[0].value,
        "2\ttwo\n3\tfn new()\n4\t  -> Self\n5\tfive"
    );
    assert_eq!(
        (merged[0].end_line, &merged[0].match_lines),
        (Some(4), &None)
    );
    assert_eq!(merged[0].line, 3);
}

/// Each line two windows hold belongs to the nearer hit (the earlier on a
/// tie); a line only one window holds is that hit's.
#[test]
fn asymmetric_ownership_covers_the_union_once() {
    let hits = [1u32, 3, 4, 12, 20];
    for before in 0..=10 {
        for after in 0..=10 {
            let context = SearchContext { before, after };
            let mut seen = std::collections::BTreeMap::<u32, usize>::new();
            let mut expected = std::collections::BTreeSet::new();
            let windows = hits
                .iter()
                .map(|&hit| w(hit, hit.saturating_sub(before).max(1), hit + after))
                .collect::<Vec<_>>();
            for &window in &windows {
                expected.extend(window.first..=window.last);
                let (low, high) = owned_lines(window, &windows);
                for line in low..=high {
                    *seen.entry(line).or_default() += 1;
                }
            }
            assert_eq!(
                seen.keys()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>(),
                expected
            );
            assert!(
                seen.values().all(|&count| count == 1),
                "{context:?}: {seen:?}"
            );
        }
    }
}

#[test]
fn every_window_line_has_one_owning_hit() {
    let hits = [w(10, 7, 13), w(13, 10, 16), w(30, 27, 33)];
    assert_eq!(owned_lines(hits[0], &hits), (7, 11));
    assert_eq!(owned_lines(hits[1], &hits), (12, 16));
    assert_eq!(owned_lines(hits[2], &hits), (27, 33));
    // Hits 3 apart with ±1: no window line is shared.
    let apart = [w(5, 4, 6), w(8, 7, 9)];
    assert_eq!(owned_lines(apart[0], &apart), (4, 6));
    assert_eq!(owned_lines(apart[1], &apart), (7, 9));
    // A row whose neighbour is on another page shows only its own
    // lines: 13's window on its own page starts after 10's line 11.
    let lines = (1..=20).map(|n| format!("l{n}")).collect::<Vec<_>>();
    let merged = merge_context_windows(
        vec![row(10, 13, &lines[9..16].join("\n"))],
        SearchContext::symmetric(3),
        &hits,
    );
    assert_eq!(
        merged[0].value,
        "12\tl12\n13\tl13\n14\tl14\n15\tl15\n16\tl16"
    );
}

#[test]
fn splices_only_when_shared_lines_agree() {
    // Rows 5 and 6 with ±1 context share lines 5..=6.
    let merged = merge_context_windows(
        vec![row(4, 5, "l4\nl5\nl6"), row(5, 6, "l5\nl6\nl7")],
        SearchContext::symmetric(1),
        &[],
    );
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].value, "4\tl4\n5\tl5\n6\tl6\n7\tl7");
    assert_eq!(merged[0].match_lines, Some(vec![5, 6]));
    // A window ending on a blank line keeps that line numbered.
    let blank = merge_context_windows(
        vec![row(4, 5, "l4\nl5\n")],
        SearchContext::symmetric(1),
        &[],
    );
    assert_eq!(blank[0].value, "4\tl4\n5\tl5\n6\t");
    // A redacted/rewritten shared line keeps both rows verbatim.
    let apart = merge_context_windows(
        vec![row(4, 5, "l4\nl5\nl6"), row(5, 6, "l5\n[REDACTED]\nl7")],
        SearchContext::symmetric(1),
        &[],
    );
    assert_eq!(apart.len(), 2);
    assert!(apart.iter().all(|m| m.match_lines.is_none()));
    // A value whose line count cannot be a ±1 window is never spliced.
    let odd = merge_context_windows(
        vec![row(4, 5, "l4\nl5\nl6"), row(5, 6, "one line")],
        SearchContext::symmetric(1),
        &[],
    );
    assert_eq!(odd.len(), 2);
    assert_eq!(odd[1].value, "one line");
    // Disjoint windows (gap at line 7..) stay separate rows.
    let gap = merge_context_windows(
        vec![row(1, 2, "l1\nl2\nl3"), row(8, 9, "l8\nl9\nl10")],
        SearchContext::symmetric(1),
        &[],
    );
    assert_eq!(gap.len(), 2);
    // Adjacent windows merge without imposing a value-size cap.
    let merged = merge_context_windows(
        vec![row(4, 5, "l4\nl5\nl6"), row(5, 6, "l5\nl6\nl7")],
        SearchContext::symmetric(1),
        &[],
    );
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].value, "4\tl4\n5\tl5\n6\tl6\n7\tl7");
}

#[test]
fn wide_overlapping_windows_merge_without_repeating_source_lines() {
    // Three overlapping windows retain every selected source line once.
    let lines: Vec<String> = (1..=20).map(|n| format!("line{n:02}")).collect();
    let window = |line: u32| {
        let lo = line.saturating_sub(3).max(1);
        let hi = (line + 3).min(20) as usize;
        row(lo, line, &lines[lo as usize - 1..hi].join("\n"))
    };
    let rows = vec![window(5), window(7), window(9)];
    let merged = merge_context_windows(rows, SearchContext::symmetric(3), &[]);
    assert_eq!(merged.len(), 1, "{merged:?}");
    let numbered = (2..=12)
        .map(|n| format!("{n}\tline{n:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(merged[0].value, numbered);
    assert_eq!(merged[0].match_lines, Some(vec![5, 7, 9]));
}

/// A match spanning lines 3–4 and a hit on line 7, each with three lines of
/// context: the lines their windows share (4–7) belong to one row each, on
/// one page and when the rows are paged apart.
#[test]
fn a_multiline_hit_and_a_nearby_hit_show_each_line_once() {
    let context = SearchContext::symmetric(3);
    let windows = [(3, 1, 7), (7, 4, 10)];
    let mut spanned = row(1, 3, "l1\nl2\nl3\nl4\nl5\nl6\nl7");
    spanned.end_line = Some(4);
    let nearby = row(4, 7, "l4\nl5\nl6\nl7\nl8\nl9\nl10");
    let hits = [w(3, 1, 7), w(7, 4, 10)];
    let expected = (1..=10).collect::<Vec<_>>();
    let together = merge_context_windows(vec![spanned.clone(), nearby.clone()], context, &hits);
    let shown = shown_once(&together, &windows, &expected);
    assert!(
        shown.iter().all(|(n, text)| *text == format!("l{n}")),
        "{shown:?}"
    );
    let mut paged = merge_context_windows(vec![spanned], context, &hits);
    paged.extend(merge_context_windows(vec![nearby], context, &hits));
    shown_once(&paged, &windows, &expected);
}

/// The lines a match covers are its hit's even when a nearer hit's window
/// holds them: a match over lines 3–6 and a hit on line 7 share the window
/// lines 4–9, and the match keeps all of 3–6.
#[test]
fn a_hit_beside_a_long_match_never_takes_its_covered_lines() {
    let context = SearchContext::symmetric(3);
    let windows = [(3, 1, 9), (7, 4, 10)];
    let mut spanned = row(1, 3, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9");
    spanned.end_line = Some(6);
    let nearby = row(4, 7, "l4\nl5\nl6\nl7\nl8\nl9\nl10");
    let hits = [
        HitWindow {
            line: 3,
            matched_last: 6,
            first: 1,
            last: 9,
        },
        w(7, 4, 10),
    ];
    let merged = merge_context_windows(vec![spanned, nearby], context, &hits);
    shown_once(&merged, &windows, &(1..=10).collect::<Vec<_>>());
    assert_eq!(
        (merged[0].end_line, &merged[0].match_lines),
        (Some(6), &None)
    );
}

/// Runs a localSearch over `text` and returns every row it shows.
fn search_rows(text: &str, query: serde_json::Value) -> Vec<SearchMatch> {
    let root = tempfile::tempdir().expect("fixture");
    let path = root.path().join("a.txt");
    std::fs::write(&path, text).expect("fixture");
    let mut query = query;
    query["path"] = serde_json::json!(path);
    let query: octocode_contract_types::LocalSearchQuery =
        serde_json::from_value(query).expect("query");
    let result = crate::local_search::execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &octocode_runtime_base::test_support::workspace_policy(root.path()),
        &octocode_runtime_base::security::ContentSecurity::new(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
        None,
        None,
    )
    .expect("search");
    result
        .files
        .iter()
        .flat_map(|file| file.matches.clone().unwrap_or_default())
        .collect()
}

/// A one-line match and a multiline match starting on the same line, and a
/// hit on line 6, with two lines of context: every line shows once and the
/// lines the multiline match covers stay with it.
#[test]
fn a_single_and_a_multiline_match_on_one_line_keep_the_covered_lines() {
    let text = "l1\nl2\nx foo(\n  bar)\nl5\ny\nl7\nl8\nl9\n";
    let rows = search_rows(
        text,
        json_fixture(
            r#"{
                "matchString": ["x|foo\\(\\n  bar\\)|y"],
                "regexEngine": "rust",
                "multiline": "on",
                "view": "content",
                "contextLines": {"before": 2, "after": 2}
            }"#,
        ),
    );
    let mut shown = std::collections::BTreeSet::new();
    for row in &rows {
        for numbered in row.value.split('\n') {
            let (number, _) = numbered.split_once('\t').expect("numbered");
            let number: u32 = number.parse().expect("number");
            assert!(shown.insert(number), "line {number} twice: {rows:?}");
        }
    }
    assert_eq!(
        shown.into_iter().collect::<Vec<_>>(),
        (1..=8).collect::<Vec<_>>(),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.end_line == Some(4) && row.value.contains("4\t  bar)")),
        "{rows:?}"
    );
}

/// Two multiline matches whose windows share lines: the lines between them
/// belong to the nearer one, each shown once, both matches whole.
#[test]
fn two_adjacent_multiline_hits_share_a_window_without_repeating_a_line() {
    let context = SearchContext::symmetric(3);
    let mut first = row(1, 3, "l1\nl2\nl3\nl4\nl5\nl6\nl7");
    first.end_line = Some(4);
    let mut second = row(4, 7, "l4\nl5\nl6\nl7\nl8\nl9\nl10\nl11");
    second.end_line = Some(8);
    let hits = [
        HitWindow {
            line: 3,
            matched_last: 4,
            first: 1,
            last: 7,
        },
        HitWindow {
            line: 7,
            matched_last: 8,
            first: 4,
            last: 11,
        },
    ];
    let merged = merge_context_windows(vec![first, second], context, &hits);
    let windows = [(3, 1, 7), (7, 4, 11)];
    shown_once(&merged, &windows, &(1..=11).collect::<Vec<_>>());
    assert_eq!(merged[0].end_line, Some(4));
    assert_eq!(merged[1].end_line, Some(8));
}

/// A redacted multiline row stays whole (it numbers no line) beside a
/// neighbour that is trimmed.
#[test]
fn a_redacted_multiline_row_stays_whole_beside_a_trimmed_neighbour() {
    let context = SearchContext::symmetric(3);
    let mut redacted = row(1, 3, "[REDACTED: secret]");
    redacted.end_line = Some(4);
    let nearby = row(4, 7, "l4\nl5\nl6\nl7\nl8\nl9\nl10");
    let hits = [
        HitWindow {
            line: 3,
            matched_last: 4,
            first: 3,
            last: 4,
        },
        w(7, 4, 10),
    ];
    let merged = merge_context_windows(vec![redacted, nearby], context, &hits);
    assert_eq!(merged[0].value, "[REDACTED: secret]");
    assert_eq!(merged[0].end_line, Some(4));
    assert!(merged[1].value.starts_with("5\tl5"), "{:?}", merged[1]);
}

/// What the wire says of each row of a multiline search over
/// [`MULTILINE_TEXT`]: its line, `endLine` and `matchedLines`.
fn multiline_wire(pattern: &str, before: u32) -> Vec<(u64, Option<u64>, Option<Vec<u64>>)> {
    search_rows(
        MULTILINE_TEXT,
        serde_json::json!({
            "matchString": [pattern], "regexEngine": "rust",
            "multiline": "on", "view": "content",
            "contextLines": {"before": before, "after": 0}
        }),
    )
    .iter()
    .map(|row| {
        let wire = serde_json::to_value(row).expect("row");
        (
            wire["line"].as_u64().expect("line"),
            wire["endLine"].as_u64(),
            serde_json::from_value(wire["matchedLines"].clone()).ok(),
        )
    })
    .collect()
}

/// One-line hits on lines 1, 2, 4, 5 and 7; `fold(` opens on line 3.
const MULTILINE_TEXT: &str = "x one\nx two\nfold(\n  y x\nx three\nmid\nx four\n";

/// A multiline search: one-line hits on adjacent lines stay one hit each,
/// never one match spanning them; a match over lines 3-4 states its extent
/// in `endLine`. `matchedLines` only names the hits a merged context window
/// holds, and every hit is named exactly once.
#[test]
fn a_multiline_row_states_its_own_extent_and_adjacent_hits_stay_separate() {
    for before in [0, 1] {
        // Repro A: the newline alternative never matches.
        let rows = multiline_wire(r"\bx\b|zz\n", before);
        assert!(rows.iter().all(|(_, end, _)| end.is_none()), "{rows:?}");
        let hits = rows
            .iter()
            .flat_map(|(line, _, matched)| matched.clone().unwrap_or_else(|| vec![*line]))
            .collect::<Vec<_>>();
        assert_eq!(hits, [1, 2, 4, 5, 7], "before {before}: {rows:?}");
        if before == 0 {
            assert!(
                rows.iter().all(|(_, _, matched)| matched.is_none()),
                "{rows:?}"
            );
        }

        // Repro B: `fold(` and the next line are one match.
        let rows = multiline_wire(r"fold\(\n.*|\bx\b", before);
        let spanned = rows
            .iter()
            .find(|(line, _, _)| *line == 3)
            .expect("the match on line 3");
        assert_eq!((spanned.1, &spanned.2), (Some(4), &None), "{rows:?}");
        let hits = rows
            .iter()
            .flat_map(|(line, _, matched)| matched.clone().unwrap_or_else(|| vec![*line]))
            .collect::<Vec<_>>();
        assert_eq!(hits, [1, 2, 3, 5, 7], "before {before}: {rows:?}");
    }
}
