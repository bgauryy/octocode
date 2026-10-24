use super::*;

fn row(line: u32, value: &str) -> SearchMatch {
    SearchMatch {
        line,
        column: None,
        value: value.into(),
        byte_offset: 0,
        match_lines: None,
        count: None,
        clipped: false,
        original_chars: None,

        original_bytes: None,
        returned_chars: None,
        enclosing: None,
        declaration: None,
    }
}

/// A match spanning lines 3–4 with one line of context is numbered as
/// rg prints it, every line it covers a matched line.
#[test]
fn a_multiline_match_window_is_numbered_from_its_first_context_line() {
    let mut spanned = row(3, "two\nfn new()\n  -> Self\nfive");
    spanned.match_lines = Some(vec![3, 4]);
    let merged = merge_context_windows(vec![spanned], SearchContext::symmetric(1), 500, &[3]);
    assert_eq!(
        merged[0].value,
        "2\ttwo\n3\tfn new()\n4\t  -> Self\n5\tfive"
    );
    assert_eq!(merged[0].match_lines, Some(vec![3, 4]));
    assert_eq!(merged[0].line, 3);
}

/// Each line within reach of two hits belongs to the nearer (the
/// earlier on a tie); a line only one hit reaches is that hit's.
#[test]
fn asymmetric_ownership_covers_the_union_once() {
    let hits = [1u32, 3, 4, 12, 20];
    for before in 0..=10 {
        for after in 0..=10 {
            let context = SearchContext { before, after };
            let mut seen = std::collections::BTreeMap::<u32, usize>::new();
            let mut expected = std::collections::BTreeSet::new();
            for &hit in &hits {
                expected.extend(hit.saturating_sub(before).max(1)..=hit + after);
                let (low, high) = owned_lines(hit, context, &hits);
                for line in low.max(1)..=high {
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
    let hits = [10, 13, 30];
    assert_eq!(owned_lines(10, SearchContext::symmetric(3), &hits), (7, 11));
    assert_eq!(
        owned_lines(13, SearchContext::symmetric(3), &hits),
        (12, 16)
    );
    assert_eq!(
        owned_lines(30, SearchContext::symmetric(3), &hits),
        (27, 33)
    );
    // Hits 3 apart with ±1: no window line is shared.
    assert_eq!(owned_lines(5, SearchContext::symmetric(1), &[5, 8]), (4, 6));
    assert_eq!(owned_lines(8, SearchContext::symmetric(1), &[5, 8]), (7, 9));
    // A row whose neighbour is on another page shows only its own
    // lines: 13's window on its own page starts after 10's line 11.
    let lines = (1..=20).map(|n| format!("l{n}")).collect::<Vec<_>>();
    let merged = merge_context_windows(
        vec![row(13, &lines[9..16].join("\n"))],
        SearchContext::symmetric(3),
        500,
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
        vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")],
        SearchContext::symmetric(1),
        500,
        &[],
    );
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].value, "4\tl4\n5\tl5\n6\tl6\n7\tl7");
    assert_eq!(merged[0].match_lines, Some(vec![5, 6]));
    // A window ending on a blank line keeps that line numbered.
    let blank = merge_context_windows(
        vec![row(5, "l4\nl5\n")],
        SearchContext::symmetric(1),
        500,
        &[],
    );
    assert_eq!(blank[0].value, "4\tl4\n5\tl5\n6\t");
    // A redacted/rewritten shared line keeps both rows verbatim.
    let apart = merge_context_windows(
        vec![row(5, "l4\nl5\nl6"), row(6, "l5\n[REDACTED]\nl7")],
        SearchContext::symmetric(1),
        500,
        &[],
    );
    assert_eq!(apart.len(), 2);
    assert!(apart.iter().all(|m| m.match_lines.is_none()));
    // A value whose line count cannot be a ±1 window is never spliced.
    let odd = merge_context_windows(
        vec![row(5, "l4\nl5\nl6"), row(6, "one line")],
        SearchContext::symmetric(1),
        500,
        &[],
    );
    assert_eq!(odd.len(), 2);
    assert_eq!(odd[1].value, "one line");
    // Disjoint windows (gap at line 7..) stay separate rows.
    let gap = merge_context_windows(
        vec![row(2, "l1\nl2\nl3"), row(9, "l8\nl9\nl10")],
        SearchContext::symmetric(1),
        500,
        &[],
    );
    assert_eq!(gap.len(), 2);
    // A merged block may use the budgets of the rows it replaces (here
    // 2 × 8 chars), so it is never larger than those rows unmerged.
    let within = merge_context_windows(
        vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")],
        SearchContext::symmetric(1),
        8,
        &[],
    );
    assert_eq!(within.len(), 1, "two 8-char rows merge into 11 chars");
    let capped = merge_context_windows(
        vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")],
        SearchContext::symmetric(1),
        5,
        &[],
    );
    assert_eq!(
        capped.len(),
        2,
        "a merge must not exceed the merged rows' matchContentLength budgets"
    );
}

#[test]
fn wide_overlapping_windows_merge_up_to_the_rows_combined_budget() {
    // Three ±3 windows over 6-char lines (48 chars each): any two merged
    // exceed one row's 60-char cap, all three fit their combined budget.
    let lines: Vec<String> = (1..=20).map(|n| format!("line{n:02}")).collect();
    let window = |line: u32| {
        let lo = line.saturating_sub(3).max(1) as usize;
        let hi = (line + 3).min(20) as usize;
        lines[lo - 1..hi].join("\n")
    };
    let rows = vec![row(5, &window(5)), row(7, &window(7)), row(9, &window(9))];
    let merged = merge_context_windows(rows, SearchContext::symmetric(3), 60, &[]);
    assert_eq!(merged.len(), 1, "{merged:?}");
    let numbered = (2..=12)
        .map(|n| format!("{n}\tline{n:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(merged[0].value, numbered);
    assert_eq!(merged[0].match_lines, Some(vec![5, 7, 9]));
}
