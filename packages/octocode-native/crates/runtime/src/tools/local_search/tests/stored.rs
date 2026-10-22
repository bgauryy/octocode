//! Stored scans and continuation pages: a later page is cut from the
//! scan its first page stored, equals the page a rescan cuts, and restarts
//! when the walk, a shown file or the snapshot no longer holds.
use super::*;
use octocode_contract_types::ErrorCode;

/// Swap `path` for a symlink to a copy outside the allowed root with the
/// same bytes and modification time: size, time and digest all match the
/// stored scan, and only the read policy sees the change.
#[cfg(unix)]
fn swap_for_outside_copy(path: &std::path::Path, outside: &std::path::Path) {
    let copy = outside.join(path.file_name().expect("file name"));
    fs::copy(path, &copy).expect("copy outside");
    let modified = fs::metadata(path)
        .and_then(|m| m.modified())
        .expect("mtime");
    fs::File::options()
        .write(true)
        .open(&copy)
        .and_then(|file| file.set_modified(modified))
        .expect("restore mtime");
    fs::remove_file(path).expect("remove");
    std::os::unix::fs::symlink(&copy, path).expect("symlink");
}

/// The files a page of the stored scan shows (`path` list).
fn page_paths(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> Vec<String> {
    execute_local_search(
        request,
        policy,
        security,
        &NeverCancel,
        None,
        Some(6_000),
        None,
    )
    .expect("page")
    .files
    .into_iter()
    .map(|file| file.path)
    .collect()
}

/// Thirty TS files of needle hits (every third also holds a token on a hit
/// line), a key file whose hit sits inside a private-key block, and an
/// innocent base64 hit: enough rows for several pages, with values that
/// redaction replaces and files whose key-block scans read their bytes.
fn paged_fixture(root: &std::path::Path) {
    for file in 0..30 {
        let mut body = (0..15)
            .map(|n| {
                format!(
                    "export function compute_{file}_{n}(value: number): number {{\n  const needle_{n} = value * {n};\n  return needle_{n} + {file};\n}}\n"
                )
            })
            .collect::<String>();
        if file % 3 == 0 {
            body.push_str(&format!(
                "const needle_token = \"ghp_{file:02}ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567\";\n"
            ));
        }
        fs::write(root.join(format!("m{file:02}.ts")), body).expect("fixture");
    }
    fs::write(
        root.join("zkey.ts"),
        "export const pem = `\n-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAneedleKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n`;\n",
    )
    .expect("key fixture");
    fs::write(
        root.join("zbase64.ts"),
        "export const blob = `\nneedleAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n`;\n",
    )
    .expect("base64 fixture");
}

/// The file-page number and snapshot travel once, in `next.*.query`.
#[test]
fn paged_results_keep_cursor_fields_only_in_their_continuations() {
    let body = search_fixture(
        &[("a.txt", "foo\nfoo\nfoo\n"), ("b.txt", "foo\n")],
        ls_query(
            serde_json::json!({"matchString": "foo", "pageSize": 1, "matchPageSize": 1, "contextLines": 0}),
            None,
        ),
    );
    let pagination = &body["pagination"];
    assert_eq!(pagination["totalPages"], 2, "{body}");
    assert!(pagination.get("snapshot").is_none(), "{body}");
    assert!(pagination.get("nextPage").is_none(), "{body}");
    // Rows left in a.txt and a later file: both routes, each a full row.
    assert_eq!(
        body["next"]["nextPage"]["query"]["queries"][0]["page"], 2,
        "{body}"
    );
    assert_eq!(
        body["next"]["nextPage"]["query"]["queries"][0]["matchPage"], 1,
        "{body}"
    );
    assert_eq!(
        body["next"]["nextMatchPage"]["query"]["queries"][0]["matchPage"], 2,
        "{body}"
    );
    let last_match_page = search_fixture(
        &[("a.txt", "foo\nfoo\nfoo\n"), ("b.txt", "foo\n")],
        ls_query(
            serde_json::json!({"matchString": "foo", "pageSize": 1, "matchPage": 3, "matchPageSize": 1, "contextLines": 0}),
            None,
        ),
    );
    assert_eq!(
        last_match_page["next"]["nextPage"]["query"]["queries"][0]["page"], 2,
        "{last_match_page}"
    );
    assert!(
        last_match_page["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string(),
        "{last_match_page}"
    );
    let file = &body["files"][0]["pagination"];
    assert_eq!(file["hasMore"], true, "{body}");
    assert!(file.get("nextMatchPage").is_none(), "{body}");
    assert_eq!(
        body["next"]["nextMatchPage"]["query"]["queries"][0]["matchPage"], 2,
        "{body}"
    );
}

/// An absolute-path search walked through continuations that name the
/// root relative to the workspace (as the response envelope spells them)
/// reaches every row exactly once: the snapshot and page cuts bind the
/// canonical root, not its spelling.
#[test]
fn a_workspace_relative_replay_of_an_absolute_search_walks_every_page_once() {
    let root = tempfile::tempdir().expect("fixture directory");
    let canonical = fs::canonicalize(root.path()).expect("canonical");
    let body = (1..=400)
        .map(|n| format!("function f{n}() {{ return {n}; }}\n"))
        .collect::<String>();
    fs::write(canonical.join("checker.ts"), &body).expect("fixture");
    fs::write(canonical.join("other.ts"), "function other() {}\n").expect("fixture");
    let (policy, security) = policy_for(&canonical);
    for (path, relative) in [
        (canonical.join("checker.ts"), "checker.ts"),
        (canonical.clone(), "."),
    ] {
        let start = serde_json::to_value(ls_query(
            serde_json::json!({"path": path.to_string_lossy().into_owned(), "matchString": "function ", "matchPageSize": 100}),
            None,
        ))
        .expect("query");
        // Every continuation replays with the workspace-relative path.
        let mut calls = 0;
        let walk = crate::tools::test_support::checks::walk(
            "localSearch",
            serde_json::json!({"queries": [start]}),
            |call| {
                let mut row = call["queries"][0].clone();
                if calls > 0 {
                    row["path"] = serde_json::json!(relative);
                }
                calls += 1;
                let request: LocalSearchQuery =
                    serde_json::from_value(row.clone()).expect("continuation query");
                let result = execute_local_search(
                    &request,
                    &policy,
                    &security,
                    &NeverCancel,
                    None,
                    Some(20_000),
                    None,
                )
                .unwrap_or_else(|error| panic!("{row} of {relative}: {error:?}"));
                let body = serde_json::to_value(&result).expect("result");
                crate::tools::test_support::checks::Page {
                    output: crate::tools::test_support::published(
                        crate::id::ToolId::LocalSearch,
                        &row,
                        &body,
                    ),
                    rows: row_ids(&body),
                }
            },
        );
        assert!(walk.queries.len() > 1, "{relative}: expected several pages");
        let expected = if relative == "." { 401 } else { 400 };
        assert_eq!(walk.rows.len(), expected, "{relative}");
    }
}

/// A later page served from the stored scan (whose digests the search took
/// in the read it searched with, whose snapshot it derives from the stored
/// query key, and whose redaction it replays) is byte-identical to the same
/// page cut from a fresh rescan, enclosing names and redacted values
/// included; so is a repeated page 1.
#[test]
fn a_stored_page_is_byte_identical_to_the_page_a_rescan_cuts() {
    let root = tempfile::tempdir().expect("fixture directory");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let search = |request: &LocalSearchQuery| {
        execute_local_search(
            request,
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(6_000),
            None,
        )
        .expect("search")
    };
    let run = |request: &LocalSearchQuery| {
        let result = search(request);
        format!(
            "{}\n{:?}",
            serde_json::to_string(&result).expect("serialize"),
            result.source_snapshot
        )
    };
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let first = run(&request);
    assert_eq!(first, run(&request), "page 1 is deterministic");
    let page_one = search(&request);
    let snapshot = page_one.source_snapshot.clone().expect("snapshot");
    let pages = page_one.pagination.as_ref().expect("paged").total_pages;
    assert!(pages >= 4, "{pages}");
    let stored = super::manifest::get(&snapshot, &policy.identity()).expect("stored scan");
    let record = stored
        .redaction
        .expect("the page-1 redaction is replayable");
    assert!(!record.changes.is_empty(), "{record:?}");
    assert_eq!(record.key_files.len(), 2, "{record:?}");
    let mut every_page = first;
    for page in 2..=pages {
        let next = ls_query(
            serde_json::json!({"snapshot": snapshot, "page": page}),
            Some(&request),
        );
        assert!(
            super::manifest::get(&snapshot, &policy.identity())
                .is_some_and(|stored| stored.redaction.is_some()),
            "page {page} replays the stored scan's redaction"
        );
        let stored = run(&next);
        assert!(stored.contains("\"enclosing\""), "{stored}");
        super::manifest::evict(&snapshot);
        let rescanned = run(&next);
        assert_eq!(stored, rescanned, "page {page}");
        every_page.push_str(&stored);
    }
    assert!(every_page.contains("zkey.ts") && every_page.contains("zbase64.ts"));
    assert!(!every_page.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ01234567"));
    assert!(!every_page.contains("KeyBody"));
    assert!(every_page.contains(&crate::security::key_fragment_placeholder()));
}

/// A matched file swapped for a symlink out of the root, under its stored
/// modification time and digest, drops the stored scan (its directory and
/// its own stamp changed). The page that would show it rescans, and the
/// rescan's walk, which follows no link, no longer finds the file: the
/// snapshot restarts.
#[cfg(unix)]
#[test]
fn a_swapped_matched_file_drops_the_stored_scan_and_restarts_its_page() {
    let root = tempfile::tempdir().expect("fixture directory");
    let outside = tempfile::tempdir().expect("outside directory");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let snapshot = execute_local_search(
        &request,
        &policy,
        &security,
        &NeverCancel,
        None,
        Some(6_000),
        None,
    )
    .expect("page 1")
    .source_snapshot
    .expect("snapshot");
    let page = |page: u32| {
        ls_query(
            serde_json::json!({"snapshot": snapshot, "page": page}),
            Some(&request),
        )
    };
    assert!(super::manifest::get(&snapshot, &policy.identity()).is_some());
    let on_three = page_paths(&page(3), &policy, &security);
    swap_for_outside_copy(&root.path().join(&on_three[0]), outside.path());
    assert!(
        super::manifest::get(&snapshot, &policy.identity()).is_none(),
        "the swap drops the stored scan"
    );
    let failed = execute_local_search(
        &page(3),
        &policy,
        &security,
        &NeverCancel,
        None,
        Some(6_000),
        None,
    )
    .expect_err("page 3 would show the swapped file");
    assert_eq!(failed.code, ErrorCode::StaleSnapshot, "{}", failed.message);
}

/// A stored page checks the read policy of every file it shows, even for a
/// swap that lands after the page took its stored scan (when the stored
/// walk's proof still held): a shown file swapped for a symlink out of the
/// root, under its stored stamp and digest, fails the page. A swapped file
/// the page neither reads nor shows leaves the page as it was; the page
/// that shows it fails.
#[cfg(unix)]
#[test]
fn a_stored_page_checks_the_read_policy_of_the_files_it_shows() {
    // One settled fixture per swap: a swap drops its own stored scan.
    let roots = [(); 3].map(|()| {
        let root = tempfile::tempdir().expect("fixture directory");
        paged_fixture(root.path());
        root
    });
    settle();
    let stored = |root: &std::path::Path| {
        let (policy, security) = policy_for(root);
        let request = ls_query(
            serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle"}),
            None,
        );
        let snapshot = execute_local_search(
            &request,
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(6_000),
            None,
        )
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
        assert!(super::manifest::get(&snapshot, &policy.identity()).is_some());
        let page = move |page: u32| {
            ls_query(
                serde_json::json!({"snapshot": snapshot, "page": page}),
                Some(&request),
            )
        };
        (policy, security, page)
    };
    let swapped_page = |root: &std::path::Path, file: &str, shown: u32| {
        let (policy, security, page) = stored(root);
        let outside = tempfile::tempdir().expect("outside directory");
        stored_page_after(&page(shown), &policy, &security, Some(6_000), || {
            swap_for_outside_copy(&root.join(file), outside.path());
        })
    };
    let (policy, security, page) = stored(roots[0].path());
    let second = serde_json::to_string(
        &execute_local_search(
            &page(2),
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(6_000),
            None,
        )
        .expect("page 2"),
    )
    .expect("json");
    let on_two = page_paths(&page(2), &policy, &security);
    let on_three = page_paths(&page(3), &policy, &security);
    let later = on_three
        .iter()
        .find(|path| !on_two.contains(path))
        .expect("a page-3 file");
    let unchanged =
        swapped_page(roots[0].path(), later, 2).expect("page 2 neither reads nor shows it");
    assert_eq!(serde_json::to_string(&unchanged).expect("json"), second);
    for (root, file, shown) in [(&roots[1], later, 3), (&roots[2], &on_two[0], 2)] {
        let denied =
            swapped_page(root.path(), file, shown).expect_err("the page shows the swapped file");
        assert_eq!(denied.code, ErrorCode::SymlinkEscape, "{}", denied.message);
        assert!(
            denied.message.contains("denied by the active path policy"),
            "{}",
            denied.message
        );
    }
}

/// A key file the stored redaction scanned, rewritten in place under its
/// stored size and time after the page took the stored scan, restarts a
/// page that does not even show it: the replay reads the key file again and
/// binds it to the stored digest, as a fresh redaction does.
#[test]
fn a_key_file_rewritten_under_its_stamp_restarts_a_replayed_page() {
    let root = tempfile::tempdir().expect("fixture directory");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let snapshot = execute_local_search(
        &request,
        &policy,
        &security,
        &NeverCancel,
        None,
        Some(6_000),
        None,
    )
    .expect("page 1")
    .source_snapshot
    .expect("snapshot");
    let page_two = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(&request),
    );
    assert!(
        !page_paths(&page_two, &policy, &security)
            .iter()
            .any(|path| path == "zkey.ts")
    );
    let key = root.path().join("zkey.ts");
    let text = fs::read_to_string(&key).expect("key fixture");
    assert!(
        super::manifest::get(&snapshot, &policy.identity())
            .is_some_and(|stored| stored.redaction.is_some())
    );
    let page = stored_page_after(&page_two, &policy, &security, Some(6_000), || {
        rewrite_keeping_stamp(&key, &text.replace("export const pem", "export const pex"));
    });
    assert_eq!(
        page.expect_err("changed key file").code,
        ErrorCode::StaleSnapshot
    );
}

/// The same rewrite made before a continuation takes the stored scan
/// restarts it too. On Unix it moved the key file's change time, so the
/// walk's proof drops the stored scan before any replay.
#[test]
fn a_key_file_rewritten_before_a_continuation_restarts_it() {
    let root = tempfile::tempdir().expect("fixture directory");
    paged_fixture(root.path());
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let run = |query: &LocalSearchQuery| {
        execute_local_search(
            query,
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(6_000),
            None,
        )
    };
    let snapshot = run(&request)
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
    let page_two = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(&request),
    );
    assert!(super::manifest::get(&snapshot, &policy.identity()).is_some());
    let key = root.path().join("zkey.ts");
    let text = fs::read_to_string(&key).expect("key fixture");
    rewrite_keeping_stamp(&key, &text.replace("export const pem", "export const pex"));
    if cfg!(unix) {
        assert!(super::manifest::get(&snapshot, &policy.identity()).is_none());
    }
    assert_eq!(
        run(&page_two).expect_err("changed key file").code,
        ErrorCode::StaleSnapshot
    );
}

/// Page 1 of `request`, its snapshot, and the page-2 query that resumes it.
fn first_page(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> (LocalSearchResult, String, LocalSearchQuery) {
    let first = execute_local_search(request, policy, security, &NeverCancel, None, None, None)
        .expect("page 1");
    let snapshot = first
        .source_snapshot
        .clone()
        .expect("continuation identity");
    let page_two = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(request),
    );
    (first, snapshot, page_two)
}

#[test]
fn selection_recovery_keeps_search_snapshot_and_rejects_source_or_window_drift() {
    let root = tempfile::tempdir().expect("fixture");
    let path = root.path().join("evidence.txt");
    fs::write(&path, "needle before\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"needle",
            "clasifyContext":{"classifiers":["Does this evidence explain the current resource lifecycle?"],"longContext":true}}),
        None,
    );
    let first = execute_local_search(
        &request,
        &policy,
        &security,
        &NeverCancel,
        None,
        Some(20_000),
        None,
    )
    .expect("unavailable classifier still provides recovery");
    assert!(first.files.is_empty());
    let snapshot = first.source_snapshot.expect("captured search identity");
    let recovery: LocalSearchQuery = serde_json::from_value(
        first.next.expect("next")["unfiltered"]["query"]["queries"][0].clone(),
    )
    .expect("executable recovery");
    assert!(recovery.clasify_context.is_none());
    assert!(recovery.selection.is_none());
    assert_eq!(
        local_search_query::snapshot(&recovery),
        Some(snapshot.as_str())
    );
    for cached in [true, false] {
        if !cached {
            super::manifest::evict(&snapshot);
        }
        let recovered = execute_local_search(
            &recovery,
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(20_000),
            None,
        )
        .expect("unchanged captured evidence is recoverable");
        assert!(values(&recovered).contains("needle before"));
        assert_eq!(
            recovered.source_snapshot.as_deref(),
            Some(snapshot.as_str())
        );
        assert_eq!(
            execute_local_search(
                &recovery,
                &policy,
                &security,
                &NeverCancel,
                None,
                Some(24_000),
                None,
            )
            .expect_err("real response-window change must restart")
            .code,
            ErrorCode::StaleSnapshot
        );
    }
    rewrite_keeping_stamp(&path, "needle after!\n");
    assert_eq!(
        execute_local_search(
            &recovery,
            &policy,
            &security,
            &NeverCancel,
            None,
            Some(20_000),
            None,
        )
        .expect_err("changed source must restart")
        .code,
        ErrorCode::StaleSnapshot
    );
}

#[test]
fn rejects_changed_and_forged_snapshots_even_on_empty_or_final_pages() {
    let root_dir = crate::tools::test_support::Fixture::new();
    let root = root_dir.0.clone();
    fs::create_dir_all(&root).expect("fixture dir");
    let source = root.join("source.txt");
    fs::write(&source, "needle one\n").expect("fixture");
    let policy = crate::tools::test_support::workspace_policy(&root);
    let security = ContentSecurity::new();
    let request = ls_query(
        serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle".to_string()}),
        None,
    );
    let initial =
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("initial search");
    let snapshot = initial.source_snapshot.expect("identity on final page");
    let mut continued = request.clone();
    continued.snapshot = Some(snapshot.parse().expect("snapshot"));
    execute_local_search(
        &continued,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("unchanged snapshot");

    fs::write(&source, "needle two\n").expect("same-size mutation");
    let stale = execute_local_search(
        &continued,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect_err("changed results must reject snapshot");
    assert_eq!(stale.code, ErrorCode::StaleSnapshot);
    assert_eq!(stale.message, crate::response::pages::STALE_SNAPSHOT_ERROR);
    let restart = stale.next.expect("restart");
    assert_eq!(restart["restart"]["tool"], "localSearch");
    // The restart is the first page: every cursor field is omitted.
    let restart_row = &restart["restart"]["query"]["queries"][0];
    for cursor in crate::response::result::CURSOR_FIELDS {
        assert!(restart_row.get(cursor).is_none(), "{cursor}: {restart_row}");
    }
    assert_eq!(restart_row["matchString"], "needle");

    let mut empty = request.clone();
    empty.match_string = MatchString::String("absent".parse().expect("matchString"));
    empty.snapshot = Some(snapshot.parse().expect("snapshot"));
    assert_eq!(
        execute_local_search(&empty, &policy, &security, &NeverCancel, None, None, None)
            .expect_err("query-scope/empty mismatch")
            .code,
        ErrorCode::StaleSnapshot
    );
    // A snapshot no search ever issued is malformed input, not a dead
    // snapshot; a well-formed one from another version or query is stale.
    for malformed in ["forged", "lexical-live-v4:nothex", "lexical-live-v4:"] {
        continued.snapshot = Some(malformed.parse().expect("snapshot"));
        let error = execute_local_search(
            &continued,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect_err("malformed snapshot");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{malformed}");
        assert!(error.message.contains("snapshot"), "{}", error.message);
        assert!(error.next.is_none(), "an input error has no restart");
    }
    let (_, digest) = snapshot.split_once(':').expect("versioned snapshot");
    for foreign in [
        format!("lexical-live-v3:{digest}"),
        format!("lexical-live-v4:{}", "0".repeat(64)),
        // A v5 snapshot of this very digest: the JSON-hashed identity.
        format!("lexical-live-v5:{digest}"),
        // A v7 snapshot of this very digest: its pages were cut with names
        // priced for a fixed first 64 files.
        format!("lexical-live-v7:{digest}"),
    ] {
        continued.snapshot = Some(foreign.parse().expect("snapshot"));
        assert_eq!(
            execute_local_search(
                &continued,
                &policy,
                &security,
                &NeverCancel,
                None,
                None,
                None
            )
            .expect_err("foreign snapshot")
            .code,
            ErrorCode::StaleSnapshot,
            "{foreign}"
        );
    }
    fs::remove_dir_all(root).expect("cleanup");
}

/// A continuation's stored scan is bound to the query that produced
/// it: changing any search-semantic field while keeping the cursor must
/// reject, whether the stored scan is still cached or was evicted.
/// A scan whose values pass 1 MiB is stored too (packed), so its later
/// pages read the stored scan instead of walking the tree again, and every
/// page served from it equals the page a rescan cuts.
#[test]
fn a_scan_over_a_mebibyte_is_stored_for_its_later_pages() {
    let root = tempfile::tempdir().expect("fixture directory");
    let line = format!("let needle = \"{}\";\n", "abcdefghij".repeat(18));
    for n in 0..400 {
        let body = (0..20)
            .map(|i| line.replace("needle", &format!("needle_{n}_{i}")))
            .collect::<String>();
        fs::write(root.path().join(format!("f{n:03}.rs")), body).expect("fixture");
    }
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "sort": "path"}),
        None,
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("first page");
    assert!(
        first.stats.matched_lines >= Some(8_000),
        "{:?}",
        first.stats
    );
    let snapshot = first.source_snapshot.clone().expect("snapshot");
    let stored = super::manifest::get(&snapshot, &policy.identity()).expect("stored scan");
    let raw = stored
        .value
        .files
        .iter()
        .flat_map(|file| &file.matches)
        .map(|hit| hit.value.len())
        .sum::<usize>();
    assert_eq!(raw, 0, "stored ordinary matches retain no snippet text");
    assert_eq!(
        stored
            .value
            .files
            .iter()
            .map(|f| f.matches.len())
            .sum::<usize>(),
        8_000
    );
    assert!(
        stored
            .value
            .files
            .iter()
            .flat_map(|f| &f.matches)
            .all(|hit| hit.deferred.is_some_and(|price| price > 0))
    );
    let next = first.next.clone().expect("continuation")["nextPage"]["query"]["queries"][0].clone();
    let page = |query: &serde_json::Value| {
        let result = execute_local_search(
            &ls_query(query.clone(), None),
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect("page");
        serde_json::to_value(&result.files).expect("files")
    };
    let from_store = page(&next);
    super::manifest::evict(&snapshot);
    assert_eq!(from_store, page(&next));
}

/// A cursor with a default spelled out (`regex:"literal"`, an off flag,
/// `defaultExcludes:true`, `contextLines:0`, the default
/// `matchContentLength`) is the same query: it keeps its snapshot and page
/// cut, from the stored scan and from a rescan, under any response window.
#[test]
fn a_spelled_out_default_keeps_the_cursor() {
    let root = tempfile::tempdir().expect("fixture directory");
    for n in 0..6 {
        fs::write(
            root.path().join(format!("f{n}.ts")),
            "export const alpha = 1;\nexport const beta = alpha;\n".repeat(20),
        )
        .expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    for window in [None, Some(6_000)] {
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha", "pageSize": 2}),
            None,
        );
        let first = execute_local_search(
            &request,
            &policy,
            &security,
            &NeverCancel,
            None,
            window,
            None,
        )
        .expect("first page");
        let next = first.next.clone().expect("continuation");
        let cursor = next["nextPage"]["query"]["queries"][0].clone();
        let expected = execute_local_search(
            &ls_query(cursor.clone(), None),
            &policy,
            &security,
            &NeverCancel,
            None,
            window,
            None,
        )
        .expect("page two");
        let shown =
            |result: &LocalSearchResult| serde_json::to_value(&result.files).expect("files");
        for spelled in [
            serde_json::json!({"regex": "literal"}),
            serde_json::json!({"includeHidden": false}),
            serde_json::json!({"includeIgnored": false}),
            serde_json::json!({"defaultExcludes": true}),
            serde_json::json!({"wholeWord": false, "invertMatch": false, "reverse": false}),
            serde_json::json!({"contextLines": 0}),
            serde_json::json!({"matchContentLength": 200}),
        ] {
            for cached in [true, false] {
                if !cached {
                    super::manifest::evict(first.source_snapshot.as_deref().expect("snapshot"));
                }
                let mut query = cursor.clone();
                query
                    .as_object_mut()
                    .expect("query")
                    .extend(spelled.as_object().expect("fields").clone());
                let page = execute_local_search(
                    &ls_query(query, None),
                    &policy,
                    &security,
                    &NeverCancel,
                    None,
                    window,
                    None,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{spelled} (cached={cached}, window={window:?}): {}",
                        error.message
                    )
                });
                assert_eq!(page.source_snapshot, expected.source_snapshot, "{spelled}");
                assert_eq!(shown(&page), shown(&expected), "{spelled}");
            }
        }
    }
}

#[test]
fn frozen_snapshot_rejects_changed_search_semantics_on_cache_hit_and_miss() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
            root.path().join("sample.ts"),
            "export function alpha(x: number): number {\n  return x + 1;\n}\nexport const b = alpha(2);\n",
        )
        .expect("fixture");
    fs::write(
        root.path().join("other.ts"),
        "import { alpha } from \"./sample\";\nexport const value = alpha(3);\n",
    )
    .expect("fixture");
    fs::write(root.path().join("notes.md"), "alpha in prose\n").expect("fixture");
    fs::create_dir(root.path().join("node_modules")).expect("dir");
    fs::write(
        root.path().join("node_modules/dep.ts"),
        "export const alpha = 1;\n",
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha", "include": ["*.ts"], "includeIgnored": true, "pageSize": 1, "matchPageSize": 1, "sort": "path"}),
        None,
    );
    let initial =
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("initial search");
    let snapshot = initial.source_snapshot.clone().expect("frozen identity");
    let continued = ls_query(
        serde_json::json!({"snapshot": snapshot, "page": 2}),
        Some(&request),
    );
    let changes = [
        (
            "matchString",
            serde_json::json!({"matchString": "nonexistent_literal_control"}),
        ),
        ("include", serde_json::json!({"include": ["*.md"]})),
        (
            "defaultExcludes",
            serde_json::json!({"defaultExcludes": false}),
        ),
        // `alpha` alone reads as literal text, so a regex mode changes it.
        ("regex", serde_json::json!({"regex": "rust"})),
        ("caseMode", serde_json::json!({"caseMode": "insensitive"})),
        ("includeHidden", serde_json::json!({"includeHidden": true})),
        ("wholeWord", serde_json::json!({"wholeWord": true})),
        // File filters that keep (`alpha` is in every file) or empty the
        // result: the key, not the collected rows, must tell them apart.
        ("fileHas", serde_json::json!({"fileHas": ["alpha"]})),
        ("fileHas", serde_json::json!({"fileHas": ["ZQZQZ"]})),
        ("fileLacks", serde_json::json!({"fileLacks": ["zz_absent"]})),
        ("fileLacks", serde_json::json!({"fileLacks": ["alpha"]})),
        (
            "path",
            serde_json::json!({"path": root.path().join("other.ts").to_string_lossy().into_owned()}),
        ),
    ];
    for cached in [true, false] {
        if !cached {
            super::manifest::evict(&snapshot);
        }
        let page_two = execute_local_search(
            &continued,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .unwrap_or_else(|e| panic!("unchanged continuation (cached={cached}): {}", e.message));
        assert_eq!(
            page_two
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["sample.ts"],
            "cached={cached}"
        );
        assert_eq!(page_two.source_snapshot.as_deref(), Some(snapshot.as_str()));
        for (field, change) in &changes {
            if !cached {
                super::manifest::evict(&snapshot);
            }
            let changed = ls_query(change.clone(), Some(&continued));
            match execute_local_search(&changed, &policy, &security, &NeverCancel, None, None, None)
            {
                Ok(result) => panic!(
                    "{field} change reused the old cursor (cached={cached}): {:?}",
                    result.files.iter().map(|f| &f.path).collect::<Vec<_>>()
                ),
                Err(error) => {
                    assert_eq!(
                        error.code,
                        ErrorCode::StaleSnapshot,
                        "{field} cached={cached}"
                    );
                    let restart = error.next.expect("restart");
                    assert!(
                        restart["restart"]["query"]["queries"][0]
                            .get("snapshot")
                            .is_none()
                    );
                }
            }
        }
    }
    let control = ls_query(
        serde_json::json!({"matchString": "nonexistent_literal_control"}),
        Some(&request),
    );
    let control =
        execute_local_search(&control, &policy, &security, &NeverCancel, None, None, None)
            .expect("control search");
    assert!(control.files.is_empty());
}

/// Continuations reuse the page-1 scan while nothing its walk searched
/// changed. A new matching file, an unmatched file that gains a match, or a
/// matched file that changed drops the stored scan, and the rescan restarts
/// the changed result: exactly what a call with no stored scan answers.
#[test]
fn continuations_reuse_their_scan_until_the_walk_changes() {
    let root = tempfile::tempdir().expect("fixture directory");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.path().join(name), "needle\n").expect("fixture");
    }
    fs::write(root.path().join("d.txt"), "hay\n").expect("fixture");
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "pageSize": 1, "sort": "path"}),
        None,
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("page 1");
    let snapshot = first
        .source_snapshot
        .clone()
        .expect("continuation identity");
    let page = |n: u32| {
        ls_query(
            serde_json::json!({"snapshot": snapshot, "page": n}),
            Some(&request),
        )
    };
    assert!(super::manifest::get(&snapshot, &policy.identity()).is_some());
    let second = execute_local_search(&page(2), &policy, &security, &NeverCancel, None, None, None)
        .expect("page 2 from the stored scan");
    assert_eq!(second.files[0].path, "b.txt");
    let stale = |page: u32, change: &str| {
        let error = execute_local_search(
            &ls_query(
                serde_json::json!({"snapshot": snapshot, "page": page}),
                Some(&request),
            ),
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect_err(change);
        assert_eq!(error.code, ErrorCode::StaleSnapshot, "{change}");
        assert!(
            super::manifest::get(&snapshot, &policy.identity()).is_none(),
            "{change}"
        );
    };
    // An unmatched file that gains a match.
    fs::write(root.path().join("d.txt"), "needle\n").expect("edit");
    stale(2, "an unmatched file gained a match");
    fs::write(root.path().join("d.txt"), "hay\n").expect("restore");
    // A new matching file.
    fs::write(root.path().join("a0.txt"), "needle\n").expect("new file");
    stale(2, "a new matching file");
    fs::remove_file(root.path().join("a0.txt")).expect("cleanup");
    // A matched file that changed: page 1 stores the restored walk's scan
    // again once its writes settled.
    settle();
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("page 1 again");
    assert_eq!(first.source_snapshot.as_deref(), Some(snapshot.as_str()));
    assert!(super::manifest::get(&snapshot, &policy.identity()).is_some());
    fs::write(root.path().join("c.txt"), "needle\nneedle\n").expect("edit");
    stale(3, "an edited matched file");
}

/// A continuation never serves stored values from a file edited since
/// the scan, even when the edit keeps its size and modification time: one
/// made after the page took the stored scan fails the stored digest the
/// page re-hashes, one made before it drops the stored scan (on Unix its
/// change time moved the walk's proof) and the rescan restarts.
#[test]
fn a_same_size_same_mtime_edit_restarts_instead_of_serving_old_values() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.ts"), "const needle = \"OLD_A\";\n").expect("fixture");
    let b = root.path().join("b.ts");
    fs::write(&b, "const needle = \"OLD_B\";\n").expect("fixture");
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "regex": "literal", "sort": "path", "include": ["*.ts"], "includeIgnored": true, "pageSize": 1}),
        None,
    );
    let (_, snapshot, page_two) = first_page(&request, &policy, &security);
    assert!(super::manifest::get(&snapshot, &policy.identity()).is_some());
    let restarts = |page: Result<LocalSearchResult, crate::response::result::ToolError>,
                    case: &str| match page {
        Ok(result) => panic!("{case}: stale page served: {}", values(&result)),
        Err(error) => {
            assert_eq!(error.code, ErrorCode::StaleSnapshot, "{case}");
            let restart = error.next.expect("restart");
            assert!(
                restart["restart"]["query"]["queries"][0]
                    .get("snapshot")
                    .is_none(),
                "{case}"
            );
        }
    };
    restarts(
        stored_page_after(&page_two, &policy, &security, None, || {
            rewrite_keeping_stamp(&b, "const needle = \"NEW_B\";\n");
        }),
        "after the page took the stored scan",
    );
    if cfg!(unix) {
        assert!(super::manifest::get(&snapshot, &policy.identity()).is_none());
    }
    restarts(
        execute_local_search(
            &page_two,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        ),
        "before the continuation",
    );
    for view in ["files", "countMatches"] {
        fs::write(&b, "const needle = \"OLD_B\";\n").expect("fixture");
        settle();
        let listed = ls_query(serde_json::json!({"resultView": view}), Some(&request));
        let (_, snapshot, next) = first_page(&listed, &policy, &security);
        assert!(
            super::manifest::get(&snapshot, &policy.identity()).is_some(),
            "{view}"
        );
        let edit = || rewrite_keeping_stamp(&b, "const nEEdle = \"NEW_B\";\n");
        match stored_page_after(&next, &policy, &security, None, edit) {
            Ok(result) => panic!("{view}: stale page served: {}", values(&result)),
            Err(error) => assert_eq!(error.code, ErrorCode::StaleSnapshot, "{view}"),
        }
        match execute_local_search(&next, &policy, &security, &NeverCancel, None, None, None) {
            Ok(result) => panic!("{view}: stale page served: {}", values(&result)),
            Err(error) => assert_eq!(error.code, ErrorCode::StaleSnapshot, "{view}"),
        }
    }
    fs::write(&b, "const needle = \"NEW_B\";\n").expect("fixture");
    let fresh = execute_local_search(
        &ls_query(serde_json::json!({"pageSize": 10}), Some(&request)),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("fresh search");
    assert!(values(&fresh).contains("NEW_B"), "{}", values(&fresh));
}

/// Every view's values are present in the bytes they were scanned from,
/// so unchanged files keep reusing the stored scan; a matching file added
/// after page 1 drops it, and page 2 restarts instead of hiding the file.
#[test]
fn every_view_reuses_its_scan_until_a_matching_file_is_added() {
    let long = format!("{} needle tail {}", "x".repeat(300), "y".repeat(300));
    let views = [
        serde_json::json!({}),
        serde_json::json!({"contextLines": 2}),
        serde_json::json!({"matchContentLength": 20}),
        serde_json::json!({"resultView": "paginated", "contextLines": 3}),
        serde_json::json!({"resultView": "matchOnly", "matchContentLength": 5}),
        serde_json::json!({"resultView": "matchOnly", "unique": "count", "matchString": "needle[ a-z]*", "regex": "rust"}),
        serde_json::json!({"resultView": "files"}),
        serde_json::json!({"resultView": "countMatches"}),
        serde_json::json!({"multiline": "on", "matchString": "needle\\nmid", "regex": "rust"}),
    ];
    // One settled fixture per view: an edit in one leaves the others' walks
    // unchanged.
    let roots = views
        .iter()
        .map(|_| {
            let root = tempfile::tempdir().expect("fixture directory");
            for name in ["a.txt", "b.txt", "c.txt"] {
                fs::write(
                    root.path().join(name),
                    format!("head\r\nlet needle = 1;\r\n{long}\nneedle\nmid ✓ line\nneedle again ✓\nend\n"),
                )
                .expect("fixture");
            }
            root
        })
        .collect::<Vec<_>>();
    settle();
    for (view, root) in views.iter().zip(&roots) {
        let (policy, security) = policy_for(root.path());
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "pageSize": 1, "sort": "path"}),
            None,
        );
        let request = ls_query(view.clone(), Some(&request));
        let first =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .unwrap_or_else(|e| panic!("{view}: {}", e.message));
        let snapshot = first
            .source_snapshot
            .clone()
            .unwrap_or_else(|| panic!("{view}: no continuation"));
        let page_two = ls_query(
            serde_json::json!({"snapshot": snapshot, "page": 2}),
            Some(&request),
        );
        assert!(
            super::manifest::get(&snapshot, &policy.identity()).is_some(),
            "{view}: stored"
        );
        let second = execute_local_search(
            &page_two,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .unwrap_or_else(|e| panic!("{view}: stored scan not reused: {}", e.message));
        assert_eq!(second.files[0].path, "b.txt", "{view}");
        fs::write(root.path().join("a0.txt"), "needle\n").expect("new file");
        let error = execute_local_search(
            &page_two,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect_err("a new matching file restarts page 2");
        assert_eq!(error.code, ErrorCode::StaleSnapshot, "{view}");
    }
}

/// A stored key-shaped value is redacted on its first page; after the
/// source is swapped for innocuous text of the same size and time, the
/// stored value must not be checked against the new text and shown, whether
/// the swap lands after the page took the stored scan or before.
#[test]
fn a_stored_key_fragment_never_surfaces_after_a_stamp_preserving_swap() {
    let root = tempfile::tempdir().expect("fixture directory");
    let body = "QUJD".repeat(24);
    fs::write(
        root.path().join("a.ts"),
        "// QUJD visible harmless marker\n",
    )
    .expect("fixture");
    let key = root.path().join("b.ts");
    let old = format!("-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----\n");
    fs::write(&key, &old).expect("fixture");
    settle();
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "QUJD", "regex": "literal", "sort": "path", "include": ["*.ts"], "includeIgnored": true, "pageSize": 1}),
        None,
    );
    let (_, snapshot, page_two) = first_page(&request, &policy, &security);
    assert!(super::manifest::get(&snapshot, &policy.identity()).is_some());
    let before = execute_local_search(
        &page_two,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("page 2 from the stored scan");
    assert!(!values(&before).contains(&body), "{}", values(&before));
    assert!(values(&before).contains("REDACTED"), "{}", values(&before));
    let clean = format!(
        "// clean source replacement\n{}\n{}\n",
        "Z".repeat(body.len()),
        "// clean trailer".to_owned() + &" ".repeat("-----END PRIVATE KEY-----".len() - 16)
    );
    // Swapped after the page took the stored scan: the stored key file no
    // longer hashes to its scan, so the page restarts.
    let swapped = stored_page_after(&page_two, &policy, &security, None, || {
        rewrite_keeping_stamp(&key, &clean);
    });
    match swapped {
        Ok(result) => panic!("old key body served: {}", values(&result)),
        Err(error) => assert_eq!(error.code, ErrorCode::StaleSnapshot, "{}", error.message),
    }
    // Swapped before: the walk's proof drops the stored scan (on Unix its
    // change time moved), and the rescan of the clean text restarts.
    if cfg!(unix) {
        assert!(super::manifest::get(&snapshot, &policy.identity()).is_none());
    }
    match execute_local_search(
        &page_two,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    ) {
        Ok(result) => panic!("old key body served: {}", values(&result)),
        Err(error) => assert_eq!(error.code, ErrorCode::StaleSnapshot, "{}", error.message),
    }
}

/// Page cuts are a pure function of the collected files and their outlines:
/// page k asked for directly shows the rows the walk's page k showed, the
/// walk shows every hit row once, and every page stays within its budget,
/// past the files the cut outlines too.
#[test]
fn pages_are_identical_across_calls() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(
        root.path().join("engine.rs"),
        let_heavy_rust("handle_request", 120),
    )
    .expect("fixture");
    for n in 0..70 {
        fs::write(
            root.path().join(format!("module_{n:02}.rs")),
            let_heavy_rust(&format!("module_{n:02}_handler"), 2),
        )
        .expect("fixture");
    }
    let fields = serde_json::json!({"path": root.path(), "matchString": "let "});
    let start = serde_json::to_value(ls_query(fields.clone(), None)).expect("query");
    let (walk, bodies) = walk_search(root.path(), start, None);
    assert_eq!(walk.rows.len(), 120 * 5 + 70 * 2 * 5);
    assert!(bodies.len() > 2, "{} pages", bodies.len());
    let (policy, security) = policy_for(root.path());
    for (index, walked) in bodies.iter().enumerate() {
        let page = ls_query(
            serde_json::json!({"page": index + 1}),
            Some(&ls_query(fields.clone(), None)),
        );
        let budget = crate::tools::local_search::executor::scan_query(
            &page,
            &policy,
            &NeverCancel,
            None,
            None,
        )
        .expect("scan")
        .page_budget;
        let direct =
            execute_local_search(&page, &policy, &security, &NeverCancel, None, None, None)
                .expect("direct page");
        let direct = serde_json::to_value(&direct).expect("result");
        assert_eq!(direct["files"], walked["files"], "page {}", index + 1);
        let chars = octocode_engine::json_size::utf16_units(&walked["files"]).unwrap_or(0);
        assert!(chars <= budget, "page {}: {chars} > {budget}", index + 1);
    }
}

/// A page's cut prices the names of earlier pages' files from their current
/// outlines, so a stamp-preserving edit to a file that only an earlier page
/// shows restarts the walk instead of cutting other pages.
#[test]
fn an_edit_to_an_earlier_pages_file_restarts_the_walk() {
    let root = tempfile::tempdir().expect("fixture");
    let small = root.path().join("small.rs");
    fs::write(&small, let_heavy_rust("alpha", 1)).expect("fixture");
    fs::write(
        root.path().join("engine.rs"),
        let_heavy_rust("handle_request", 200),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "let "}),
        None,
    );
    let (first, _, page_two) = first_page(&request, &policy, &security);
    assert!(first.files.iter().any(|file| file.path == "small.rs"));
    let served = execute_local_search(
        &page_two,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("page 2");
    assert!(served.files.iter().all(|file| file.path != "small.rs"));
    rewrite_keeping_stamp(&small, &let_heavy_rust("omega", 1));
    match execute_local_search(
        &page_two,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    ) {
        Ok(result) => panic!("page cut from changed outlines: {}", values(&result)),
        Err(error) => assert_eq!(error.code, ErrorCode::StaleSnapshot),
    }
}

#[test]
fn rescanned_pages_reject_changed_enclosing_names() {
    for evict_before_edit in [false, true] {
        let root = tempfile::tempdir().expect("fixture");
        let source = root.path().join("engine.rs");
        fs::write(&source, let_heavy_rust(&"long_name_".repeat(12), 240)).expect("fixture");
        let (policy, security) = policy_for(root.path());
        let request = ls_query(
            serde_json::json!({"path": root.path(), "matchString": "let "}),
            None,
        );
        let (first, snapshot, page_two) = first_page(&request, &policy, &security);
        assert!(
            first
                .next
                .as_ref()
                .is_some_and(|next| next.get("nextPage").is_some())
        );
        super::super::manifest::evict(&snapshot);
        let unchanged = execute_local_search(
            &page_two,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect("an unchanged rescan keeps its snapshot");
        assert!(!unchanged.files.is_empty());
        if evict_before_edit {
            super::super::manifest::evict(&snapshot);
        }
        fs::write(&source, let_heavy_rust("f", 240)).expect("rename enclosing functions");
        let changed = execute_local_search(
            &page_two,
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        );
        match changed {
            Ok(result) => panic!("accepted changed page cuts: {}", values(&result)),
            Err(error) => assert_eq!(error.code, ErrorCode::StaleSnapshot),
        }
    }
}

#[test]
fn deferred_pages_equal_owned_pages_through_the_final_duplicate() {
    let root = tempfile::tempdir().expect("fixture");
    let body = (0..123)
        .map(|i| match i % 4 {
            0 => format!("// needle comment {i}\n"),
            1 => format!("let needle_{i} = 1;\n"),
            2 => "needle();\n".to_owned(),
            _ => format!("é😀 needle {i} {}\n", "x".repeat(400)),
        })
        .collect::<String>();
    let token = format!("{}{}", "ghp_", "1234567890abcdefghijklmnopqrstuvwxyzAB");
    fs::write(
        root.path().join("dense.rs"),
        format!("{body}needle {token}\n"),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    settle();
    for fields in [
        serde_json::json!({}),
        serde_json::json!({"pageSize":1,"matchPageSize":7}),
        serde_json::json!({"contextLines":2}),
        serde_json::json!({"resultView":"matchOnly","unique":"count"}),
    ] {
        let base = ls_query(
            serde_json::json!({"path":root.path(),"matchString":"needle","matchContentLength":40}),
            None,
        );
        let mut request = ls_query(fields, Some(&base));
        let mut pages = 0;
        loop {
            let compact = execute_local_search(
                &request,
                &policy,
                &security,
                &NeverCancel,
                Some(1),
                Some(2200),
                None,
            )
            .expect("compact page");
            let plain = ls_query(serde_json::json!({"snapshot":null}), Some(&request));
            let mut original = super::super::executor::scan_query(
                &plain,
                &policy,
                &NeverCancel,
                Some(1),
                Some(2200),
            )
            .expect("scan");
            let mut options = original.search_options.clone();
            options.values = octocode_engine::types::SourceValues::Hashed;
            original.parsed = super::super::executor::scan(
                &plain,
                &policy,
                options,
                &policy.identity(),
                &NeverCancel,
            )
            .expect("owned scan")
            .parsed;
            original.reusable = None;
            let owned = super::super::executor::page_of(
                &plain,
                &policy,
                &security,
                &NeverCancel,
                Some(1),
                original,
                None,
            )
            .expect("owned page");
            assert_eq!(values(&compact), values(&owned), "page {pages}");
            assert_eq!(
                serde_json::to_value(&compact.pagination).expect("paging"),
                serde_json::to_value(&owned.pagination).expect("paging")
            );
            pages += 1;
            assert!(pages < 150);
            let next = compact
                .next
                .as_ref()
                .and_then(|next| next.get("nextPage").or_else(|| next.get("nextMatchPage")))
                .and_then(|next| next.get("query"));
            let Some(next) = next else {
                break;
            };
            request = ls_query(next["queries"][0].clone(), None);
            let cached = super::super::executor::scan_query(
                &request,
                &policy,
                &NeverCancel,
                Some(1),
                Some(2200),
            )
            .expect("cached continuation");
            assert!(cached.digests.is_some(), "continuation reused stored scan");
            assert_eq!(
                Some(cached.identity.as_str()),
                local_search_query::snapshot(&request)
            );
        }
        if request.result_view != LocalSearchQueryResultView::MatchOnly {
            assert!(pages > 1);
        }
    }
}

#[test]
#[ignore = "memory measurement: set SEARCH_MEMORY_CORPUS; optionally SEARCH_MEMORY_OWNED=1"]
fn measure_dense_search_memory() {
    let path =
        std::path::PathBuf::from(std::env::var("SEARCH_MEMORY_CORPUS").expect("corpus path"));
    let (policy, security) = policy_for(path.parent().expect("parent"));
    let request = ls_query(
        serde_json::json!({"path":path,"matchString":"needle"}),
        None,
    );
    let result = if std::env::var_os("SEARCH_MEMORY_OWNED").is_some() {
        let mut scan =
            super::super::executor::scan_query(&request, &policy, &NeverCancel, Some(1), None)
                .expect("compact scan");
        let mut options = scan.search_options.clone();
        options.values = octocode_engine::types::SourceValues::Hashed;
        // Drop the compact scan before the measured owned engine allocates.
        scan.parsed.files.clear();
        scan.parsed = super::super::executor::scan(
            &request,
            &policy,
            options,
            &policy.identity(),
            &NeverCancel,
        )
        .expect("owned scan")
        .parsed;
        scan.reusable = None;
        super::super::executor::page_of(
            &request,
            &policy,
            &security,
            &NeverCancel,
            Some(1),
            scan,
            None,
        )
        .expect("owned page")
    } else {
        execute_local_search(
            &request,
            &policy,
            &security,
            &NeverCancel,
            Some(1),
            None,
            None,
        )
        .expect("compact page")
    };
    assert_eq!(result.stats.matched_lines, Some(120_000));
    let snapshot = result.source_snapshot.as_deref().expect("snapshot");
    if std::env::var_os("SEARCH_MEMORY_OWNED").is_none() {
        assert!(
            super::manifest::get(snapshot, &policy.identity()).is_some(),
            "large scan stored"
        );
    }
    eprintln!(
        "pages: {:?}; rows: {:?}",
        result.pagination, result.stats.matched_lines
    );
}
