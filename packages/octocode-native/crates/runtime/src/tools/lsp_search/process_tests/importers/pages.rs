//! Importer pages reach every reference and caller exactly once.

use super::*;

/// References past the importer cap: walking `nextPage` then
/// `nextImporterPage` reaches every importer file in exactly one window,
/// lists every row once (a recovered row in another window's file waits for
/// that window), and only importer page 1 lists the anchor's own answer.
/// A changed candidate list stales both a window's entry and its later
/// location pages, and the restart walks from the first window again.
#[test]
fn importer_pages_reach_every_reference_exactly_once() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let start = serde_json::json!({
            "operation": "references", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "pageSize": 10,
            "workspaceRoot": root.to_string_lossy()
        });
        let (pages, continuations) = walk_pages(&pool, &root, &execution, start.clone()).await;
        let windows = pages
            .iter()
            .map(|page| page.importer_page)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(windows.into_iter().collect::<Vec<_>>(), vec![1, 2, 3]);
        let mut seen = std::collections::BTreeMap::new();
        for page in &pages {
            for row in reference_rows(&page.row) {
                if row.0 == "a.ts" {
                    assert_eq!(
                        page.importer_page, 1,
                        "only importer page 1 lists the anchor"
                    );
                }
                if let Some(earlier) = seen.insert(row.clone(), page.importer_page) {
                    panic!(
                        "{row:?} listed twice (windows {earlier} and {})",
                        page.importer_page
                    );
                }
            }
        }
        let mut expected = vec![("a.ts".to_owned(), 1, 17)];
        for index in 0..IMPORTERS {
            let name = format!("f{index:03}.ts");
            expected.push((name.clone(), 1, 10));
            expected.push((name, 2, 1));
        }
        expected.sort();
        assert_eq!(
            seen.keys().cloned().collect::<Vec<_>>(),
            expected,
            "no gaps"
        );
        // Each importer file is listed by its own window: f000-f023 in 1,
        // f024-f047 in 2, f048-f059 in 3.
        for ((name, _, _), window) in &seen {
            if let Some(index) = name
                .strip_prefix('f')
                .and_then(|rest| rest.strip_suffix(".ts"))
                .and_then(|index| index.parse::<usize>().ok())
            {
                assert_eq!(*window as usize, index / 24 + 1, "{name}");
            }
        }
        // Only the last window's last page is complete; earlier windows are
        // partial, never a terminal limit.
        for page in &pages {
            assert!(page.row.get("terminalLimit").is_none(), "{}", page.row);
        }
        let last = &pages.last().expect("pages").row;
        assert!(last.get("isPartial").is_none(), "{last}");
        let first = &pages[0].row;
        assert_eq!(first["isPartial"], true, "{first}");
        assert!(
            first["partialReasons"]
                .as_array()
                .is_some_and(|reasons| reasons.contains(&"importerScanCapped".into())),
            "{first}"
        );

        // A new candidate stales the second window's entry and its later
        // location page alike; the restart has no importer cursor.
        let entry = continuations
            .iter()
            .find(|(name, _)| name == "nextImporterPage")
            .map(|(_, row)| row.clone())
            .expect("a window entry");
        let inside = continuations
            .iter()
            .find(|(name, row)| name == "nextPage" && row["importerPage"] == 2)
            .map(|(_, row)| row.clone())
            .expect("a later location page of window 2");
        write_importer(&root, "f000a.ts");
        let paths = crate::tools::test_support::workspace_policy(&root);
        for stale in [entry, inside] {
            let query = serde_json::from_value(stale).expect("query");
            let row = crate::tools::lsp_search::execute(
                query,
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
            )
            .await
            .expect("stale page");
            assert_eq!(row["errorCode"], "staleSnapshot", "{row}");
            let restart = next_row(&row, "restart").expect("restart");
            for cursor in crate::response::result::CURSOR_FIELDS {
                assert!(restart.get(cursor).is_none(), "{cursor}: {restart}");
            }
        }
        // The restart walks the changed list once more: 61 importers.
        let (pages, _) = walk_pages(&pool, &root, &execution, start).await;
        let rows = pages
            .iter()
            .flat_map(|page| reference_rows(&page.row))
            .collect::<Vec<_>>();
        let unique = rows.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(rows.len(), unique.len(), "no duplicates after restart");
        assert_eq!(rows.len(), 1 + 2 * (IMPORTERS + 1));
    });
}

/// Callers past the importer cap: each importer file's reference-derived
/// caller is listed once, by its own window, and later windows leave out
/// the anchor's walk.
#[test]
fn importer_pages_reach_every_caller_exactly_once() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let start = serde_json::json!({
            "operation": "callers", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "pageSize": 10,
            "workspaceRoot": root.to_string_lossy()
        });
        let (pages, _) = walk_pages(&pool, &root, &execution, start).await;
        let mut callers = Vec::new();
        for page in &pages {
            for file in page
                .row
                .pointer("/files")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = Path::new(file["path"].as_str().expect("path"))
                    .file_name()
                    .expect("file name")
                    .to_string_lossy()
                    .into_owned();
                for _ in file["matches"].as_array().expect("matches") {
                    callers.push((name.clone(), page.importer_page));
                }
            }
        }
        callers.sort();
        let expected = (0..IMPORTERS)
            .map(|index| (format!("f{index:03}.ts"), (index / 24 + 1) as u64))
            .collect::<Vec<_>>();
        assert_eq!(
            callers, expected,
            "every importer's caller once, in its window"
        );
    });
}
