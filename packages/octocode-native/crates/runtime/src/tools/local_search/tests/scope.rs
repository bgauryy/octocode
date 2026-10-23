use super::*;

use octocode_contract_types::ErrorCode;

mod counts;
mod enclosing_names;
mod hidden_ignored;
mod offered_leads;
mod skipped;

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
        local_search_query::MAX_COLLECTED_FILES,
    );
    let warning =
        coverage::uncollected_warning(&stats, local_search_query::MAX_COLLECTED_FILES).expect("disclosed");
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
