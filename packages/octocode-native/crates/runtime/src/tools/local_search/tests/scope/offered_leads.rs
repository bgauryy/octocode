//! Leads: every offered lead validates, and every clipped value is one
//! executable expandValues call away.

use super::*;

/// The `expandValues` lead of one page: its tool and its rows, after
/// asserting the page offers no other value-expansion lead and states no
/// value left out.
fn page_expansion(result: &LocalSearchResult) -> Option<(String, Vec<serde_json::Value>)> {
    let next = result.next.clone().unwrap_or_default();
    let names = next
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name.starts_with("expandValues"))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    assert!(names.len() <= 1, "one lead name: {names:?}");
    assert!(
        !result
            .warnings
            .iter()
            .any(|w| w.starts_with("expandValues")),
        "no value is left without a read: {:?}",
        result.warnings
    );
    let lead = next.get("expandValues")?;
    let rows = lead["query"]["queries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        rows.len() <= crate::response::result::rows_per_call(crate::id::ToolId::LocalFetch),
        "{lead}"
    );
    Some((lead["tool"].as_str().unwrap_or_default().to_owned(), rows))
}

/// A result large enough to lead to per-file counts, read as matchOnly
/// with `unique`, leads to a call the schema accepts, for every view and
/// uniqueness mode: a lead never fails validation.
#[test]
fn every_lead_of_every_view_validates_against_the_schema() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.txt"), "needle one\n".repeat(900)).expect("a");
    fs::write(root.path().join("b.txt"), "needle two\n".repeat(900)).expect("b");
    let (policy, security) = policy_for(root.path());
    let views = [
        LocalSearchQueryResultView::Paginated,
        LocalSearchQueryResultView::MatchOnly,
        LocalSearchQueryResultView::Files,
        LocalSearchQueryResultView::CountMatches,
    ];
    let uniques = [
        LocalSearchQueryUnique::Off,
        LocalSearchQueryUnique::Count,
        LocalSearchQueryUnique::List,
    ];
    for view in views {
        for unique in uniques {
            let only = view == LocalSearchQueryResultView::MatchOnly;
            if !only && unique != LocalSearchQueryUnique::Off {
                continue;
            }
            for page_size in [None, Some(1)] {
                let request = ls_query(
                    serde_json::json!({
                        "path": root.path().to_string_lossy().into_owned(),
                        "matchString": "needle",
                        "resultView": view,
                        "unique": unique,
                        "pageSize": page_size,
                    }),
                    None,
                );
                let result = execute_local_search(
                    &request,
                    &policy,
                    &security,
                    &NeverCancel,
                    None,
                    None,
                    None,
                )
                .expect("search");
                let body = serde_json::to_value(result).expect("serialize");
                assert_leads_validate(&body, &format!("{view:?}/{unique:?}/{page_size:?}"));
            }
        }
    }
}

/// Every default page holds no more clipped-value reads than one
/// `expandValues` call runs: a page with clipped values in more files than
/// one call holds ends early, and the rest follow on `nextPage`. Walking
/// the pages reads every clipped value once.
#[test]
fn every_clipped_value_on_a_default_page_is_one_expand_values_call_away() {
    let limit = crate::response::result::rows_per_call(crate::id::ToolId::LocalFetch);
    let root = tempfile::tempdir().expect("fixture directory");
    let total = limit + 2;
    for index in 0..total {
        fs::write(
            root.path().join(format!("f{index}.txt")),
            format!("alpha {}\n", "q".repeat(900)),
        )
        .expect("fixture");
    }
    // One file whose clipped rows need more range reads than one call holds.
    let mut wide = String::new();
    for _ in 0..(limit * 10 + 15) {
        wide.push_str(&format!("alpha {}\n\n", "w".repeat(900)));
    }
    fs::write(root.path().join("wide.txt"), &wide).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let mut request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha",
            "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let mut shown_rows = std::collections::HashSet::new();
    let mut read_lines = std::collections::HashSet::new();
    let mut pages = 0;
    loop {
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("search");
        pages += 1;
        assert!(pages < 60, "pagination must terminate");
        let clipped_files = result
            .files
            .iter()
            .filter(|file| file.matches.iter().flatten().any(|row| row.clipped))
            .map(|file| file.path.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for file in &result.files {
            for row in file.matches.iter().flatten() {
                assert!(shown_rows.insert((file.path.clone(), row.line)), "row once");
            }
        }
        let expansion = page_expansion(&result);
        if clipped_files.is_empty() {
            assert!(expansion.is_none());
        } else {
            let (tool, rows) = expansion.expect("expandValues");
            assert_eq!(tool, "localFetch");
            let read_files = rows
                .iter()
                .filter_map(|row| row["path"].as_str().map(str::to_owned))
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(read_files, clipped_files, "page {pages}");
            for row in &rows {
                for range in row["ranges"].as_array().into_iter().flatten() {
                    let (start, end) = range
                        .as_str()
                        .and_then(|range| range.split_once('-'))
                        .expect("range");
                    let start: u32 = start.parse().expect("start");
                    let end: u32 = end.parse().expect("end");
                    for line in start..=end {
                        read_lines
                            .insert((row["path"].as_str().unwrap_or_default().to_owned(), line));
                    }
                }
            }
        }
        let Some(next_page) = result.next.as_ref().and_then(|next| next.get("nextPage")) else {
            break;
        };
        request =
            serde_json::from_value(next_page["query"]["queries"][0].clone()).expect("continuation");
    }
    let rows = total + limit * 10 + 15;
    assert_eq!(shown_rows.len(), rows, "every row once");
    // Every clipped row's line is read by some page's expandValues.
    for (path, line) in &shown_rows {
        assert!(read_lines.contains(&(path.clone(), *line)), "{path}:{line}");
    }
}

/// A caller-sized page whose clipped values need more reads than one call
/// holds offers every read: `expandValues`, then `expandValues2`, …, each a
/// full call, and together they read every clipped row's line.
#[test]
fn a_grid_page_reads_every_clipped_value_in_numbered_calls() {
    use crate::id::{ToolId, channel, is_enrichment};
    let limit = crate::response::result::rows_per_call(ToolId::LocalFetch);
    let root = tempfile::tempdir().expect("fixture directory");
    let files = limit * 6;
    for index in 0..files {
        let mut text = String::new();
        for hit in 0..10 {
            text.push_str(&format!(
                "{} needle{hit} {}\n",
                "x".repeat(3000),
                "x".repeat(3000)
            ));
            text.push_str(&"filler\n".repeat(15));
        }
        fs::write(root.path().join(format!("f{index:02}.txt")), text).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle",
            "regex": "literal", "pageSize": files, "matchPageSize": 10, "sort": "path"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(
        !result.warnings.iter().any(|w| w.contains("expandValues")),
        "{:?}",
        result.warnings
    );
    let clipped = result
        .files
        .iter()
        .flat_map(|file| {
            file.matches
                .iter()
                .flatten()
                .filter(|row| row.clipped)
                .map(|row| (file.path.clone(), row.line))
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(clipped.len(), files * 10, "every row clipped");
    let next = result.next.clone().unwrap_or_default();
    let mut names = next
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, _)| name.clone())
        .filter(|name| name.starts_with("expandValues"))
        .collect::<Vec<_>>();
    names.sort_by_key(|name| name["expandValues".len()..].parse::<usize>().unwrap_or(1));
    let expected = (1..=6)
        .map(|n| match n {
            1 => "expandValues".to_owned(),
            n => format!("expandValues{n}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
    let mut read = std::collections::BTreeSet::new();
    for name in &names {
        assert_eq!(channel(ToolId::LocalSearch, name), crate::id::Channel::Page);
        assert!(is_enrichment(name), "{name}");
        let lead = &next[name];
        assert_eq!(lead["tool"], "localFetch", "{lead}");
        assert!(lead.get("confidence").is_none(), "{lead}");
        let rows = lead["query"]["queries"].as_array().expect("rows");
        assert!(!rows.is_empty() && rows.len() <= limit, "{lead}");
        for row in rows {
            let path = row["path"].as_str().expect("path").to_owned();
            for range in row["ranges"].as_array().into_iter().flatten() {
                let (start, end) = range
                    .as_str()
                    .and_then(|range| range.split_once('-'))
                    .expect("range");
                let (start, end): (u32, u32) =
                    (start.parse().expect("start"), end.parse().expect("end"));
                for line in start..=end {
                    read.insert((path.clone(), line));
                }
            }
        }
    }
    let unread = clipped.difference(&read).collect::<Vec<_>>();
    assert!(
        unread.is_empty(),
        "clipped rows no read reaches: {unread:?}"
    );
}

/// A multiline page whose clipped values need a file-scoped search in one
/// file and a whole read in another still offers one executable
/// `expandValues`: one tool (localFetch from each file's first clipped
/// line, which pages the rest), every clipped file a row.
#[test]
fn mixed_value_reads_are_one_executable_expand_values_call() {
    let root = tempfile::tempdir().expect("fixture directory");
    // a.txt: one value within matchContentLength, clipped only by the
    // per-row share of a page of many rows (a whole read reaches it).
    let mut a = "alpha;\nzz\n".repeat(30);
    a.push_str(&format!("alpha {};\n", "a".repeat(45_000)));
    fs::write(root.path().join("a.txt"), &a).expect("fixture");
    // b.txt: one value past matchContentLength (a wider search reaches it).
    let b = format!("alpha\n{};\n", format!("{}\n", "b".repeat(99)).repeat(600));
    fs::write(root.path().join("b.txt"), &b).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha[^;]*;",
            "regex": "rust", "multiline": "on", "pageSize": 2,
            "matchPageSize": 40, "matchContentLength": 50_000, "sort": "path"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let clipped = result
        .files
        .iter()
        .map(|file| {
            let first = file
                .matches
                .iter()
                .flatten()
                .filter(|row| row.clipped)
                .map(|row| (row.line, row.original_chars))
                .next();
            (file.path.clone(), first)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        clipped,
        [
            ("a.txt".to_owned(), Some((61, Some(45_007)))),
            ("b.txt".to_owned(), Some((1, Some(b.len() - 1)))),
        ],
        "a.txt clips within matchContentLength, b.txt past it"
    );
    let (tool, rows) = page_expansion(&result).expect("expandValues");
    assert_eq!(tool, "localFetch", "{rows:?}");
    let to_end = format!("-{}", handoff::READ_TO_END_LINE);
    assert_eq!(
        rows,
        [
            serde_json::json!({"path": "a.txt", "ranges": [format!("61{to_end}")]}),
            serde_json::json!({"path": "b.txt", "ranges": [format!("1{to_end}")]}),
        ]
    );
    let (reads, content) = follow_expansions(&result, &policy);
    assert_eq!(reads, 2);
    assert!(
        content.contains(&format!("alpha {}", "a".repeat(100))),
        "a.txt read"
    );
    assert!(
        content.contains(&format!("alpha\n{}", "b".repeat(99))),
        "b.txt read"
    );
}
