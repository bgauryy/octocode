use super::*;
use octocode_contract_types::ErrorCode;

#[test]
fn cancellation_during_the_walk_reports_cancelled() {
    use crate::execution::cancel::CancellationCheck;
    use std::sync::atomic::{AtomicU32, Ordering};
    struct CancelAfter(AtomicU32, u32);
    impl CancellationCheck for CancelAfter {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, Ordering::SeqCst) >= self.1 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }
    }
    let root = tempfile::tempdir().expect("fixture directory");
    for i in 0..40 {
        fs::write(root.path().join(format!("f{i:02}.txt")), "needle\n").expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle".to_string()}),
        None,
    );
    // The first check (before the walk) passes; the walk's polls cancel.
    let cancel = CancelAfter(AtomicU32::new(0), 3);
    let error = execute_local_search(&request, &policy, &security, &cancel, None, None, None)
        .expect_err("cancelled mid-walk");
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(
        cancel.0.load(Ordering::SeqCst) < 40,
        "walk polled every file"
    );
}

/// A bare word include also searches inside directories named by it.
#[test]
fn bare_word_include_searches_inside_named_dir() {
    let body = search_fixture(
        &[
            ("tools/local_fetch/a.rs", "needle\n"),
            ("tools/other/b.rs", "needle\n"),
        ],
        ls_query(
            serde_json::json!({"matchString": "needle", "include": ["local_fetch"]}),
            None,
        ),
    );
    let text = body["files"].to_string();
    assert!(text.contains("local_fetch/a.rs"), "{body}");
    assert!(!text.contains("other/b.rs"), "{body}");
}

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

/// Every `next.*` lead of a body is a call its own tool accepts.
fn assert_leads_validate(body: &serde_json::Value, label: &str) {
    for (name, lead) in body["next"].as_object().into_iter().flatten() {
        let tool = lead["tool"].as_str().expect("lead tool");
        if let Err(error) = crate::contracts::validate(tool, lead["query"].clone()) {
            panic!(
                "{label}: next.{name} is invalid: {:?}\n{lead}",
                error.issues
            );
        }
    }
}

/// Every page of a walk, from `request`: runs page 1, then each later page
/// pinned to the first page's snapshot, until the page count it reported.
fn walk_pages(files: &[(&str, &str)], request: &LocalSearchQuery) -> Vec<serde_json::Value> {
    let root = tempfile::tempdir().expect("fixture directory");
    for (name, body) in files {
        fs::write(root.path().join(name), body).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
        Some(request),
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("first page");
    let total = first.pagination.as_ref().map_or(1, |p| p.total_pages);
    let snapshot = first.source_snapshot.clone();
    let mut pages = vec![serde_json::to_value(&first).expect("serialize")];
    for page in 2..=total {
        let next = ls_query(
            serde_json::json!({"page": page, "snapshot": snapshot}),
            Some(&request),
        );
        let result =
            execute_local_search(&next, &policy, &security, &NeverCancel, None, None, None)
                .expect("later page");
        pages.push(serde_json::to_value(&result).expect("serialize"));
    }
    pages
}

/// `fileHas`/`fileLacks` filter matched files by other terms they
/// hold anywhere, not on the hit line: "uses X and Y but not Z".
#[test]
fn file_terms_keep_files_with_every_term_and_none_excluded() {
    let files = [
        ("src/a.rs", "fn handler() {}\nuse tokio;\n"),
        ("src/b.rs", "fn handler() {}\nuse tokio;\nuse async_std;\n"),
        ("src/c.rs", "fn handler() {}\n"),
    ];
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString":"handler","fileHas":["tokio"],"fileLacks":["async_std"]}),
            None,
        ),
    );
    let paths: Vec<&str> = body["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();
    assert_eq!(paths, ["src/a.rs"], "{body}");
}

/// The skipped-path disclosure names why files were skipped: the deciding ignore
/// file, line and pattern (`gen/.gitignore:2 '*'`), or `hidden`.
#[test]
fn an_empty_search_names_the_rule_that_skipped_its_matches() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "logs/\n").expect("gitignore");
    for dir in ["logs", "gen", ".notes"] {
        fs::create_dir_all(root.path().join(dir)).expect("dir");
    }
    fs::write(root.path().join("gen/.gitignore"), "# generated\n*\n").expect("nested");
    fs::write(root.path().join("logs/log.txt"), "needle-here\n").expect("fixture");
    fs::write(root.path().join("gen/api.ts"), "needle-here\n").expect("fixture");
    fs::write(root.path().join(".notes/x.txt"), "needle-here\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle-here"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let hint = &result.hints[0];
    assert!(
        hint.starts_with("3 files that match behind skipped paths"),
        "{hint}"
    );
    let body = serde_json::to_value(&result).expect("result");
    let entries = body["skippedPaths"]["entries"].as_array().expect("skips");
    for (path, cause, reason) in [
        ("logs", "ignored", ".gitignore:1 'logs/'"),
        ("gen/api.ts", "ignored", "gen/.gitignore:2 '*'"),
        (".notes", "hidden", "hidden .notes/"),
    ] {
        assert!(
            entries.iter().any(
                |entry| entry["path"] == skipped_source_path(root.path(), path)
                    && entry["cause"] == cause
                    && entry["reason"] == reason
            ),
            "{path}: {body}"
        );
    }
    assert!(hint.len() <= 120, "{hint}");
}

/// Enclosing names fill only the room the rendered page leaves: rows render
/// with the root's workspace-relative prefix, which the page cut charges, so
/// the names must too, or a deep root overflows the response window.
#[test]
fn enclosing_names_leave_room_for_the_rendered_path_prefix() {
    let workspace = tempfile::tempdir().expect("fixture directory");
    let deep = format!(
        "{}/src",
        ["a-rather-long-directory-name-for-paths"; 5].join("/")
    );
    let root = workspace.path().join(&deep);
    fs::create_dir_all(&root).expect("deep root");
    for index in 0..40 {
        fs::write(
            root.join(format!("module_{index:02}.rs")),
            format!("fn handler_{index:02}_with_a_long_name() {{\n    needle();\n}}\n\nfn other_{index:02}() {{\n    needle();\n}}\n"),
        )
        .expect("fixture");
    }
    // The row reserve exceeds half this window, so the page budget is its
    // floor: half the window.
    let window = 4_000;
    let budget = window / 2;
    let prefix = deep.len() + 1;
    let start = serde_json::to_value(ls_query(
        serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    ))
    .expect("query");
    let (walk, bodies) = walk_search(workspace.path(), start, Some(window));
    for (index, body) in bodies.iter().enumerate() {
        let files = serde_json::to_string(&body["files"]).expect("files").len();
        let rendered = files + prefix * body["files"].as_array().map_or(0, Vec::len);
        assert!(
            rendered <= budget,
            "page {}: {rendered} > {budget}",
            index + 1
        );
    }
    assert_eq!(walk.rows.len(), 80);
}

/// A hit in a JSON or TOML file names its key path, one
/// `matchKeys` map per file row; other files carry none.
#[test]
fn json_and_toml_hits_name_their_key_paths() {
    let package = "{\n  \"scripts\": {\n    \"typescript\": \"tsc --noEmit\"\n  },\n  \"devDependencies\": {\n    \"typescript\": \"6.0.2\"\n  }\n}\n";
    let cargo = "[package]\nname = \"demo\"\n\n[workspace]\nmembers = [\n    \"typescript-x\",\n]\n\n[dependencies]\ntypescript = \"1\"\n";
    let body = search_fixture(
        &[
            ("package.json", package),
            ("Cargo.toml", cargo),
            ("notes.md", "typescript\n"),
        ],
        ls_query(
            serde_json::json!({"matchString": "typescript", "sort": "path"}),
            None,
        ),
    );
    let file = |name: &str| {
        body["files"]
            .as_array()
            .and_then(|files| files.iter().find(|file| file["path"] == name))
            .unwrap_or_else(|| panic!("{name}: {body}"))
            .clone()
    };
    assert_eq!(
        file("package.json")["matchKeys"],
        serde_json::json!({"3": "scripts.typescript", "6": "devDependencies.typescript"}),
        "{body}"
    );
    assert_eq!(
        file("Cargo.toml")["matchKeys"],
        serde_json::json!({"6": "workspace.members[0]", "10": "dependencies.typescript"}),
        "{body}"
    );
    assert!(file("notes.md").get("matchKeys").is_none(), "{body}");
}

/// The over-1 MiB note counts only files whose language has
/// declarations (a lockfile never had any), and says `1 hit row`.
#[test]
fn oversized_enclosing_note_skips_files_without_declarations() {
    let filler = "// padding line for an oversized source file\n".repeat(25_000);
    let body = search_fixture(
        &[
            ("uv.lock", &format!("needle = 1\n{filler}")),
            ("big.rs", &format!("fn f() {{ needle(); }}\n{filler}")),
        ],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    let warnings = body["warnings"].to_string();
    assert!(
        warnings.contains("are not named in files over 1 MiB; 1 hit row there has none."),
        "{body}"
    );
    let lock_only = search_fixture(
        &[("uv.lock", &format!("needle = 1\n{filler}"))],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    crate::tools::test_support::checks::assert_absent(&lock_only["warnings"], "1 MiB");
}

/// Files past the collection cap are named with their count on every
/// page, never only in `stats`.
#[test]
fn files_past_the_collection_cap_are_disclosed() {
    let scanned = octocode_engine::types::TextSearchStats {
        files_matched: Some(10_851),
        capped: Some(true),
        cap_reasons: vec![octocode_engine::types::CapReason::MaxCollectedFiles],
        ..Default::default()
    };
    let stats = executor::search_stats(
        LocalSearchQueryResultView::Paginated,
        &scanned,
        executor::MAX_COLLECTED_FILES,
    );
    let warning =
        coverage::uncollected_warning(&stats, executor::MAX_COLLECTED_FILES).expect("disclosed");
    assert!(warning.contains("851 of 10851"), "{warning}");
    let uncapped = executor::search_stats(
        LocalSearchQueryResultView::Paginated,
        &octocode_engine::types::TextSearchStats {
            files_matched: Some(10),
            ..Default::default()
        },
        10,
    );
    assert!(coverage::uncollected_warning(&uncapped, 10).is_none());
}

/// A PCRE2 matching limit on a readable file is the pattern's failure,
/// not an unreadable scope, and the error names the file root-relative.
#[test]
fn a_pcre2_matching_limit_is_a_pattern_failure_not_an_unreadable_file() {
    let root = tempfile::tempdir().expect("fixture directory");
    let mut text = "a".repeat(50_000);
    text.push_str("b\n");
    fs::write(root.path().join("patho.txt"), &text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let file = root.path().join("patho.txt");
    let request = ls_query(
        serde_json::json!({"path": file.to_string_lossy().into_owned(), "matchString": "(a+)+$", "regex": "pcre2"}),
        None,
    );
    let error = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect_err("every file failed in the matcher");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    assert!(error.message.contains("first: patho.txt:"), "{error:?}");
    assert!(
        !error
            .message
            .contains(&root.path().to_string_lossy().into_owned()),
        "{error:?}"
    );
    // In a folder with a matching file the limit is disclosed per call.
    fs::write(root.path().join("ok.txt"), "aaa\n").expect("fixture");
    let folder = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "(a+)+$", "regex": "pcre2"}),
        None,
    );
    let result = execute_local_search(&folder, &policy, &security, &NeverCancel, None, None, None)
        .expect("one file searched");
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w
                .starts_with("patternLimit: the regex hit the engine's matching limit in 1 file")),
        "{:?}",
        result.warnings
    );
}

/// The scope listing a localSearch lead offers walks the same scope the
/// search did: `includeIgnored` and `defaultExcludes:false` carry over.
#[test]
fn scope_listing_keeps_the_search_walk_scope() {
    let request = ls_query(
        serde_json::json!({"path": "/repo", "matchString": "x", "includeIgnored": true, "defaultExcludes": false, "include": ["*.ts"]}),
        None,
    );
    let listing = leads::scope_listing(&request, true);
    let query = &listing["query"]["queries"][0];
    assert_eq!(query["operation"], "files", "{listing}");
    assert_eq!(query["includeIgnored"], true, "{listing}");
    assert_eq!(query["defaultExcludes"], false, "{listing}");
    assert_eq!(query["include"], serde_json::json!(["*.ts"]), "{listing}");
    let plain = leads::scope_listing(
        &ls_query(
            serde_json::json!({"path": "/repo", "matchString": "x"}),
            None,
        ),
        true,
    );
    let plain = &plain["query"]["queries"][0];
    assert!(
        plain.get("includeIgnored").is_none() && plain.get("defaultExcludes").is_none(),
        "{plain}"
    );
}

/// An ignore rule above the search root is named relative to it,
/// never by an absolute path.
#[test]
fn a_skip_reason_above_the_root_is_root_relative() {
    let top = tempfile::tempdir().expect("top");
    fs::create_dir_all(top.path().join(".git")).expect("git");
    fs::write(top.path().join(".gitignore"), "*.log\n").expect("ignore");
    let root = top.path().join("sub");
    fs::create_dir_all(&root).expect("root");
    let reason =
        crate::tools::empty_walk::ignore_rule(&root, &root.join("x.log"), false).expect("ignored");
    assert_eq!(reason, "../.gitignore:1 '*.log'");
}

/// A page holds as many files as fit the response window, in every view,
/// and names the enclosing declaration of every row it shows: 25 small
/// files are one page, not a page of 20 and a page of 5.
#[test]
fn a_small_result_of_many_files_fills_one_page_with_every_row_named() {
    let sources: Vec<(String, String)> = (0..25)
        .map(|i| {
            (
                format!("f{i:02}.rs"),
                format!("fn f{i}() {{\n    needle();\n}}\n"),
            )
        })
        .collect();
    let fixtures: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect();
    for view in ["matchOnly", "paginated"] {
        let request = ls_query(
            serde_json::json!({"matchString": "needle", "resultView": view}),
            None,
        );
        let pages = walk_pages(&fixtures, &request);
        assert_eq!(pages.len(), 1, "{view}: {pages:?}");
        let counts: Vec<usize> = pages
            .iter()
            .map(|page| page["files"].as_array().expect("files").len())
            .collect();
        assert_eq!(counts, [25], "{view}: {pages:?}");
        for page in &pages {
            assert!(page.get("warnings").is_none(), "{view}: {page}");
            for file in page["files"].as_array().expect("files") {
                assert!(file["matches"][0].get("enclosing").is_some(), "{page}");
            }
        }
    }
}

/// Streamed pages are cut by the response budget, so a file can ride
/// several pages: `totalItems` counts the hit rows the pages walk (each
/// shown exactly once), never the files, and `totalPages` counts those
/// same pages.
#[test]
fn streamed_pagination_counts_pages_and_items_in_one_unit() {
    let hot = "needle\n".repeat(1200);
    let fixtures = [("a.txt", "needle\n"), ("hot.txt", hot.as_str())];
    let request = ls_query(serde_json::json!({"matchString": "needle"}), None);
    let pages = walk_pages(&fixtures, &request);
    assert_eq!(pages.len(), 2, "budget-sized pages");
    let shown: usize = pages
        .iter()
        .flat_map(|page| page["files"].as_array().expect("files"))
        .map(|file| file["matches"].as_array().expect("rows").len())
        .sum();
    assert_eq!(shown, 1201);
    for page in &pages {
        let pagination = &page["pagination"];
        assert_eq!(pagination["totalPages"], pages.len(), "{page}");
        assert_eq!(pagination["totalItems"], shown, "{page}");
        assert!(pagination.get("pageSize").is_none(), "{page}");
    }
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

/// Local eval (a): children of the search root that are ignored by the
/// enclosing repository's `.gitignore` are named as ignored, even when each
/// child is a repository of its own (a nested `.git` ends the child's own
/// rules, never the outer rule that skipped the whole child).
#[test]
fn an_empty_search_over_ignored_nested_repositories_names_the_ignore_rule() {
    let top = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(top.path().join(".git")).expect("outer repository");
    fs::write(top.path().join(".gitignore"), "repos/*\n").expect("gitignore");
    let root = top.path().join("repos");
    for child in ["a", "b"] {
        fs::create_dir_all(root.join(child).join(".git")).expect("nested repository");
        fs::create_dir_all(root.join(child).join(".github")).expect("hidden dir");
        fs::write(root.join(child).join(".github/x.py"), "needle\n").expect("fixture");
        fs::write(root.join(child).join("y.py"), "needle\n").expect("fixture");
    }
    let reason = crate::tools::empty_walk::ignore_rule(&root, &root.join("a/.github/x.py"), false)
        .expect("skipped");
    assert_eq!(reason, "../.gitignore:1 'repos/*'");
    let (policy, security) = policy_for(top.path());
    let request = ls_query(
        serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    let body = serde_json::to_value(&result).expect("result");
    let entries = body["skippedPaths"]["entries"]
        .as_array()
        .expect("skipped paths");
    assert!(
        entries
            .iter()
            .any(|entry| entry["path"] == skipped_source_path(&root, "a")
                && entry["cause"] == "ignored"
                && entry["reason"] == reason),
        "{body}"
    );
    assert!(
        entries.iter().all(|entry| entry["cause"] != "hidden"),
        "the outer ignored scope explains the miss: {body}"
    );
    let lead = &result.next.as_ref().expect("lead")["includeSkipped"];
    assert_eq!(
        lead["query"]["queries"][0]["includeIgnored"], true,
        "{lead}"
    );
}

/// The flat totals of a search row (`matchCount`, `filesScanned`, …) satisfy
/// the strict output contract: no `stats` object, `fileCount`, `capped` or
/// `capReason`, and an unreadable path or a limit surfaces as
/// `fileErrors` / `partialReasons`.
#[test]
fn a_search_row_serializes_flat_totals_that_satisfy_the_output_contract() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.txt"), "needle one\nneedle two\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let mut wire = serde_json::to_value(&result).expect("serialize");
    assert_eq!(wire["matchCount"], 2, "{wire}");
    assert_eq!(wire["filesScanned"], 1, "{wire}");
    for gone in ["stats", "fileCount", "capped", "capReason", "searchTime"] {
        assert!(wire.get(gone).is_none(), "{gone}: {wire}");
    }
    crate::contracts::validate_output(
        "localSearch",
        &serde_json::json!({"results":[{"index":0,"data":wire.take()}]}),
    )
    .expect("flat totals satisfy the output contract");

    let stats = SearchStats {
        total_occurrences: Some(0),
        matched_lines: Some(0),
        files_matched: 0,
        files_searched: 3,
        bytes_searched: None,
        capped: Some(true),
        cap_reasons: Vec::new(),
        partial_reasons: vec!["maxFiles".into(), "maxOnlyMatchingPerLine".into()],
        error_count: Some(1),
        first_error: Some("x.bin: denied".into()),
        file_errors: Some(FileErrors {
            count: 1,
            first: Some("x.bin: denied".into()),
        }),
        binary_skipped: None,
    };
    let wire = serde_json::to_value(&stats).expect("serialize");
    assert_eq!(
        wire["fileErrors"],
        serde_json::json!({"count":1,"first":"x.bin: denied"})
    );
    assert_eq!(
        wire["partialReasons"],
        serde_json::json!(["maxFiles", "maxOnlyMatchingPerLine"])
    );
}

/// A line holding more matches than one page (or the old per-line cap of
/// 1000) still reaches every match: `next.nextMatchPage` walks them all.
#[test]
fn match_only_pages_every_match_of_a_line_with_thousands_of_hits() {
    let root = tempfile::tempdir().expect("fixture directory");
    let hits = 2_500;
    fs::write(
        root.path().join("min.js"),
        format!("{}\n", "needle ".repeat(hits)),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let mut request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "resultView": LocalSearchQueryResultView::MatchOnly, "matchPageSize": 400}),
        None,
    );
    let mut seen = Vec::new();
    for _ in 0..20 {
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("page");
        let body = serde_json::to_value(&result).expect("serialize");
        assert!(body.get("partialReasons").is_none(), "{body}");
        for matched in body["files"][0]["matches"].as_array().expect("matches") {
            seen.push(matched["column"].as_u64().expect("column"));
        }
        let Some(next) = body["next"].get("nextMatchPage") else {
            break;
        };
        request = ls_query(next["query"]["queries"][0].clone(), Some(&request));
    }
    let expected: Vec<u64> = (0..hits as u64).map(|n| n * 7 + 1).collect();
    assert_eq!(seen, expected);
}

/// Top-level totals are stated only in units the view counted: countLines
/// counts lines, not occurrences, so it states `matchedLineCount` and no
/// `matchCount`; filesWithout lists files without a hit and states neither.
#[test]
fn totals_are_stated_only_in_the_units_a_view_counted() {
    let files = [("a.txt", "foo foo\nfoo\n"), ("b.txt", "bar\n")];
    let lines = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countLines"}),
            None,
        ),
    );
    assert_eq!(lines["matchedLineCount"], 2, "{lines}");
    assert!(lines.get("matchCount").is_none(), "{lines}");
    let matches = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countMatches"}),
            None,
        ),
    );
    assert_eq!(matches["matchCount"], 3, "{matches}");
    let without = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "filesWithout"}),
            None,
        ),
    );
    assert_eq!(without["files"][0]["path"], "b.txt", "{without}");
    assert!(without.get("matchCount").is_none(), "{without}");
    assert!(without.get("matchedLineCount").is_none(), "{without}");
}

/// `reverse` reverses a count view under the default relevance sort, which
/// ranks by count: the fewest hits come first.
#[test]
fn reverse_flips_a_count_view_under_relevance() {
    let files = [("a.txt", "foo\n"), ("b.txt", "foo\nfoo\nfoo\n")];
    let forward = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countMatches"}),
            None,
        ),
    );
    assert_eq!(forward["files"][0]["path"], "b.txt", "{forward}");
    let reversed = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countMatches", "reverse": true}),
            None,
        ),
    );
    assert_eq!(reversed["files"][0]["path"], "a.txt", "{reversed}");
}

/// `reverse` is the exact reverse of the forward order, ties included.
#[test]
fn reverse_is_the_exact_reverse_of_a_count_view_including_ties() {
    let files = [
        ("a.txt", "foo\n"),
        ("b.txt", "foo\nfoo\n"),
        ("c.txt", "foo\nfoo\n"),
        ("d.txt", "foo\n"),
    ];
    for view in ["countMatches", "countLines"] {
        let order = |reverse: bool| -> Vec<String> {
            let body = search_fixture(
                &files,
                ls_query(
                    serde_json::json!({"matchString": "foo", "resultView": view, "reverse": reverse}),
                    None,
                ),
            );
            body["files"]
                .as_array()
                .expect("files")
                .iter()
                .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
                .collect()
        };
        let forward = order(false);
        assert_eq!(forward, ["b.txt", "c.txt", "a.txt", "d.txt"], "{view}");
        let mut expected = forward;
        expected.reverse();
        assert_eq!(order(true), expected, "{view}");
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
        assert_eq!(lead["confidence"], "exact", "{lead}");
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

/// A `language` given as a file extension or in another case is not a
/// ripgrep type name: the error names the type, and the repair runs the same
/// search with it.
#[test]
fn an_extension_or_case_variant_as_language_repairs_to_its_type_name() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.rs"), "fn alpha() {}\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    for language in ["rs", ".RS", "Rust"] {
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha", "language": language}),
            None,
        );
        let error =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect_err("not a type name");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
        assert!(error.message.contains("\"rust\""), "{error:?}");
        let next = error.next.as_deref().expect("repair");
        assert_eq!(
            next["repair"]["query"]["queries"][0]["language"], "rust",
            "{next}"
        );
    }
}

/// An `include` that names a default-pruned directory (`build/*.rs`) asks
/// for its files: that directory is walked. Without it the default prune
/// stands.
#[test]
fn an_include_naming_a_pruned_directory_walks_it() {
    let files = [
        ("build/gen.rs", "fn alpha() {}\n"),
        ("src/a.rs", "fn alpha() {}\n"),
    ];
    let named = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "alpha", "include": ["build/*.rs"]}),
            None,
        ),
    );
    assert_eq!(named["files"][0]["path"], "build/gen.rs", "{named}");
    let plain = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString": "alpha"}), None),
    );
    let paths = plain["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| file["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec![serde_json::json!("src/a.rs")], "{plain}");
}

/// At the last page a continuation can name, file pages that remain end the
/// row at a terminal limit the shared cursor owner states.
#[test]
fn the_page_ceiling_names_the_file_pages_past_it() {
    let max = crate::contracts::query_schema_max(crate::id::ToolId::LocalSearch, None, "page");
    let root = tempfile::tempdir().expect("fixture");
    for index in 0..max + 2 {
        fs::write(root.path().join(format!("f{index:05}.txt")), "CEILING\n").expect("file");
    }
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"CEILING","regex":"literal","pageSize":1,"page":max,"sort":"path"}),
        None,
    );
    let result = execute_local_search(
        &request,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("search");
    assert!(
        result
            .next
            .as_ref()
            .is_none_or(|next| next.get("nextPage").is_none()),
        "{:?}",
        result.next
    );
    let warning = format!(
        "terminalLimit: page:{max} is the last a continuation may request; the rest is on no page."
    );
    assert!(result.warnings.contains(&warning), "{:?}", result.warnings);
    assert!(result.terminal_limit && result.is_partial);
    assert_eq!(
        result.stats.partial_reasons,
        [crate::response::paging::CURSOR_MAXIMUM]
    );
}

#[test]
fn skipped_path_evidence_outlives_compact_hints() {
    let root = tempfile::tempdir().expect("fixture");
    // `.ignore` rules apply outside a git work tree, where the default
    // prune names directories by name alone.
    let ignored = (0..3)
        .map(|n| format!("ignored_{n}_{}", "long_name_".repeat(12)))
        .collect::<Vec<_>>();
    fs::write(
        root.path().join(".ignore"),
        ignored
            .iter()
            .map(|n| format!("{n}/\n"))
            .collect::<String>(),
    )
    .expect("rules");
    for dir in &ignored {
        fs::create_dir(root.path().join(dir)).expect("ignored directory");
        fs::write(root.path().join(dir).join("hit.txt"), "needle\n").expect("hit");
    }
    let generated = format!("{}.min.js", "generated_".repeat(12));
    fs::write(root.path().join(&generated), "needle\n").expect("generated hit");
    let pruned = format!("{}/node_modules", "container_".repeat(10));
    fs::create_dir_all(root.path().join(&pruned)).expect("pruned directory");
    fs::write(root.path().join(&pruned).join("hit.txt"), "needle\n").expect("pruned hit");
    fs::write(root.path().join("visible.txt"), "different\n").expect("visible");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"needle"}),
        None,
    );
    for unproven in [false, true] {
        let run = || {
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("search")
        };
        let result = if unproven {
            crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, run)
        } else {
            run()
        };
        for hint in &result.hints {
            assert!(hint.chars().count() <= 120, "unproven={unproven}: {hint}");
        }
        let body = serde_json::to_value(&result).expect("result");
        let entries = body["skippedPaths"]["entries"]
            .as_array()
            .expect("skipped entries");
        assert_eq!(body["skippedPaths"]["sampled"], true, "{body}");
        for (n, dir) in ignored.iter().enumerate() {
            let matching = entries
                .iter()
                .filter(|entry| {
                    entry["path"] == skipped_source_path(root.path(), dir)
                        && entry["cause"] == "ignored"
                })
                .collect::<Vec<_>>();
            assert_eq!(matching.len(), 1, "unproven={unproven}: {body}");
            assert_eq!(matching[0]["reason"], format!(".ignore:{} '{dir}/'", n + 1));
        }
        // A proven miss names each default-excluded path that holds a
        // match; an unproven one counts the walk's under one entry.
        let excluded = entries
            .iter()
            .filter(|entry| entry["cause"] == "default-excluded")
            .collect::<Vec<_>>();
        if unproven {
            assert_eq!(excluded.len(), 1, "{body}");
            assert_eq!(excluded[0]["count"], 2, "{body}");
        } else {
            for name in [&generated, &pruned] {
                assert_eq!(
                    excluded
                        .iter()
                        .filter(|entry| entry["path"] == skipped_source_path(root.path(), name))
                        .count(),
                    1,
                    "{body}"
                );
            }
        }
        assert_leads_validate(&body, "skipped paths");
        let next = body["next"]["includeSkipped"]["query"]["queries"][0].clone();
        let prepared = crate::contracts::prepare_many_and_validate(
            "localSearch",
            serde_json::json!({"queries":[next]}),
        )
        .expect("canonical continuation");
        let retried = ls_query(prepared[0].clone(), None);
        let found =
            execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
                .expect("copied lead");
        assert_eq!(found.files.len(), 5, "{found:?}");
    }
}

#[test]
fn skipped_paths_are_first_page_disclosure_with_copied_search_scope() {
    let root = tempfile::tempdir().expect("fixture");
    for n in 0..3 {
        fs::write(root.path().join(format!("v{n}.txt")), "needle\n").expect("visible hit");
    }
    fs::write(root.path().join("generated.min.js"), "needle\n").expect("excluded");
    let (policy, security) = policy_for(root.path());
    let mut request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"needle","pageSize":1,"resultView":"files"}),
        None,
    );
    let mut paths = Vec::new();
    for page in 0..3 {
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("page");
        let body = serde_json::to_value(&result).expect("result");
        if page == 0 {
            assert_eq!(body["skippedPaths"]["sampled"], false, "{body}");
            assert_eq!(
                body["skippedPaths"]["entries"],
                serde_json::json!([{"path":skipped_source_path(root.path(),"generated.min.js"),"cause":"default-excluded"}]),
                "{body}"
            );
        } else {
            assert!(
                body.get("skippedPaths").is_none(),
                "disclosure is not repeated: {body}"
            );
        }
        paths.extend(result.files.iter().map(|f| f.path.clone()));
        let published = crate::tools::test_support::published(
            crate::id::ToolId::LocalSearch,
            &serde_json::to_value(&request).expect("query"),
            &body,
        );
        crate::contracts::validate_output("localSearch", &published).expect("published schema");
        if page < 2 {
            let next = body["next"]["nextPage"]["query"].clone();
            let prepared = crate::contracts::prepare_many_and_validate("localSearch", next)
                .expect("canonical page");
            request = ls_query(prepared[0].clone(), None);
        } else {
            assert!(body["next"]["nextPage"].is_null(), "{body}");
        }
    }
    assert_eq!(paths, ["v0.txt", "v1.txt", "v2.txt"]);
}

#[test]
fn skipped_paths_are_not_repeated_on_the_match_page_axis() {
    let root = tempfile::tempdir().expect("fixture");
    for name in ["a.txt", "b.txt"] {
        fs::write(root.path().join(name), "needle one\nneedle two\n").expect("hits");
    }
    fs::write(root.path().join("generated.min.js"), "needle\n").expect("excluded");
    let start = serde_json::to_value(ls_query(
        serde_json::json!({"path":root.path(),"matchString":"needle","pageSize":1,"matchPageSize":1,"sort":"path"}),
        None,
    ))
    .expect("query");
    // Each branch exactly once, every page within the output contract.
    let (walk, bodies) = walk_search(root.path(), start, None);
    assert_eq!(walk.queries.len(), 4);
    let disclosures = bodies
        .iter()
        .filter(|body| body.get("skippedPaths").is_some())
        .count();
    assert_eq!(disclosures, 1);
    // The walk's breadth-first order follows the lead order; the rows are
    // each hit once.
    let mut rows = walk.rows;
    rows.sort();
    assert_eq!(rows, ["a.txt:1", "a.txt:2", "b.txt:1", "b.txt:2"]);
}

/// `includeHidden` alone reaches every dot directory that is not generated
/// output or ignored: the tool-config directories (`.github`, `.vscode`) are
/// no longer behind `defaultExcludes` as well, as in structureSearch. A dot
/// directory a `.gitignore` rule names stays disclosed as ignored, behind
/// `includeIgnored`.
#[test]
fn include_hidden_alone_reaches_hidden_directories_that_are_not_ignored_or_generated() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir(root.path().join(".git")).expect("repository");
    fs::write(root.path().join(".gitignore"), ".octocode/\n").expect("rules");
    for file in [
        "src/a.txt",
        ".agents/notes.md",
        ".github/workflows/ci.yml",
        ".vscode/settings.json",
        ".octocode/tmp/scratch.txt",
    ] {
        let path = root.path().join(file);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, "needle\n").expect("hit");
    }
    let (policy, security) = policy_for(root.path());
    let search = |fields: serde_json::Value| {
        let request = ls_query(
            serde_json::json!({"path": root.path(), "matchString": "needle"}),
            None,
        );
        let request = ls_query(fields, Some(&request));
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("search");
        serde_json::to_value(&result).expect("result")
    };
    let paths = |body: &serde_json::Value| {
        let mut paths = body["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|file| file["path"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        paths.sort_unstable();
        paths
    };
    assert_eq!(paths(&search(serde_json::json!({}))), ["src/a.txt"]);
    let hidden = search(serde_json::json!({"includeHidden": true}));
    assert_eq!(
        paths(&hidden),
        [
            ".agents/notes.md",
            ".github/workflows/ci.yml",
            ".vscode/settings.json",
            "src/a.txt"
        ],
        "{hidden}"
    );
    // What `includeHidden` leaves out is disclosed with the one flag that
    // reaches it, not silently.
    let causes = hidden["skippedPaths"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry["cause"].as_str())
        .collect::<Vec<_>>();
    assert!(causes.contains(&"ignored"), "{hidden}");
    let lead = &hidden["next"]["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["includeIgnored"], true, "{hidden}");
    assert!(lead.get("defaultExcludes").is_none(), "{hidden}");
}

/// Outside a git work tree the default prune names its directories by name:
/// `includeHidden` alone still reaches the tool-config ones (`.github`,
/// `.vscode`), while generated dot directories (`.cache`) stay pruned.
#[test]
fn include_hidden_alone_reaches_tool_config_directories_outside_a_repository() {
    let root = tempfile::tempdir().expect("fixture");
    for file in [".github/ci.yml", ".vscode/settings.json", ".cache/c.txt"] {
        let path = root.path().join(file);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, "needle\n").expect("hit");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "needle", "includeHidden": true}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let mut paths = result
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(paths, [".github/ci.yml", ".vscode/settings.json"]);
}

/// A search with hits still discloses the dot entries it left out, and its
/// one `includeSkipped` lead lifts the cause on the way: the one flag
/// `includeHidden` reaches a dot directory the text search once pruned by
/// default too (`.github`), and following the lead reaches the hit there.
#[test]
fn a_search_with_hits_discloses_hidden_entries_and_its_lead_reaches_them() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join(".github/workflows")).expect("ci dir");
    fs::write(
        root.path().join(".github/workflows/ci.yml"),
        "uses: actions/setup-node@v5\n",
    )
    .expect("ci");
    fs::write(root.path().join(".eslintrc.json"), "{}\n").expect("dot file");
    fs::write(
        root.path().join("notes.md"),
        "uses: actions/setup-node@v4\n",
    )
    .expect("doc");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "actions/setup-node"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert_eq!(
        result
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["notes.md"]
    );
    let body = serde_json::to_value(&result).expect("result");
    assert_eq!(
        body["skippedPaths"]["entries"],
        serde_json::json!([
            {"path": skipped_source_path(root.path(), ".eslintrc.json"), "cause": "hidden", "count": 2},
        ]),
        "{body}"
    );
    let lead = body["next"]["includeSkipped"]["query"]["queries"][0].clone();
    assert_eq!(lead["includeHidden"], true, "{body}");
    assert!(lead.get("defaultExcludes").is_none(), "{body}");
    let retried = ls_query(lead, None);
    let all = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("lead");
    let mut paths = all
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(paths, [".github/workflows/ci.yml", "notes.md"]);
}

/// A search with hits discloses the entries ignore rules left out too (the
/// search decides them itself, so they never vanish inside the walker),
/// counted under one entry, and its one `includeSkipped` lead lifts what
/// stands on the way: an ignored dot directory is hidden too, and an
/// ignored `node_modules/` default-excluded too. Following it reaches
/// every hit.
#[test]
fn a_search_with_hits_discloses_ignored_entries_and_its_lead_reaches_them() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir(root.path().join(".git")).expect("repository");
    fs::write(
        root.path().join(".gitignore"),
        "results/\n.scratch/\nnode_modules/\n",
    )
    .expect("rules");
    for file in [
        "src/a.txt",
        "results/run/r.txt",
        ".scratch/s.txt",
        "node_modules/dep/n.txt",
    ] {
        let path = root.path().join(file);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, "tool-health\n").expect("hit");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "tool-health"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("result");
    assert_eq!(body["files"].as_array().map(Vec::len), Some(1), "{body}");
    let entries = body["skippedPaths"]["entries"].as_array().expect("entries");
    let ignored = entries
        .iter()
        .find(|entry| entry["cause"] == "ignored")
        .expect("ignored entry");
    assert_eq!(ignored["count"], 3, "{body}");
    let lead = body["next"]["includeSkipped"]["query"]["queries"][0].clone();
    assert_eq!(lead["includeIgnored"], true, "{body}");
    assert_eq!(lead["includeHidden"], true, "{body}");
    assert_eq!(lead["defaultExcludes"], false, "{body}");
    let all = execute_local_search(
        &ls_query(lead, None),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("lead");
    let mut paths = all
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            ".scratch/s.txt",
            "node_modules/dep/n.txt",
            "results/run/r.txt",
            "src/a.txt"
        ]
    );
    let details = execute_local_search(
        &ls_query(
            serde_json::json!({"path": root.path(), "matchString": "tool-health", "extras": ["details"]}),
            None,
        ),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("details");
    let listed = serde_json::to_value(&details).expect("result")["skippedPaths"]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["cause"] == "ignored")
        .count();
    assert_eq!(listed, 3);
}

/// Disclosure costs one entry per cause, not one per path or name: the
/// cause's first path, counting the paths behind it (`sampled`), and
/// `extras:["details"]` lists every path.
#[test]
fn skipped_paths_are_counted_per_cause_and_listed_whole_on_request() {
    let root = tempfile::tempdir().expect("fixture");
    for n in 0..4 {
        let dir = root.path().join(format!("pkg{n}/node_modules/dep"));
        fs::create_dir_all(&dir).expect("pruned");
        fs::write(dir.join("x.txt"), "needle\n").expect("pruned hit");
        fs::write(root.path().join(format!("pkg{n}/app.min.js")), "needle\n").expect("generated");
    }
    fs::write(root.path().join("visible.txt"), "needle\n").expect("hit");
    let (policy, security) = policy_for(root.path());
    let run = |fields: serde_json::Value| {
        let mut request = serde_json::json!({"path": root.path(), "matchString": "needle"});
        request
            .as_object_mut()
            .expect("object")
            .extend(fields.as_object().expect("fields").clone());
        let result = execute_local_search(
            &ls_query(request, None),
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect("search");
        serde_json::to_value(&result).expect("result")
    };
    let grouped = run(serde_json::json!({}));
    assert_eq!(
        grouped["skippedPaths"],
        serde_json::json!({"entries": [
            {"path": skipped_source_path(root.path(), "pkg0/node_modules"), "cause": "default-excluded", "count": 8},
        ], "sampled": true}),
        "{grouped}"
    );
    assert!(
        grouped["warnings"].as_array().is_none_or(|all| all
            .iter()
            .all(|w| !w.to_string().contains("Default excludes"))),
        "{grouped}"
    );
    let every = run(serde_json::json!({"extras": ["details"]}));
    let entries = every["skippedPaths"]["entries"]
        .as_array()
        .expect("entries");
    assert_eq!(entries.len(), 8, "{every}");
    assert!(
        entries.iter().all(|entry| entry.get("reason").is_none()),
        "{every}"
    );
    assert_eq!(every["skippedPaths"]["sampled"], false, "{every}");
}

/// Inside a git work tree a directory with a default-excluded name that git
/// does not ignore is source the repository keeps (a crate's `build/`
/// scripts): it is searched. An ignored one stays out.
#[test]
fn a_tracked_directory_with_a_pruned_name_is_searched_inside_git() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir(root.path().join(".git")).expect("repository");
    fs::write(root.path().join(".gitignore"), "/target/\n").expect("rules");
    for file in ["build/config_gen.rs", "target/debug/out.rs", "src/lib.rs"] {
        let path = root.path().join(file);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, "fn needle() {}\n").expect("source");
    }
    fs::write(root.path().join("Cargo.lock"), "needle = \"1.0\"\n").expect("lockfile");
    let (policy, security) = policy_for(root.path());
    let run = |include_ignored: bool| {
        let request = ls_query(
            serde_json::json!({"path": root.path(), "matchString": "needle", "includeIgnored": include_ignored, "sort": "path"}),
            None,
        );
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
            .files
            .into_iter()
            .map(|file| file.path)
            .collect::<Vec<_>>()
    };
    // Lockfiles pin dependency versions: evidence, searched by default.
    assert_eq!(
        run(false),
        ["Cargo.lock", "build/config_gen.rs", "src/lib.rs"]
    );
    // `includeIgnored` walks ignored paths, but an ignored `target/` is
    // still default-excluded.
    assert_eq!(
        run(true),
        ["Cargo.lock", "build/config_gen.rs", "src/lib.rs"]
    );
}

/// Globs that matched no file lead to the scope's listing without them,
/// so the listing shows the names that exist instead of the same empty
/// filter; a search past the collection cap leads to the scope's top
/// directories (`narrowScope`), in the search's walk scope.
#[test]
fn a_scope_miss_lists_the_scope_without_its_globs_and_the_cap_splits_by_directory() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("lib.rs"), "fn main() {}\n").expect("lib");
    let missed = search_root(
        root.path(),
        serde_json::json!({"matchString": "main", "include": ["nonexistent_zz"]}),
    );
    let next = missed.next.as_ref().expect("lead");
    let listing = &next["viewTree"]["query"]["queries"][0];
    assert_eq!(listing["operation"], "files", "{next}");
    assert!(listing.get("include").is_none(), "{next}");
    let request = ls_query(
        serde_json::json!({"path": "/repo", "matchString": "x", "defaultExcludes": false}),
        None,
    );
    let tree = leads::scope_directories(&request);
    let tree = &tree["query"]["queries"][0];
    assert_eq!(
        (
            &tree["operation"],
            &tree["maxDepth"],
            &tree["defaultExcludes"]
        ),
        (
            &serde_json::json!("tree"),
            &serde_json::json!(1),
            &serde_json::json!(false)
        ),
        "{tree}"
    );
}

/// A field the row's other fields leave without effect is named in a
/// warning instead of silently ignored: context lines in a view that lists
/// no lines, and include/exclude globs on a path that is one file (searched
/// whatever the globs). A field that applies says nothing.
#[test]
fn a_field_without_effect_is_named_in_a_warning() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("lib.rs"), "fn main() {}\n").expect("lib");
    let warnings = |fields: serde_json::Value| search_root(root.path(), fields).warnings;
    assert_eq!(
        warnings(
            serde_json::json!({"matchString": "main", "resultView": "files", "contextLines": 3})
        ),
        ["contextLines has no effect with resultView \"files\": only line views show context."],
    );
    assert_eq!(
        warnings(serde_json::json!({
            "matchString": "main", "resultView": "countMatches", "contextBefore": 1, "contextAfter": 2
        })),
        [
            "contextBefore, contextAfter have no effect with resultView \"countMatches\": only line views show context."
        ],
    );
    assert!(warnings(serde_json::json!({"matchString": "main", "contextLines": 3})).is_empty());
    let file = root.path().join("lib.rs").to_string_lossy().into_owned();
    let (policy, security) = policy_for(root.path());
    let on_file = |fields: serde_json::Value| {
        let mut fields = fields;
        fields["path"] = file.clone().into();
        execute_local_search(
            &ls_query(fields, None),
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
            None,
        )
        .expect("search")
        .warnings
    };
    assert_eq!(
        on_file(
            serde_json::json!({"matchString": "main", "include": ["*.ts"], "exclude": ["*.rs"]})
        ),
        ["include, exclude have no effect: path is one file, searched whatever the globs."],
    );
    assert!(on_file(serde_json::json!({"matchString": "main"})).is_empty());
}
