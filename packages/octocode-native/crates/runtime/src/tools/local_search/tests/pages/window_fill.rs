//! Default pages fill the response window, adaptive path pages and context
//! windows show each row or line once.

use super::*;

#[test]
fn default_search_pages_fill_the_window_instead_of_stopping_at_an_overview() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("hot.txt"), "needle\n".repeat(1_200)).expect("fixture");
    let start = serde_json::to_value(ls_query(
        serde_json::json!({"path":root.path(),"matchString":"needle"}),
        None,
    ))
    .expect("query");
    let (walk, bodies) = walk_search(root.path(), start, None);
    let first = bodies[0]["files"][0]["matches"]
        .as_array()
        .expect("matches");
    assert!(
        first.len() > 100,
        "first page stopped before filling budget: {}",
        first.len()
    );
    // The walk delivered each row once and checked every continuation
    // against the input contract.
    assert_eq!(walk.rows.len(), 1_200);
    assert!(walk.queries.len() < 5, "{} pages", walk.queries.len());
}

/// Rows page 1 of `let_heavy_rust("handle_request", 200)` held when every
/// hit row reserved a flat 93 chars for its `enclosing` name.
const FLAT_RESERVE_FIRST_PAGE_ROWS: usize = 154;

/// A default page charges each hit row the `enclosing` name it carries (the
/// full name on a declaration's first row of the page, `enclosingLine` on
/// its later rows) instead of a flat reserve, so a page of a hot file fills
/// the window and still stays within the page budget.
#[test]
fn default_pages_fill_the_window_with_enclosing_names() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(
        root.path().join("engine.rs"),
        let_heavy_rust("handle_request", 200),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let query = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "let "}),
        None,
    );
    let budget =
        crate::tools::local_search::executor::scan_query(&query, &policy, &NeverCancel, None, None)
            .expect("scan")
            .page_budget;
    let result = execute_local_search(&query, &policy, &security, &NeverCancel, None, None, None)
        .expect("page 1");
    let rows = result.files[0].matches.as_ref().expect("rows");
    assert!(
        rows.len() * 10 >= FLAT_RESERVE_FIRST_PAGE_ROWS * 13,
        "{} rows on page 1, flat reserve held {FLAT_RESERVE_FIRST_PAGE_ROWS}",
        rows.len()
    );
    let chars = octocode_engine::json_size::utf16_units(&result.files).unwrap_or(0);
    assert!(chars <= budget, "page 1: {chars} chars > {budget}");
    assert!(
        rows.iter()
            .all(|row| row.enclosing.is_some() || row.enclosing_line.is_some()),
        "every row is named"
    );
}

#[test]
fn default_path_views_page_by_cost_and_explicit_counts_still_win() {
    let root = tempfile::tempdir().expect("fixture");
    for index in 0..300 {
        fs::write(root.path().join(format!("f{index:03}.txt")), "needle\n").expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    for view in ["files", "filesWithout", "countLines", "countMatches"] {
        let term = if view == "filesWithout" {
            "absent"
        } else {
            "needle"
        };
        let query = ls_query(
            serde_json::json!({"path":root.path(),"matchString":term,"resultView":view}),
            None,
        );
        let result =
            execute_local_search(&query, &policy, &security, &NeverCancel, None, None, None)
                .expect("default");
        assert_eq!(result.files.len(), 300, "{view}");
        assert!(result.next.is_none(), "{view}");
        let explicit = ls_query(serde_json::json!({"pageSize":7}), Some(&query));
        let result = execute_local_search(
            &explicit,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect("explicit");
        assert_eq!(result.files.len(), 7, "{view}");
        assert!(result.next.is_some(), "{view}");
    }
}

#[test]
fn adaptive_path_pages_copy_their_query_and_cover_each_count_once() {
    let root = tempfile::tempdir().expect("fixture");
    for index in 0..300 {
        fs::write(
            root.path().join(format!("f{index:03}.txt")),
            "needle\nneedle\n",
        )
        .expect("fixture");
    }
    let start = serde_json::to_value(ls_query(
        serde_json::json!({"path":root.path(),"matchString":"needle","resultView":"countLines"}),
        None,
    ))
    .expect("query");
    let (walk, bodies) = walk_search(root.path(), start, Some(6_000));
    for body in &bodies {
        for file in body["files"].as_array().expect("files") {
            assert_eq!(file["matchedLineCount"], 2);
        }
        if let Some(next) = body["next"].get("nextPage") {
            assert!(next["query"]["queries"][0].get("pageSize").is_none());
        }
    }
    // Each path once, every continuation within the input contract.
    assert_eq!(walk.rows.len(), 300);
    assert!(bodies.len() > 1);
}

/// Many small files fill one response window instead of pages of a fixed
/// file count, and every row on that page still names its enclosing
/// declaration.
#[test]
fn small_files_fill_the_response_window_with_every_row_named() {
    let root = tempfile::tempdir().expect("fixture");
    for i in 0..40 {
        fs::write(
            root.path().join(format!("file{i:02}.ts")),
            format!("export function f{i}() {{\n  return \"pageMarker\";\n}}\n"),
        )
        .expect("fixture");
    }
    let start =
        serde_json::json!({"path": root.path().to_string_lossy(), "matchString": "pageMarker"});
    let (walk, bodies) = walk_search(root.path(), start, Some(20_000));
    assert_eq!(walk.rows.len(), 40, "every file once");
    assert_eq!(
        bodies.len(),
        1,
        "one page holds all 40 files: {:?}",
        bodies[0]["pagination"]
    );
    let files = bodies[0]["files"].as_array().expect("files");
    assert_eq!(files.len(), 40);
    for file in files {
        let number = file["path"].as_str().expect("path")[4..6]
            .parse::<u32>()
            .expect("number");
        assert_eq!(
            owner_of(file, 0).map(|owner| owner["symbolName"].clone()),
            Some(serde_json::json!(format!("f{number}"))),
            "{file}"
        );
    }
}

/// With context, a file whose hits are split across pages (by rank or by
/// the page budget) shows each hit line and each context line once across
/// the whole chain: a window never repeats lines another page's row owns.
#[test]
fn context_windows_show_every_line_once_across_pages() {
    let root = tempfile::tempdir().expect("fixture");
    let mut body = String::new();
    let mut hits = Vec::new();
    for line in 1..=400u32 {
        // Irregular spacing, mixed ranks: declarations, code, comments.
        let text = match line % 11 {
            0 => format!("fn needle_{line}() {{}}"),
            3 | 5 => format!("    let x = needle({line});"),
            7 => format!("    // needle {line}"),
            _ => format!("    other({line});"),
        };
        if text.contains("needle") {
            hits.push(line);
        }
        body.push_str(&text);
        body.push('\n');
    }
    fs::write(root.path().join("a.rs"), &body).expect("a");
    fs::write(root.path().join("b.rs"), &body).expect("b");
    for (before, after) in [(3, 3), (4, 0), (0, 4), (1, 5)] {
        let start = serde_json::json!({"path": root.path().to_string_lossy(),
        "matchString": "needle", "contextBefore": before, "contextAfter": after});
        let (_, bodies) = walk_search(root.path(), start, Some(4_000));
        assert!(bodies.len() > 2, "the chain pages: {}", bodies.len());
        let mut seen = std::collections::BTreeMap::<(String, u32), usize>::new();
        for page in &bodies {
            for file in page["files"].as_array().into_iter().flatten() {
                let path = file["path"].as_str().expect("path").to_owned();
                for row in file["matches"].as_array().into_iter().flatten() {
                    for text in row["value"].as_str().expect("value").split('\n') {
                        let number = text
                            .split('\t')
                            .next()
                            .and_then(|n| n.parse::<u32>().ok())
                            .unwrap_or_else(|| panic!("numbered line: {text:?}"));
                        *seen.entry((path.clone(), number)).or_default() += 1;
                    }
                }
            }
        }
        let repeated = seen
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        assert!(repeated.is_empty(), "lines shown twice: {repeated:?}");
        for path in ["a.rs", "b.rs"] {
            for &hit in &hits {
                assert!(
                    seen.contains_key(&(path.to_owned(), hit)),
                    "{path}:{hit} never shown"
                );
            }
        }
        let expected: std::collections::BTreeSet<_> = hits
            .iter()
            .flat_map(|&hit| hit.saturating_sub(before).max(1)..=hit.saturating_add(after).min(400))
            .collect();
        for path in ["a.rs", "b.rs"] {
            let actual = seen
                .keys()
                .filter(|(p, _)| p == path)
                .map(|(_, line)| *line)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(actual, expected, "{path} context {before}/{after}");
        }
    }
}

#[test]
fn asymmetric_overrides_and_equivalent_queries_preserve_evidence() {
    let (root, policy, security) = context_fixture();
    let run = |extra: serde_json::Value| {
        let base = ls_query(
            serde_json::json!({"path": root.path(), "matchString":"needle"}),
            None,
        );
        let q = ls_query(extra, Some(&base));
        let key = super::super::cursor::query_key(&q, root.path(), 4_000);
        let out = execute_local_search(&q, &policy, &security, &NeverCancel, None, None, None)
            .expect("search");
        (key, serde_json::to_value(out).expect("json"))
    };
    for (fields, expected) in [
        (
            serde_json::json!({"contextLines":3,"contextBefore":1,"contextAfter":0}),
            vec![4, 5],
        ),
        (serde_json::json!({"contextAfter":2}), vec![5, 6, 7]),
        (
            serde_json::json!({"contextBefore":100}),
            vec![1, 2, 3, 4, 5],
        ),
    ] {
        let (_, result) = run(fields);
        let actual = result["files"][0]["matches"][0]["value"]
            .as_str()
            .expect("value")
            .lines()
            .map(|line| {
                line.split_once('\t')
                    .expect("numbered")
                    .0
                    .parse::<u32>()
                    .expect("number")
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{result}");
    }
    let symmetric = run(serde_json::json!({"contextLines":2}));
    assert_eq!(
        symmetric,
        run(serde_json::json!({"contextBefore":2,"contextAfter":2}))
    );
    assert_eq!(
        run(serde_json::json!({})),
        run(serde_json::json!({"contextLines":3,"contextBefore":0,"contextAfter":0}))
    );
    assert_ne!(
        run(serde_json::json!({"contextBefore":2})).0,
        run(serde_json::json!({"contextAfter":2})).0
    );
    for view in ["matchOnly", "countLines", "countMatches", "files"] {
        let plain = run(serde_json::json!({"resultView":view})).1;
        let context =
            run(serde_json::json!({"resultView":view,"contextBefore":2,"contextAfter":0})).1;
        assert_eq!(plain["files"], context["files"], "{view}");
    }
}
