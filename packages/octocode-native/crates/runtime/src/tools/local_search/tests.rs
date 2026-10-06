use super::*;

/// Test query from wire JSON over a base query (or a minimal valid one);
/// `null` removes a field so its contract default applies.
fn ls_query(fields: serde_json::Value, base: Option<&LocalSearchQuery>) -> LocalSearchQuery {
    let mut value = base.map_or_else(
            || serde_json::json!({"path": "_", "matchString": "_", "mainGoal": "test", "reasoning": "test"}),
            |base| serde_json::to_value(base).expect("query serializes"),
        );
    let object = value.as_object_mut().expect("query object");
    for (key, field) in fields.as_object().expect("fields object") {
        if field.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), field.clone());
        }
    }
    serde_json::from_value(value).expect("valid localSearch query")
}
use crate::{policy::path::PathPolicy, security::ContentSecurity, tools::cancel::NeverCancel};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn context_fixture() -> (tempfile::TempDir, PathPolicy, ContentSecurity) {
    let root = tempfile::tempdir().expect("fixture directory");
    // Nine ~37-char lines around one hit: a ±4 window is ~334 chars.
    let lines: Vec<String> = (1..=9)
        .map(|i| {
            if i == 5 {
                format!("let needle_{i} = compute_value_number_{i}();")
            } else {
                format!("let other_{i} = compute_value_number_{i}();")
            }
        })
        .collect();
    fs::write(root.path().join("ctx.rs"), lines.join("\n") + "\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    (root, policy, security)
}

#[test]
fn match_page_ceiling_preserves_evidence_and_discloses_terminal_suffix() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.txt"), "CEILING\n".repeat(1001)).expect("matches");
    fs::write(root.path().join("b.txt"), "CEILING\n").expect("other file page");
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"CEILING","regex":"literal","pageSize":1,"matchPageSize":1,"matchPage":1000}),
        None,
    );
    let result = execute_local_search(
        &request,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
    )
    .expect("search");
    let body = serde_json::to_value(result).expect("output");
    assert_eq!(body["files"][0]["matches"][0]["line"], 1000, "{body}");
    assert!(body["next"].get("nextMatchPage").is_none(), "{body}");
    assert!(body["next"].get("nextPage").is_some(), "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
}

#[test]
fn context_lines_scale_the_default_match_content_length() {
    let (root, policy, security) = context_fixture();
    let base = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle".to_string(), "contextLines": 4}),
        None,
    );
    let run = |request: &LocalSearchQuery| {
        let result = execute_local_search(request, &policy, &security, &NeverCancel, None, None)
            .expect("search");
        serde_json::to_value(result).expect("serialize")
    };
    let scaled = run(&base);
    let matched = &scaled["files"][0]["matches"][0];
    assert!(matched.get("truncated").is_none(), "{scaled}");
    assert!(matched["value"].as_str().expect("value").chars().count() > 300);

    let explicit = run(&ls_query(
        serde_json::json!({"matchContentLength": 200}),
        Some(&base.clone()),
    ));
    assert_eq!(explicit["files"][0]["matches"][0]["truncated"], true);

    let detailed = run(&ls_query(
        serde_json::json!({"contextLines": null, "resultView": LocalSearchQueryResultView::Detailed}),
        Some(&base.clone()),
    ));
    assert!(
        detailed["files"][0]["matches"][0]
            .get("truncated")
            .is_none(),
        "{detailed}"
    );
}

#[test]
fn match_only_caps_display_after_unique_grouping_and_preserves_continuations() {
    let root = tempfile::tempdir().expect("fixture directory");
    let prefix = format!("needle{}", "界".repeat(4096));
    fs::write(
        root.path().join("giant.txt"),
        format!("{prefix}END\n{prefix}END\n{prefix}OTHER\n"),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle.*".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly, "matchContentLength": 30, "matchPageSize": 1, "unique": LocalSearchQueryUnique::Count}),
        None,
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("first page");
    let body = serde_json::to_value(&first).expect("serialize");
    let matched = &body["files"][0]["matches"][0];
    assert_eq!(
        matched["value"].as_str().expect("value").chars().count(),
        30
    );
    assert_eq!(matched["truncated"], true);
    assert_eq!(matched["originalChars"], 4105);
    assert_eq!(matched["returnedChars"], 30);
    assert_eq!(matched["count"], 2);
    assert_eq!(body["stats"]["totalMatches"], 3);
    assert_eq!(body["stats"]["totalMatchedLines"], 3);
    assert_eq!(body["stats"]["capped"], false);
    assert_eq!(body["files"][0]["pagination"]["totalItems"], 2);
    assert!(
        first
            .warnings
            .iter()
            .any(|warning| warning.contains("matchContentLength"))
    );
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert_eq!(next["matchContentLength"], 30);
    assert_eq!(next["unique"], "count");
    assert_eq!(next["matchPage"], 2);
    let continued = ls_query(
        serde_json::json!({"matchPage": 2, "snapshot": first.source_snapshot}),
        Some(&request),
    );
    let second = execute_local_search(&continued, &policy, &security, &NeverCancel, None, None)
        .expect("second page");
    let body = serde_json::to_value(second).expect("serialize");
    let matched = &body["files"][0]["matches"][0];
    assert_eq!(matched["originalChars"], 4107);
    assert_eq!(matched["returnedChars"], 30);
    assert_eq!(matched["count"], 1);
    // The last match page has nowhere further to route: no per-file
    // paging; its clipped value is read whole by `expandValues` only.
    assert!(body["files"][0].get("pagination").is_none(), "{body}");
    let names = body["next"]
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(names, ["expandValues"], "{body}");

    for (limit, expected_chars) in [(None, 200), (Some(1), 1), (Some(4105), 4105)] {
        let bounded = ls_query(
            serde_json::json!({"matchPage": 1, "snapshot": null, "matchContentLength": limit}),
            Some(&continued.clone()),
        );
        let result = execute_local_search(&bounded, &policy, &security, &NeverCancel, None, None)
            .expect("default, minimal and exact-boundary caps");
        let body = serde_json::to_value(result).expect("serialize");
        let matched = &body["files"][0]["matches"][0];
        assert_eq!(
            matched["value"].as_str().expect("value").chars().count(),
            expected_chars
        );
        assert_eq!(matched.get("truncated").is_some(), expected_chars < 4105);
        assert_eq!(
            matched.get("originalChars").is_some(),
            expected_chars < 4105
        );
        assert_eq!(matched["count"], 2);
    }
}

// A search that matches an interior base64 body line of a private key
// must not return the key body, even though the match view holds no BEGIN/END
// marker. The full-file block scan (triggered by the base64-shaped snippet)
// redacts it; the window sanitizer alone cannot.
#[test]
fn interior_private_key_match_is_redacted_without_markers_in_view() {
    let root = tempfile::tempdir().expect("fixture directory");
    // 64-char base64 body line carrying a distinctive, searchable fragment.
    let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let file = format!(
        "fn main() {{}}\n-----BEGIN RSA PRIVATE KEY-----\nZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ\n{body}\nYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY\n-----END RSA PRIVATE KEY-----\nlet done = true;\n"
    );
    fs::write(root.path().join("app.rs"), file).expect("fixture");
    // Content view returns the whole matched line (not just the match span),
    // so a hit inside the base64 body would surface the key material.
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "MIIEpQIB".to_string(), "resultView": LocalSearchQueryResultView::Detailed, "contextLines": 0}),
    );
    let body_json = serde_json::to_value(&result).expect("serialize");
    let value = body_json["files"][0]["matches"][0]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(
        !value.contains("interiorKeyBody"),
        "private key body leaked via a localSearch match: {value}"
    );
}

#[test]
fn redacted_match_values_are_flagged_as_not_verbatim() {
    let body = search_fixture(
        &[(
            "db.ts",
            "const url = \"postgres://admin:hunter2secretpw@db.example.com/app\";\n",
        )],
        ls_query(
            serde_json::json!({"matchString": "postgres".to_string()}),
            None,
        ),
    );
    let value = body["files"][0]["matches"][0]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(!value.contains("hunter2secretpw"), "{body}");
    assert!(
        body["warnings"].as_array().is_some_and(|warnings| warnings
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.starts_with("redactedMatches")))),
        "redaction must be flagged: {body}"
    );
    let clean = search_fixture(
        &[("a.txt", "plain needle\n")],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string()}),
            None,
        ),
    );
    assert!(!clean.to_string().contains("redactedMatches"), "{clean}");
}

// Over-redaction guard: a base64-shaped line with no private-key block
// anywhere in the file must be returned intact — the snippet triggers a
// full-file scan that finds no block and redacts nothing.
#[test]
fn innocent_base64_match_is_not_redacted() {
    let root = tempfile::tempdir().expect("fixture directory");
    let blob = "aGVsbG8gd29ybGRfaW5ub2NlbnRfYmFzZTY0X2Jsb2JfaGVyZQ==";
    fs::write(
        root.path().join("data.txt"),
        format!("prefix\n{blob}\nsuffix\n"),
    )
    .expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "aGVsbG8".to_string(), "resultView": LocalSearchQueryResultView::Detailed, "contextLines": 0}),
    );
    let body_json = serde_json::to_value(&result).expect("serialize");
    let value = body_json["files"][0]["matches"][0]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(
        value.contains(blob),
        "innocent base64 was wrongly redacted: {value}"
    );
}

// A pathological single giant line (generated/minified file) must not
// emit a multi-MB body. The total-response budget clips each match value —
// every match row and its line anchor is preserved (no silent drop, no
// continuation cursor needed); the clip is flagged `truncated`.
#[test]
fn oversized_match_is_clipped_to_response_budget_without_dropping_rows() {
    let root = tempfile::tempdir().expect("fixture directory");
    let line = format!("needle {}", "x".repeat(3_000_000));
    fs::write(root.path().join("giant.txt"), format!("{line}\n")).expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::Detailed, "matchContentLength": 5_000_000, "contextLines": 0}),
    );
    let matches = result.files[0].matches.as_ref().expect("matches");
    assert_eq!(
        matches.len(),
        1,
        "the match row must be preserved, not dropped"
    );
    assert_eq!(
        matches[0].line, 1,
        "line anchor preserved for localFetch follow-up"
    );
    assert!(
        matches[0].value.chars().count() <= 1_100_000,
        "value not clipped to budget: {} chars",
        matches[0].value.chars().count()
    );
    assert!(matches[0].truncated, "clip must be flagged truncated");
    let body = serde_json::to_string(&result).expect("serialize");
    assert!(
        body.len() <= 2_000_000,
        "response body exceeded the byte budget: {} bytes",
        body.len()
    );
}

#[test]
fn content_view_snippet_carries_truncation_indicator() {
    // A content-view match on a line longer than matchContentLength is
    // clipped by the engine; the runtime must surface truncated/originalChars
    // plus the truncation warning.
    let root = tempfile::tempdir().expect("fixture directory");
    let long_line = format!("needle {}", "x".repeat(600));
    fs::write(root.path().join("big.txt"), format!("{long_line}\n")).expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "needle".to_string()}),
    );
    let body = serde_json::to_value(&result).expect("serialize");
    let matched = &body["files"][0]["matches"][0];
    assert_eq!(matched["truncated"], true);
    assert!(
        matched["originalChars"].as_u64().expect("originalChars") >= 606,
        "{matched:?}"
    );
    assert!(matched["returnedChars"].as_u64().expect("returnedChars") <= 500);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("matchContentLength"))
    );
}

#[test]
fn excludes_sensitive_and_symlink_descendants_before_projection() {
    let root = std::env::temp_dir().join(format!(
        "local-search-security-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(root.join(".aws")).expect("fixture dirs");
    fs::write(root.join("safe.txt"), "needle\n").expect("safe fixture");
    fs::write(root.join(".aws/credentials"), "needle secret\n").expect("secret fixture");
    fs::write(root.join("private-key.pem"), "needle private\n").expect("key fixture");
    fs::write(root.join("binary.dat"), b"needle\0binary").expect("binary fixture");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/passwd", root.join("escape")).expect("symlink fixture");
    }
    let policy = crate::tools::test_support::workspace_policy(&root);
    let security = ContentSecurity::new();
    let request = ls_query(
        serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle".to_string(), "hidden": true, "noIgnore": true, "sort": LocalSearchQuerySort::Path}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert_eq!(
        result
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        // A binary file keeps the matches before its first NUL.
        vec!["binary.dat", "safe.txt"]
    );
    assert_eq!(result.stats.files_searched, 2);
    assert_eq!(result.stats.cap_reason.as_deref(), Some("binaryQuit"));
    assert_eq!(
        result.source_root,
        root.canonicalize().expect("canonical root")
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn rejects_changed_and_forged_snapshots_even_on_empty_or_final_pages() {
    let root = std::env::temp_dir().join(format!(
        "local-search-snapshot-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("fixture dir");
    let source = root.join("source.txt");
    fs::write(&source, "needle one\n").expect("fixture");
    let policy = crate::tools::test_support::workspace_policy(&root);
    let security = ContentSecurity::new();
    let request = ls_query(
        serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle".to_string()}),
        None,
    );
    let initial = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("initial search");
    let snapshot = initial.source_snapshot.expect("identity on final page");
    let mut continued = request.clone();
    continued.snapshot = Some(snapshot.parse().expect("snapshot"));
    execute_local_search(&continued, &policy, &security, &NeverCancel, None, None)
        .expect("unchanged snapshot");

    fs::write(&source, "needle two\n").expect("same-size mutation");
    let stale = execute_local_search(&continued, &policy, &security, &NeverCancel, None, None)
        .expect_err("changed results must reject snapshot");
    assert_eq!(stale.code, "staleSnapshot");
    assert_eq!(stale.message, crate::response::pages::STALE_SNAPSHOT_ERROR);
    let restart = stale.next.expect("restart");
    assert_eq!(restart["restart"]["tool"], "localSearch");
    assert!(
        restart["restart"]["query"]["queries"][0]
            .get("snapshot")
            .is_none()
    );
    assert_eq!(restart["restart"]["query"]["queries"][0]["page"], 1);
    assert_eq!(restart["restart"]["query"]["queries"][0]["matchPage"], 1);

    let mut empty = request.clone();
    empty.match_string = "absent".parse().expect("matchString");
    empty.snapshot = Some(snapshot.parse().expect("snapshot"));
    assert_eq!(
        execute_local_search(&empty, &policy, &security, &NeverCancel, None, None)
            .expect_err("query-scope/empty mismatch")
            .code,
        "staleSnapshot"
    );
    continued.snapshot = Some("forged".parse().expect("snapshot"));
    assert_eq!(
        execute_local_search(&continued, &policy, &security, &NeverCancel, None, None)
            .expect_err("forged snapshot")
            .code,
        "staleSnapshot"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

/// A continuation's stored scan is bound to the query that produced
/// it: changing any search-semantic field while keeping the cursor must
/// reject, whether the stored scan is still cached or was evicted.
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
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha", "include": ["*.ts"], "noIgnore": true, "pageSize": 1, "matchPageSize": 1, "sort": "path"}),
        None,
    );
    let initial = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
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
        ("hidden", serde_json::json!({"hidden": true})),
        ("wholeWord", serde_json::json!({"wholeWord": true})),
        (
            "path",
            serde_json::json!({"path": root.path().join("other.ts").to_string_lossy().into_owned()}),
        ),
    ];
    for cached in [true, false] {
        if !cached {
            super::manifest::evict(&snapshot);
        }
        let page_two =
            execute_local_search(&continued, &policy, &security, &NeverCancel, None, None)
                .unwrap_or_else(|e| {
                    panic!("unchanged continuation (cached={cached}): {}", e.message)
                });
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
            match execute_local_search(&changed, &policy, &security, &NeverCancel, None, None) {
                Ok(result) => panic!(
                    "{field} change reused the old cursor (cached={cached}): {:?}",
                    result.files.iter().map(|f| &f.path).collect::<Vec<_>>()
                ),
                Err(error) => {
                    assert_eq!(error.code, "staleSnapshot", "{field} cached={cached}");
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
    let control = execute_local_search(&control, &policy, &security, &NeverCancel, None, None)
        .expect("control search");
    assert!(control.files.is_empty());
}

/// Continuations reuse the page-1 scan whatever `noIgnore` says, so pages
/// stay consistent; a matched file that changed since the scan, or a
/// scan that expired, falls back to a rescan that restarts a changed
/// result.
#[test]
fn continuations_reuse_their_scan_until_a_matched_file_changes() {
    let root = tempfile::tempdir().expect("fixture directory");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.path().join(name), "needle\n").expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "pageSize": 1, "sort": "path"}),
        None,
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
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
    // A new matching file is not part of the stored scan.
    fs::write(root.path().join("a0.txt"), "needle\n").expect("new file");
    let second = execute_local_search(&page(2), &policy, &security, &NeverCancel, None, None)
        .expect("page 2 from the stored scan");
    assert_eq!(second.files[0].path, "b.txt");
    // Once the scan is gone, the rescan sees the new file and restarts.
    super::manifest::evict(&snapshot);
    assert_eq!(
        execute_local_search(&page(2), &policy, &security, &NeverCancel, None, None)
            .expect_err("changed result")
            .code,
        "staleSnapshot"
    );
    fs::remove_file(root.path().join("a0.txt")).expect("cleanup");
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("page 1 again");
    assert_eq!(first.source_snapshot.as_deref(), Some(snapshot.as_str()));
    // A matched file that changed invalidates the stored scan.
    fs::write(root.path().join("c.txt"), "needle\nneedle\n").expect("edit");
    assert_eq!(
        execute_local_search(&page(3), &policy, &security, &NeverCancel, None, None)
            .expect_err("edited matched file")
            .code,
        "staleSnapshot"
    );
}

/// Replace `path`'s bytes with `content` of the same length and restore
/// its nanosecond modification time, so size and time cannot reveal the
/// edit.
fn rewrite_keeping_stamp(path: &std::path::Path, content: &str) {
    let before = fs::metadata(path).expect("fixture metadata");
    assert_eq!(before.len(), content.len() as u64, "same-size rewrite");
    let modified = before.modified().expect("mtime");
    fs::write(path, content).expect("rewrite");
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(modified))
        .expect("restore mtime");
    let after = fs::metadata(path).expect("fixture metadata");
    assert_eq!(
        (after.len(), after.modified().expect("mtime")),
        (before.len(), modified)
    );
}

fn values(result: &LocalSearchResult) -> String {
    serde_json::to_string(&result.files).expect("files serialize")
}

/// A continuation never serves stored values from a file edited since
/// the scan, even when the edit keeps its size and modification time.
#[test]
fn a_same_size_same_mtime_edit_restarts_instead_of_serving_old_values() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.ts"), "const needle = \"OLD_A\";\n").expect("fixture");
    let b = root.path().join("b.ts");
    fs::write(&b, "const needle = \"OLD_B\";\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "regex": "literal", "sort": "path", "include": ["*.ts"], "noIgnore": true, "pageSize": 1}),
        None,
    );
    let (_, _, page_two) = first_page(&request, &policy, &security);
    rewrite_keeping_stamp(&b, "const needle = \"NEW_B\";\n");
    match execute_local_search(&page_two, &policy, &security, &NeverCancel, None, None) {
        Ok(result) => panic!("stale page served: {}", values(&result)),
        Err(error) => {
            assert_eq!(error.code, "staleSnapshot");
            let restart = error.next.expect("restart");
            assert!(
                restart["restart"]["query"]["queries"][0]
                    .get("snapshot")
                    .is_none()
            );
        }
    }
    for view in ["files", "countMatches"] {
        fs::write(&b, "const needle = \"OLD_B\";\n").expect("fixture");
        let listed = ls_query(serde_json::json!({"resultView": view}), Some(&request));
        let first = execute_local_search(&listed, &policy, &security, &NeverCancel, None, None)
            .expect("list page 1");
        let next = ls_query(
            serde_json::json!({"snapshot": first.source_snapshot.clone().expect("identity"), "page": 2}),
            Some(&listed),
        );
        rewrite_keeping_stamp(&b, "const nEEdle = \"NEW_B\";\n");
        match execute_local_search(&next, &policy, &security, &NeverCancel, None, None) {
            Ok(result) => panic!("{view}: stale page served: {}", values(&result)),
            Err(error) => assert_eq!(error.code, "staleSnapshot", "{view}"),
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
    )
    .expect("fresh search");
    assert!(values(&fresh).contains("NEW_B"), "{}", values(&fresh));
}

/// Every view's values are present in the bytes they were scanned from,
/// so unchanged files keep reusing the stored scan: a matching file added
/// after page 1 stays unseen on page 2 instead of restarting the search.
#[test]
fn every_view_keeps_reusing_its_scan_while_files_are_unchanged() {
    let root = tempfile::tempdir().expect("fixture directory");
    let long = format!("{} needle tail {}", "x".repeat(300), "y".repeat(300));
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(
            root.path().join(name),
            format!("head\r\nlet needle = 1;\r\n{long}\nneedle\nmid ✓ line\nneedle again ✓\nend\n"),
        )
        .expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let views = [
        serde_json::json!({}),
        serde_json::json!({"contextLines": 2}),
        serde_json::json!({"matchContentLength": 20}),
        serde_json::json!({"resultView": "detailed"}),
        serde_json::json!({"resultView": "matchOnly", "matchContentLength": 5}),
        serde_json::json!({"resultView": "matchOnly", "unique": "count", "matchString": "needle[ a-z]*"}),
        serde_json::json!({"resultView": "files"}),
        serde_json::json!({"resultView": "countMatches"}),
        serde_json::json!({"multiline": "on", "matchString": "needle\\nmid"}),
    ];
    for (index, view) in views.iter().enumerate() {
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "pageSize": 1, "sort": "path"}),
            None,
        );
        let request = ls_query(view.clone(), Some(&request));
        let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
            .unwrap_or_else(|e| panic!("{view}: {}", e.message));
        let snapshot = first
            .source_snapshot
            .clone()
            .unwrap_or_else(|| panic!("{view}: no continuation"));
        let added = root.path().join(format!("a{index}.txt"));
        fs::write(&added, "needle\n").expect("new file");
        let second = execute_local_search(
            &ls_query(
                serde_json::json!({"snapshot": snapshot, "page": 2}),
                Some(&request),
            ),
            &policy,
            &security,
            &NeverCancel,
            None,
            None,
        )
        .unwrap_or_else(|e| panic!("{view}: stored scan not reused: {}", e.message));
        assert_eq!(second.files[0].path, "b.txt", "{view}");
        fs::remove_file(added).expect("cleanup");
    }
}

/// A stored key-shaped value is redacted on its first page; after the
/// source is swapped for innocuous text of the same size and time, the
/// stored value must not be checked against the new text and shown.
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
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "QUJD", "regex": "literal", "sort": "path", "include": ["*.ts"], "noIgnore": true, "pageSize": 1}),
        None,
    );
    let (_, _, page_two) = first_page(&request, &policy, &security);
    let before = execute_local_search(&page_two, &policy, &security, &NeverCancel, None, None)
        .expect("page 2 from the stored scan");
    assert!(!values(&before).contains(&body), "{}", values(&before));
    assert!(values(&before).contains("REDACTED"), "{}", values(&before));
    let clean = format!(
        "// clean source replacement\n{}\n{}\n",
        "Z".repeat(body.len()),
        "// clean trailer".to_owned() + &" ".repeat("-----END PRIVATE KEY-----".len() - 16)
    );
    rewrite_keeping_stamp(&key, &clean);
    match execute_local_search(&page_two, &policy, &security, &NeverCancel, None, None) {
        Ok(result) => assert!(
            !values(&result).contains(&body),
            "old key body surfaced: {}",
            values(&result)
        ),
        Err(error) => assert_eq!(error.code, "staleSnapshot"),
    }
}

#[test]
fn capped_or_bound_results_are_partial_and_terminal_when_next_is_impossible() {
    use super::executor::classify_search;
    assert_eq!(
        classify_search(true, false, false, false, false, true),
        (SearchStatus::Empty, false)
    );
    assert_eq!(
        classify_search(true, false, false, false, true, true),
        (SearchStatus::Partial, true),
        "an empty result with unsearched content is partial, not empty"
    );
    assert_eq!(
        classify_search(false, true, false, false, false, true),
        (SearchStatus::Partial, true)
    );
    assert_eq!(
        classify_search(false, false, true, false, false, false),
        (SearchStatus::Partial, false)
    );
    assert_eq!(
        classify_search(false, false, true, false, false, true),
        (SearchStatus::Partial, true)
    );
    assert_eq!(
        classify_search(false, false, false, false, false, true),
        (SearchStatus::Success, false)
    );
}

/// Source directories with build-ish names are searched like any other;
/// only dependency/build caches are pruned.
#[test]
fn source_dirs_named_output_cache_vendor_are_searched_but_node_modules_is_not() {
    let root = tempfile::tempdir().expect("fixture directory");
    for dir in [
        "detail/output",
        "core/cache",
        "vendor/lib",
        "node_modules/pkg",
    ] {
        fs::create_dir_all(root.path().join(dir)).expect("dir");
        fs::write(root.path().join(dir).join("x.txt"), "needle\n").expect("file");
    }
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "resultView": "files"}),
        None,
    );
    let result = execute_local_search(
        &request,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
    )
    .expect("search");
    let body = serde_json::to_string(&serde_json::to_value(&result).expect("json")).expect("text");
    for found in [
        "detail/output/x.txt",
        "core/cache/x.txt",
        "vendor/lib/x.txt",
    ] {
        assert!(body.contains(found), "{found} missing: {body}");
    }
    assert!(
        result
            .files
            .iter()
            .all(|file| !file.path.contains("node_modules")),
        "{body}"
    );
    // Not searched, but never silently: named with the escape flag.
    assert!(
        result.warnings.iter().any(|warning| warning
            == "Default excludes skipped 1 dir (node_modules/); the same search with defaultExcludes:false covers them."),
        "{body}"
    );
}

/// Explicitly targeting a single file that the engine skips (over the
/// per-file byte ceiling → capped:true, capReason:"maxFileSize",
/// filesScanned:0) must explain the skip instead of returning a silent
/// "no matches" false negative.
#[test]
fn skipped_single_file_target_explains_the_cap_instead_of_silent_empty() {
    let root = tempfile::tempdir().expect("fixture directory");
    let oversized = root.path().join("huge.txt");
    // Sparse file over the engine's 512 MiB line-search ceiling: the skip
    // is decided on metadata length, so no bytes need to be written.
    let file = fs::File::create(&oversized).expect("fixture");
    file.set_len(512 * 1024 * 1024 + 1).expect("sparse length");
    drop(file);
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": oversized.to_string_lossy().into_owned(), "matchString": "needle".to_string()}),
        None,
    );

    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");

    assert_eq!(body["stats"]["filesScanned"], 0, "{body}");
    assert_eq!(body["stats"]["capped"], true, "{body}");
    assert!(
        body["stats"]["capReason"]
            .as_str()
            .is_some_and(|reason| reason.contains("maxFileSize")),
        "{body}"
    );
    let hint = body["hints"][0].as_str().expect("skip hint");
    assert!(hint.contains("skipped"), "{hint}");
    assert!(hint.contains("maxFileSize"), "{hint}");
    assert!(hint.contains("localFetch"), "{hint}");
}

fn search_fixture(files: &[(&str, &str)], request: LocalSearchQuery) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture directory");
    for (name, body) in files {
        let path = root.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture dir");
        }
        fs::write(path, body).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
        Some(&request),
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    serde_json::to_value(&result).expect("serialize")
}

/// `include` naming a directory path scopes the search to its files
/// (it used to match nothing: filesScanned 0 read as "no text match").
#[test]
fn a_directory_path_include_searches_its_files() {
    let body = search_fixture(
        &[("pkg/src/a.ts", "needle\n"), ("pkg/test/b.ts", "needle\n")],
        ls_query(
            serde_json::json!({"matchString": "needle", "include": ["pkg/src"]}),
            None,
        ),
    );
    let text = body.to_string();
    assert!(text.contains("pkg/src/a.ts"), "{body}");
    assert!(!text.contains("pkg/test/b.ts"), "{body}");
}

/// When include/exclude leave no file to search, the row says the scope
/// matched nothing instead of suggesting a different spelling of the text.
#[test]
fn an_include_that_matches_no_file_says_so() {
    let body = search_fixture(
        &[("pkg/src/a.ts", "needle\n")],
        ls_query(
            serde_json::json!({"matchString": "Needle", "include": ["nowhere/dir"]}),
            None,
        ),
    );
    assert_eq!(body["stats"]["filesScanned"], 0, "{body}");
    let hint = body["hints"][0].as_str().expect("scope hint");
    assert!(hint.contains("include/exclude matched no file"), "{hint}");
    assert!(
        hint.chars().count() <= 120,
        "clipped by the response stage: {hint}"
    );
    assert!(
        !body["hints"].to_string().contains("all-lowercase"),
        "{body}"
    );
}

/// A small page names each hit's enclosing declaration (a definition hit
/// names the one around it), and a searched symbol declared on a hit
/// line yields an lspSearch lead anchored on that line (`callers` for a
/// function) when a language server would start for the file. A sweep of
/// more hits than one small page stays plain lines.
#[test]
fn small_pages_name_enclosing_declarations_and_lead_to_references() {
    let source = "struct Harness;\nimpl Harness {\n    fn try_read_output(&self) {\n        helper();\n    }\n}\n\nfn helper() {}\n\nfn caller() {\n    helper();\n}\n";
    let body = search_fixture(
        &[("src/a.rs", source)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let rows = &body["files"][0]["matches"];
    assert_eq!(rows[0]["line"], 4, "{body}");
    assert_eq!(rows[0]["in"], "method try_read_output@3-5", "{body}");
    assert_eq!(rows[1]["line"], 8, "{body}");
    assert!(rows[1].get("in").is_none(), "{body}");
    assert_eq!(rows[2]["in"], "fn caller@10-12", "{body}");
    let lead = &body["next"]["verifyReferences"];
    let declared_at = body["files"][0]["path"].as_str().expect("path").to_owned();
    let server = crate::tools::lsp_search::verify_query(
        &declared_at,
        "helper",
        8,
        crate::tools::lsp_search::Verify::Callers,
    );
    if server.is_none() {
        assert!(lead.is_null(), "no language server, no lead: {body}");
        return;
    }
    assert_eq!(lead["tool"], "lspSearch", "{body}");
    assert_eq!(
        lead["query"]["queries"][0]["operation"], "callers",
        "{body}"
    );
    assert_eq!(
        lead["query"]["queries"][0]["symbolName"], "helper",
        "{body}"
    );
    assert_eq!(lead["query"]["queries"][0]["lineHint"], 8, "{body}");
    let uri = lead["query"]["queries"][0]["path"].as_str().expect("path");
    assert!(
        std::path::Path::new(uri).is_absolute() && uri.ends_with("src/a.rs"),
        "{uri}"
    );

    let keyword = search_fixture(
        &[("src/a.rs", source)],
        ls_query(serde_json::json!({"matchString": "fn helper"}), None),
    );
    assert_eq!(
        keyword["next"]["verifyReferences"]["query"]["queries"][0]["lineHint"], 8,
        "{keyword}"
    );
    let insensitive = search_fixture(
        &[("src/a.rs", source)],
        ls_query(
            serde_json::json!({"matchString": "helper", "caseMode": "insensitive"}),
            None,
        ),
    );
    assert!(
        insensitive["next"].get("verifyReferences").is_none(),
        "{insensitive}"
    );

    // A run of hits in one declaration names it once, with its last
    // line: rows below it inside that range share it.
    let run =
        "fn f() {\n    helper();\n    helper();\n    helper();\n}\nstatic H: fn() = helper;\n";
    let body = search_fixture(
        &[("src/r.rs", run)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let rows = &body["files"][0]["matches"];
    assert_eq!(rows[0]["in"], "fn f@1-5", "{body}");
    for row in 1..4 {
        assert!(rows[row].get("in").is_none(), "{body}");
    }
    assert_eq!(rows[3]["line"], 6, "{body}");

    // A declaration search names the owner of declaring hits only: the
    // other hits are text that merely mentions the declaration.
    let mentions = "impl Harness {\n    fn try_read_output(&self) {}\n}\nfn caller() {\n    let s = \"fn try_read_output\";\n}\n";
    let body = search_fixture(
        &[("src/m.rs", mentions)],
        ls_query(
            serde_json::json!({"matchString": "fn try_read_output"}),
            None,
        ),
    );
    let rows = &body["files"][0]["matches"];
    assert_eq!(rows[0]["in"], "impl Harness@1-3", "{body}");
    assert_eq!(rows[1]["line"], 5, "{body}");
    assert!(rows[1].get("in").is_none(), "{body}");

    let sweep = format!("fn many() {{\n{}}}\n", "    helper();\n".repeat(60));
    let wide = search_fixture(
        &[("src/b.rs", &sweep)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    // A sweep names its owner too: once, on the first row of the run.
    let rows = wide["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 60, "{wide}");
    assert_eq!(rows[0]["in"], "fn many@1-62", "{wide}");
    assert!(
        rows[1..].iter().all(|row| row.get("in").is_none()),
        "{wide}"
    );
}

/// A result within the page budget is shown whole with no paging
/// metadata, however many hits it has; a larger one opens with 10 rows
/// per file, and a caller cap always wins.
#[test]
fn a_small_result_shows_every_hit_without_paging() {
    let small = "needle\n".repeat(13);
    let body = search_fixture(
        &[("a.txt", &small)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    let file = &body["files"][0];
    assert_eq!(file["matches"].as_array().map(Vec::len), Some(13), "{body}");
    assert!(file.get("pagination").is_none(), "{body}");
    assert!(body.get("pagination").is_none(), "{body}");
    assert!(body["next"].get("nextMatchPage").is_none(), "{body}");
    let many_files: Vec<(String, String)> = (0..25)
        .map(|n| (format!("f{n:02}.txt"), "needle\n".to_owned()))
        .collect();
    let refs: Vec<(&str, &str)> = many_files
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect();
    let spread = search_fixture(
        &refs,
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(
        spread["files"].as_array().map(Vec::len),
        Some(25),
        "all files on one page"
    );
    assert!(spread.get("pagination").is_none(), "{spread}");
    // Far more than one old match page, still well within the budget.
    let moderate = "needle\n".repeat(150);
    let body = search_fixture(
        &[("a.txt", &moderate)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(
        body["files"][0]["matches"].as_array().map(Vec::len),
        Some(150),
        "{body}"
    );
    assert!(body.get("next").is_none(), "{body}");
    let large = "needle\n".repeat(1200);
    let body = search_fixture(
        &[("a.txt", &large)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(
        body["files"][0]["matches"].as_array().map(Vec::len),
        Some(10)
    );
    assert!(body["next"]["nextPage"].is_object(), "{body}");
    assert!(body["next"].get("nextMatchPage").is_none(), "{body}");
    assert_eq!(body["files"][0]["pagination"]["hasMore"], true, "{body}");
    let capped = search_fixture(
        &[("a.txt", &small)],
        ls_query(
            serde_json::json!({"matchString": "needle", "matchPageSize": 10}),
            None,
        ),
    );
    assert_eq!(
        capped["files"][0]["matches"].as_array().map(Vec::len),
        Some(10)
    );
}

/// The first page of a complete result hands off one localFetch read: the
/// searched symbol's shown declaration whole, else the top file's hits
/// (±6-line ranges), so the next call reads instead of re-searching;
/// already-contextual results do not.
#[test]
fn a_small_complete_result_hands_off_a_read_of_its_top_file() {
    let body = search_fixture(
        &[("src/a.py", "x = 1\ny = needle\nprint(needle)\n")],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    let read = &body["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{body}");
    let query = &read["query"]["queries"][0];
    assert!(
        query["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("src/a.py")),
        "{body}"
    );
    // The read names the hits' ±6-line windows as ranges: exactly the
    // shown hits, whatever localFetch's matchString dialect would match.
    assert_eq!(query["ranges"], serde_json::json!(["1-9"]), "{body}");
    assert!(query.get("matchString").is_none(), "{body}");
    let mut runnable = query.clone();
    runnable["mainGoal"] = serde_json::json!("g");
    runnable["reasoning"] = serde_json::json!("r");
    crate::contracts::validate_query("localFetch", runnable).expect("valid localFetch read");
    let apart: String = (1..=40)
        .map(|line| {
            if line == 3 || line == 30 {
                "needle\n".to_owned()
            } else {
                "x\n".to_owned()
            }
        })
        .collect();
    let apart = search_fixture(
        &[("a.py", &apart)],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(
        apart["next"]["read"]["query"]["queries"][0]["ranges"],
        serde_json::json!(["1-9", "24-36"]),
        "{apart}"
    );
    // A declaration search reads the declaring file, wherever its
    // hit count ranks it.
    let mention = "s = 'def needle'\nt = 'def needle'\nu = 'def needle'\n";
    let declared = search_fixture(
        &[
            ("a.py", mention),
            ("b.py", "x = 1\ndef needle():\n    pass\n"),
        ],
        ls_query(serde_json::json!({"matchString": "def needle"}), None),
    );
    assert_eq!(declared["files"][0]["path"], "a.py", "{declared}");
    let read = &declared["next"]["read"]["query"]["queries"][0];
    assert!(
        read["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("b.py")),
        "{declared}"
    );
    // The declaration itself, whole.
    assert_eq!(read["ranges"], serde_json::json!(["2-3"]), "{declared}");
    // A declaration longer than one block read (a large class) is not read
    // whole: the read takes its hit in context, and localFetch offers the
    // member that window cuts.
    let long = format!("x = 1\ndef needle():\n{}", "    pass\n".repeat(450));
    let large = search_fixture(
        &[("b.py", &long)],
        ls_query(serde_json::json!({"matchString": "def needle"}), None),
    );
    assert_eq!(
        large["next"]["read"]["query"]["queries"][0]["ranges"],
        serde_json::json!(["1-8"]),
        "{large}"
    );
    // More windows than one ranges read holds: the read matches the text.
    let scattered: String = (1..=11)
        .map(|_| "needle_a\n".to_owned() + &"x\n".repeat(13))
        .collect();
    let regex = search_fixture(
        &[("a.py", &scattered)],
        ls_query(serde_json::json!({"matchString": "needle_(a|b)"}), None),
    );
    let read = &regex["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["matchString"], "needle_(a|b)", "{regex}");
    assert_eq!(read["contextLines"], 6, "{regex}");
    assert_eq!(read["regex"], "rust", "{regex}");
    let wide = search_fixture(
        &[
            ("a.py", "needle\n"),
            ("b.py", "needle\n"),
            ("c.py", "needle\n"),
            ("d.py", "needle\n"),
        ],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    // A wider result still reads its top file: the natural next step.
    assert!(
        wide["next"]["read"]["query"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.py")),
        "{wide}"
    );
    let context = search_fixture(
        &[("a.py", "x\nneedle\ny\n")],
        ls_query(
            serde_json::json!({"matchString": "needle", "contextLines": 1}),
            None,
        ),
    );
    assert!(
        context
            .get("next")
            .is_none_or(|next| next.get("read").is_none()),
        "{context}"
    );
}

fn all_values(body: &serde_json::Value) -> String {
    body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|f| f["matches"].as_array().cloned().unwrap_or_default())
        .map(|m| m["value"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

// A clipped value (matchOnly span, matchContentLength) must not
// leak a partial secret that no longer matches the secret patterns.
#[test]
fn clipped_values_never_leak_partial_secrets() {
    let token = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
    let file = format!("token: {token} end\n");
    let cases = [
        ls_query(
            serde_json::json!({"matchString": "ghp_[0-9a-z]{20}".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": "token".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly, "matchContentLength": 4}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": "end".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly, "matchContentLength": 2}),
            None,
        ),
        ls_query(
            serde_json::json!({"matchString": "token".to_string(), "matchContentLength": 30}),
            None,
        ),
    ];
    for request in cases {
        let body = search_fixture(&[("sec.txt", &file)], request);
        let values = all_values(&body);
        assert!(!values.is_empty());
        for fragment in ["1234567890", "klmnopqrstuvwxyz"] {
            assert!(!values.contains(fragment), "leaked {fragment}: {values}");
        }
    }
    // Clean lines in the same file keep their exact values.
    let body = search_fixture(
        &[("sec.txt", &format!("{file}plain needle line\n"))],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    assert_eq!(all_values(&body), "needle");
}

#[test]
fn short_spans_inside_private_key_bodies_are_redacted() {
    let file = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END RSA PRIVATE KEY-----\n";
    let body = search_fixture(
        &[("key.rs", file)],
        ls_query(
            serde_json::json!({"matchString": "interior".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    assert!(!all_values(&body).contains("interior"), "{body}");
}

/// Sources above the whole-file key-scan cap are verified by the streamed
/// prefix pass: late interior key bodies stay redacted (terminated or
/// not), while innocent base64 outside any block stays readable.
#[test]
fn late_key_bodies_in_large_sources_stay_redacted() {
    let filler = "let filler_value = 0;\n".repeat(560_000);
    let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyQWERTYAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let innocent = "QUJDREVGR0hJSktMTU5PUFFSU1RVinnocentB64VldYWVowMTIzNDU2Nzg5";
    let terminated = format!(
        "{innocent}\n{filler}-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\n"
    );
    let unterminated =
        format!("{innocent}\n{filler}-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n");
    for content in [&terminated, &unterminated] {
        assert!(content.len() as u64 > 10 * 1024 * 1024);
        for view in ["matchOnly", "detailed"] {
            let found = search_fixture(
                &[("big.txt", content.as_str())],
                ls_query(
                    serde_json::json!({"matchString": "interior", "resultView": view, "contextLines": 0}),
                    None,
                ),
            );
            assert_eq!(found["files"][0]["matches"][0]["line"], 560_003, "{found}");
            assert!(!all_values(&found).contains("interior"), "{found}");
        }
        let readable = search_fixture(
            &[("big.txt", content.as_str())],
            ls_query(
                serde_json::json!({"matchString": "innocentB64", "resultView": "detailed", "contextLines": 0}),
                None,
            ),
        );
        assert!(all_values(&readable).contains(innocent), "{readable}");
    }
}

#[test]
fn next_page_restarts_match_rows_and_next_match_page_tracks_shown_files() {
    let many = "foo\n".repeat(5);
    let files = [
        ("a.txt", many.as_str()),
        ("b.txt", "foo\n"),
        ("c.txt", many.as_str()),
    ];
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 2, "matchPage": 3, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    // Page 1 at matchPage 3: a.txt's 5th row is shown, nothing is left over.
    assert!(
        body["next"].get("nextMatchPage").is_none(),
        "{}",
        body["next"]
    );
    assert_eq!(
        body["next"]["nextPage"]["query"]["queries"][0]["matchPage"],
        1
    );
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 2, "page": 2, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    assert_eq!(
        body["next"]["nextMatchPage"]["query"]["queries"][0]["matchPage"],
        2
    );
}

/// A grid page names what each page reaches: `nextMatchPage` the rows
/// left in this page's files, `nextPage` the files after them. Row
/// totals live once, as `stats.matchedLines` beside the occurrence
/// count, and a later match page says which match page it shows.
#[test]
fn grid_pages_count_the_rows_a_match_page_reaches_and_name_the_match_page() {
    let many = "foo\n".repeat(5);
    let files = [
        ("a.txt", many.as_str()),
        ("b.txt", "foo foo\n"),
        ("c.txt", many.as_str()),
    ];
    let request = |match_page: u32| {
        ls_query(
            serde_json::json!({"matchString": "foo", "pageSize": 2, "matchPage": match_page, "matchPageSize": 2, "sort": LocalSearchQuerySort::Path}),
            None,
        )
    };
    let body = search_fixture(&files, request(1));
    // The shared page stage says how many rows `nextMatchPage` reaches.
    assert!(body.get("warnings").is_none(), "{body}");
    assert!(body["next"].get("nextMatchPage").is_some(), "{body}");
    assert!(body["pagination"].get("totalMatches").is_none(), "{body}");
    assert!(body["pagination"].get("matchPage").is_none(), "{body}");
    assert_eq!(body["stats"]["totalMatchedLines"], 11, "{body}");
    assert_eq!(body["stats"]["totalMatches"], 12, "{body}");
    let second = search_fixture(&files, request(2));
    assert_eq!(second["pagination"]["currentPage"], 1, "{second}");
    assert_eq!(second["pagination"]["matchPage"], 2, "{second}");
    assert!(second["next"].get("nextMatchPage").is_some(), "{second}");
}

#[test]
fn a_clipped_file_shows_its_deciding_hits_first_and_lists_the_rest() {
    // Comment and call hits come first by line; the assignment and the
    // branch that decide behaviour sit past the per-file cap.
    let mut file = String::new();
    for _ in 0..4 {
        file.push_str(" * maximumSize doc\n");
    }
    for _ in 0..3 {
        file.push_str("check(maximumSize);\n");
    }
    file.push_str("this.maximumSize = maximumSize;\n"); // line 8
    file.push_str("if (maximumSize > 0) {\n"); // line 9
    let request = |match_page| {
        ls_query(
            serde_json::json!({"matchString": "maximumSize", "matchPageSize": 3, "matchPage": match_page, "contextLines": 0}),
            None,
        )
    };
    let lines = |body: &serde_json::Value| -> Vec<u64> {
        body["files"][0]["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .filter_map(|m| m["line"].as_u64())
            .collect()
    };
    let first = search_fixture(&[("CacheBuilder.java", &file)], request(1));
    // Page 1 holds the best-ranked rows, shown in source order.
    assert_eq!(lines(&first), [5, 8, 9], "{first}");
    // The rows still unseen are named, cheap to read at their lines.
    assert_eq!(
        first["files"][0]["pagination"]["moreLines"], "1-4,6-7",
        "{first}"
    );
    let second = search_fixture(&[("CacheBuilder.java", &file)], request(2));
    assert_eq!(lines(&second), [1, 6, 7], "{second}");
    assert_eq!(second["files"][0]["pagination"]["moreLines"], "2-4");
    let last = search_fixture(&[("CacheBuilder.java", &file)], request(3));
    assert_eq!(lines(&last), [2, 3, 4], "{last}");
    assert!(last["files"][0].get("pagination").is_none(), "{last}");
}

#[test]
fn a_repeated_row_follows_every_distinct_row_of_a_clipped_file() {
    // Identical impls of one trait method rank as declarations, but the
    // second copy adds nothing over the distinct call site.
    let file = "fn needle() {\nfn needle() {\ncall(needle);\n";
    let body = search_fixture(
        &[("a.rs", file)],
        ls_query(
            serde_json::json!({"matchString": "needle", "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    let lines: Vec<u64> = body["files"][0]["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .filter_map(|m| m["line"].as_u64())
        .collect();
    assert_eq!(lines, [1, 3], "{body}");
    assert_eq!(body["files"][0]["pagination"]["moreLines"], "2");
}

#[test]
fn line_rows_omit_the_column_that_span_rows_need() {
    let files = [("a.txt", "x needle needle\n")];
    let lines = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert!(
        lines["files"][0]["matches"][0].get("column").is_none(),
        "{lines}"
    );
    let spans = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "needle", "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    let columns: Vec<u64> = spans["files"][0]["matches"]
        .as_array()
        .expect("spans")
        .iter()
        .filter_map(|m| m["column"].as_u64())
        .collect();
    assert_eq!(columns, [2, 9], "{spans}");
}

#[test]
fn later_match_pages_omit_files_exhausted_on_earlier_pages() {
    let many = "foo\n".repeat(5);
    let files = [("a.txt", many.as_str()), ("b.txt", "foo\n")];
    let request = |match_page| {
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPage": match_page, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        )
    };
    let body = search_fixture(&files, request(2));
    let paths: Vec<&str> = body["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();
    assert_eq!(paths, ["a.txt"], "{body}");
    // Every file exhausted: keep the out-of-range diagnostic rows.
    let body = search_fixture(&files, request(9));
    assert_eq!(body["files"][0]["pagination"]["outOfRange"], true, "{body}");
}

#[test]
fn list_views_have_no_match_rows_to_page() {
    let body = search_fixture(
        &[("a.txt", &"foo\n".repeat(15))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "resultView": LocalSearchQueryResultView::Files}),
            None,
        ),
    );
    assert!(body.get("next").is_none(), "{}", body["next"]);
    assert_ne!(body["status"], "partial", "{body}");
}

#[test]
fn binary_prefix_matches_mark_a_search_partial_and_terminal() {
    // Matches before the NUL do not prove the rest of the file was
    // searched: the cut is a coverage limit no continuation can lift.
    let body = search_fixture(
        &[("bin.dat", "foo\u{0}foo\n"), ("a.txt", "foo\n")],
        ls_query(serde_json::json!({"matchString": "foo".to_string()}), None),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert!(body["next"].get("nextPage").is_none(), "{body}");
    assert_eq!(body["stats"]["capReason"], "binaryQuit");
    // capped agrees with capReason instead of contradicting it.
    assert_eq!(body["stats"]["capped"], true, "{body}");
    // localFetch rejects binary files, so a binary file is never the read:
    // the read takes the shown text hit.
    let read = &body["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{body}");
    assert_eq!(
        read["query"]["queries"][0]["path"]
            .as_str()
            .map(|path| path.ends_with("a.txt")),
        Some(true),
        "{body}"
    );
    assert!(!body.to_string().contains("bin.dat\",\"ranges"), "{body}");
}

#[test]
fn reverse_applies_to_default_relevance_order() {
    let files = [("a.txt", "foo\nfoo\nfoo\n"), ("b.txt", "foo\n")];
    let order = |reverse| {
        let body = search_fixture(
            &files,
            ls_query(
                serde_json::json!({"matchString": "foo".to_string(), "reverse": reverse}),
                None,
            ),
        );
        body["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(None), ["a.txt", "b.txt"]);
    assert_eq!(order(Some(true)), ["b.txt", "a.txt"]);
}

fn ranked_paths(files: &[(&str, &str)], fields: serde_json::Value) -> Vec<String> {
    let body = search_fixture(files, ls_query(fields, None));
    body["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn relevance_ranks_a_declaration_hit_above_a_comment_or_string_hit_at_equal_count() {
    // Path order alone would put the comment and string hits first.
    let files = [
        ("a_comment.rs", "// parse_config reads the file\n"),
        ("b_string.rs", "let label = \"parse_config\";\n"),
        ("c_call.rs", "let cfg = parse_config(path);\n"),
        ("d_decl.rs", "pub fn parse_config(path: &str) -> Config {\n"),
    ];
    for view in ["paginated", "countMatches", "countLines", "matchOnly"] {
        assert_eq!(
            ranked_paths(
                &files,
                serde_json::json!({"matchString": "parse_config", "resultView": view}),
            ),
            ["d_decl.rs", "c_call.rs", "a_comment.rs", "b_string.rs"],
            "view {view}"
        );
    }
}

#[test]
fn relevance_ranks_a_source_file_above_test_and_generated_files_at_equal_count() {
    let hit = "let cfg = parse_config(path);\n";
    let files = [
        ("a/tests/config.rs", hit),
        ("b/config_test.go", hit),
        ("c/config.test.ts", hit),
        ("d/generated/config.rs", hit),
        ("e/vendor/config.go", hit),
        ("z/src/config.rs", hit),
    ];
    let ranked = ranked_paths(&files, serde_json::json!({"matchString": "parse_config"}));
    assert_eq!(ranked[0], "z/src/config.rs", "{ranked:?}");
    // The demoted rest keeps a total order: path breaks the tie, and a
    // generated file follows every hand-written one.
    assert_eq!(
        &ranked[1..],
        [
            "a/tests/config.rs",
            "b/config_test.go",
            "c/config.test.ts",
            "e/vendor/config.go",
            "d/generated/config.rs",
        ]
    );
    // Match count still dominates: two test hits beat one source hit.
    let ranked = ranked_paths(
        &[
            ("a/tests/config.rs", "parse_config();\nparse_config();\n"),
            ("z/src/config.rs", hit),
        ],
        serde_json::json!({"matchString": "parse_config"}),
    );
    assert_eq!(ranked, ["a/tests/config.rs", "z/src/config.rs"]);
}

#[test]
fn relevance_on_path_list_views_demotes_test_paths_and_path_sort_does_not() {
    let hit = "parse_config();\n";
    let files = [("a/tests/config.rs", hit), ("z/src/config.rs", hit)];
    let relevance = ranked_paths(
        &files,
        serde_json::json!({"matchString": "parse_config", "resultView": "files"}),
    );
    assert_eq!(relevance, ["z/src/config.rs", "a/tests/config.rs"]);
    let by_path = ranked_paths(
        &files,
        serde_json::json!({"matchString": "parse_config", "resultView": "files", "sort": "path"}),
    );
    assert_eq!(by_path, ["a/tests/config.rs", "z/src/config.rs"]);
    let without = ranked_paths(
        &[("a/tests/other.rs", "x\n"), ("z/src/other.rs", "x\n")],
        serde_json::json!({"matchString": "parse_config", "resultView": "filesWithout"}),
    );
    assert_eq!(without, ["z/src/other.rs", "a/tests/other.rs"]);
}

// Lean defaults: the paginated view returns only the hit line, 10 rows per
// file page and 20 files per page; `detailed` keeps a ±3 context window.
#[test]
fn default_paginated_view_is_lean_and_detailed_keeps_context() {
    let file = numbered(30, &(1..=12).collect::<Vec<_>>());
    let many: Vec<(String, String)> = (0..25)
        .map(|i| {
            (
                format!("f{i:02}.txt"),
                "line 1 needle\nline 2 needle\n".to_owned(),
            )
        })
        .collect();
    // A hot last file puts the result over one page budget.
    let hot = "needle\n".repeat(1200);
    let mut fixtures: Vec<(&str, &str)> = vec![("a.txt", file.as_str()), ("z.txt", &hot)];
    fixtures.extend(many.iter().map(|(p, c)| (p.as_str(), c.as_str())));
    let body = search_fixture(
        &fixtures,
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    let files = body["files"].as_array().expect("files");
    assert_eq!(files.len(), 20, "{body}");
    let a = &files[0];
    assert_eq!(a["matches"].as_array().expect("rows").len(), 10, "{body}");
    assert_eq!(a["matches"][0]["value"], "line 1 needle");
    assert_eq!(a["pagination"]["totalItems"], 12);
    let detailed = search_fixture(
        &[("a.txt", &numbered(30, &[20]))],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::Detailed}),
            None,
        ),
    );
    assert_eq!(
        detailed["files"][0]["matches"][0]["value"],
        "17\tline 17\n18\tline 18\n19\tline 19\n20\tline 20 needle\n21\tline 21\n22\tline 22\n23\tline 23"
    );
}

/// Following `nextPage` from a default search walks every row exactly
/// once in pages cut by the response budget: a hot file is not paged ten
/// rows at a time, and each continuation runs unchanged.
#[test]
fn a_default_walk_reaches_every_row_once_in_budget_sized_pages() {
    let root = tempfile::tempdir().expect("fixture directory");
    let rows_per_file = 60;
    for n in 0..30 {
        let body = (1..=rows_per_file)
            .map(|line| format!("needle {n} {line} {}\n", "x".repeat(60)))
            .collect::<String>();
        fs::write(root.path().join(format!("f{n:02}.txt")), body).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let mut request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
            .expect("page");
        let body = serde_json::to_value(&result).expect("serialize");
        pages += 1;
        let chars = crate::tools::stream_page::json_chars(&body["files"]);
        assert!(
            chars <= crate::tools::stream_page::MAX_PAGE_CHARS,
            "page {pages}: {chars} chars"
        );
        for file in body["files"].as_array().expect("files") {
            for row in file["matches"].as_array().expect("rows") {
                let key = (
                    file["path"].as_str().expect("path").to_owned(),
                    row["line"].as_u64().expect("line"),
                );
                assert!(seen.insert(key), "row shown twice on page {pages}");
            }
        }
        assert!(
            body["next"].get("nextMatchPage").is_none(),
            "{}",
            body["next"]
        );
        let Some(next) = body["next"].get("nextPage") else {
            break;
        };
        assert!(
            next["query"]["queries"][0].get("pageSize").is_none(),
            "{next}"
        );
        request = serde_json::from_value(next["query"]["queries"][0].clone())
            .expect("continuation query");
    }
    assert_eq!(seen.len(), 30 * rows_per_file);
    // About 30 × 60 rows of ~100 bytes: the overview page plus
    // total/budget pages, not one page per ten rows of each file.
    assert!((7..=11).contains(&pages), "{pages} pages");
}

fn numbered(lines: u32, needles: &[u32]) -> String {
    (1..=lines)
        .map(|n| {
            if needles.contains(&n) {
                format!("line {n} needle\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect()
}

// Overlapping/adjacent ±contextLines windows are emitted once: every source
// line appears exactly once, in order, and every matched line is recorded.
#[test]
fn overlapping_context_windows_merge_into_one_block() {
    let file = numbered(30, &[5, 7, 12, 25]);
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 2}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    // 5 and 7 overlap (3..=9), 12 is adjacent (10..=14 follows 9), 25 is apart.
    assert_eq!(matches.len(), 2, "{body}");
    assert_eq!(matches[0]["line"], 5);
    assert_eq!(matches[0]["matchLines"], serde_json::json!([5, 7, 12]));
    let expected: String = (3..=14)
        .map(|n| {
            if [5, 7, 12].contains(&n) {
                format!("{n}\tline {n} needle")
            } else {
                format!("{n}\tline {n}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(matches[0]["value"], expected);
    assert_eq!(matches[1]["line"], 25);
    assert!(matches[1].get("matchLines").is_none(), "{body}");
    assert_eq!(
        matches[1]["value"],
        "23\tline 23\n24\tline 24\n25\tline 25 needle\n26\tline 26\n27\tline 27"
    );
    // Counts stay per matched line.
    assert_eq!(body["stats"]["totalMatchedLines"], 4);
}

#[test]
fn windows_merge_at_file_edges_and_stay_apart_when_disjoint() {
    let file = numbered(6, &[1, 2, 6]);
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 1}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2, "{body}");
    assert_eq!(
        matches[0]["value"],
        "1\tline 1 needle\n2\tline 2 needle\n3\tline 3"
    );
    assert_eq!(matches[0]["matchLines"], serde_json::json!([1, 2]));
    assert_eq!(matches[1]["value"], "5\tline 5\n6\tline 6 needle");
    // matchOnly never merges: it carries spans, not windows.
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 3, "{body}");
    assert!(matches.iter().all(|m| m.get("matchLines").is_none()));
}

// A truncated window must never be merged (its line structure is unknown).
#[test]
fn truncated_windows_are_not_merged() {
    let long = "x".repeat(200);
    let file = format!("{long}\nneedle a\n{long}\nneedle b\n{long}\n");
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "matchContentLength": 100}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2, "{body}");
    assert!(matches.iter().all(|m| m.get("matchLines").is_none()));
}

// Single-page results carry no redundant accounting: no engine constant,
// no pagination block, no per-file row counters or per-file pagination
// unless that file has more match pages.
#[test]
fn single_page_results_omit_redundant_accounting() {
    let body = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", &"foo\n".repeat(3))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    assert!(body.get("searchEngine").is_none(), "{body}");
    assert!(body.get("pagination").is_none(), "{body}");
    let files = body["files"].as_array().expect("files");
    for file in files {
        assert!(file.get("totalMatchRows").is_none(), "{file}");
        assert!(file.get("returnedMatchRows").is_none(), "{file}");
    }
    let b = files.iter().find(|f| f["path"] == "b.txt").expect("b");
    let a = files.iter().find(|f| f["path"] == "a.txt").expect("a");
    assert!(a.get("pagination").is_none(), "{a}");
    assert_eq!(b["pagination"]["hasMore"], true);
    assert_eq!(b["pagination"]["totalItems"], 3);
    assert!(b["pagination"].get("pageSize").is_none(), "{b}");
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert!(next["snapshot"].is_string(), "{body}");
    // Multi-page file results still carry file pagination.
    let body = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", "foo\n")],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 1}),
            None,
        ),
    );
    assert_eq!(body["pagination"]["totalPages"], 2, "{body}");
    assert!(
        body["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string(),
        "{body}"
    );
}

// Continuations carry only the caller's window fields; the next call
// derives the view's defaults (the detailed view's 3 context lines) the
// same way, so its snapshot still matches.
#[test]
fn continuations_derive_the_view_window_defaults() {
    let file = "foo\n".repeat(3);
    let make = |view| {
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "resultView": view, "matchPageSize": 1}),
            None,
        )
    };
    let body = search_fixture(
        &[("a.txt", &file)],
        make(LocalSearchQueryResultView::MatchOnly),
    );
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert!(next.get("contextLines").is_none(), "{next}");
    let sent = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "foo", "matchPageSize": 1, "contextLines": 0, "matchContentLength": 300}),
            None,
        ),
    );
    let next = &sent["next"]["nextMatchPage"]["query"]["queries"][0];
    assert_eq!(next["contextLines"], 0, "{next}");
    assert_eq!(next["matchContentLength"], 300, "{next}");

    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.txt"), &file).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
        Some(&make(LocalSearchQueryResultView::Detailed)),
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("first page");
    let body = serde_json::to_value(&first).expect("serialize");
    let next = body["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    // The continuation re-derives the view's defaults: neither the
    // context window nor the value width is repeated in it.
    assert!(next.get("contextLines").is_none(), "{next}");
    assert!(next.get("matchContentLength").is_none(), "{next}");
    let mut continued: LocalSearchQuery =
        serde_json::from_value(next).expect("continuation parses");
    continued.path = request.path.clone();
    execute_local_search(&continued, &policy, &security, &NeverCancel, None, None)
        .expect("detailed continuation is not stale");
}

/// Search `root` with `fields` (path set to `root`), default security and
/// no cancellation.
fn search_root(root: &std::path::Path, fields: serde_json::Value) -> LocalSearchResult {
    let (policy, security) = policy_for(root);
    let mut fields = fields;
    fields["path"] = root.to_string_lossy().into_owned().into();
    let request = ls_query(fields, None);
    execute_local_search(&request, &policy, &security, &NeverCancel, None, None).expect("search")
}

/// Run a structureSearch row (a lead's query) and return its JSON text.
fn list_structure(
    mut query: serde_json::Value,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> String {
    query["mainGoal"] = serde_json::json!("test");
    query["reasoning"] = serde_json::json!("test");
    let query: crate::tools::structure_search::StructureSearchQuery =
        serde_json::from_value(query).expect("structureSearch query");
    crate::tools::structure_search::execute_structure(&query, policy, security, &NeverCancel, None)
        .expect("listing runs")
        .to_string()
}

/// Page 1 of `request`, its snapshot, and the page-2 query that resumes it.
fn first_page(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> (LocalSearchResult, String, LocalSearchQuery) {
    let first =
        execute_local_search(request, policy, security, &NeverCancel, None, None).expect("page 1");
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

fn policy_for(root: &std::path::Path) -> (PathPolicy, ContentSecurity) {
    let policy = crate::tools::test_support::workspace_policy(root);
    (policy, ContentSecurity::new())
}

/// Make `dir` unreadable; returns false when the process can still read it
/// (e.g. running as root), so the caller skips.
#[cfg(unix)]
fn lock_dir(dir: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o000)).expect("chmod 000");
    if fs::read_dir(dir).is_ok() {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).expect("restore");
        return false;
    }
    true
}

/// `root/hidden/secret.txt` holding the only `needle-only-here`, with
/// `hidden` made unreadable; `None` when the process can still read it.
#[cfg(unix)]
fn locked_secret(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let hidden = root.join("hidden");
    fs::create_dir(&hidden).expect("dir");
    fs::write(hidden.join("secret.txt"), "needle-only-here\n").expect("fixture");
    lock_dir(&hidden).then_some(hidden)
}

#[cfg(unix)]
fn unlock_dir(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
}

#[cfg(unix)]
#[test]
fn a_scope_where_every_path_is_unreadable_is_an_access_error_not_empty() {
    let root = tempfile::tempdir().expect("fixture directory");
    let Some(hidden) = locked_secret(root.path()) else {
        return;
    };
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle-only-here".to_string(), "regex": LocalSearchQueryRegex::Literal, "noIgnore": true}),
        None,
    );
    let outcome = execute_local_search(&request, &policy, &security, &NeverCancel, None, None);
    unlock_dir(&hidden);
    let error = outcome.expect_err("every candidate failed");
    assert_eq!(error.code, "fileAccessFailed");
    assert!(
        error.message.contains("Permission denied"),
        "{}",
        error.message
    );
    assert!(error.hints.iter().any(|h| h.contains("permissions")));
    assert!(error.hints.iter().all(|h| !h.contains("caseMode")));
}

#[cfg(unix)]
#[test]
fn unreadable_paths_beside_readable_misses_make_the_result_partial() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("open.txt"), "nothing here\n").expect("fixture");
    let Some(hidden) = locked_secret(root.path()) else {
        return;
    };
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle-only-here".to_string(), "regex": LocalSearchQueryRegex::Literal, "noIgnore": true}),
        None,
    );
    let outcome = execute_local_search(&request, &policy, &security, &NeverCancel, None, None);
    unlock_dir(&hidden);
    let result = outcome.expect("readable file searched");
    assert_eq!(result.status, SearchStatus::Partial);
    let body = serde_json::to_value(&result).expect("serialize");
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert_eq!(body["stats"]["errorCount"], 1, "{body}");
    let hint = body["hints"][0].as_str().expect("hint");
    assert!(hint.contains("permissions"), "{hint}");
    assert!(!hint.contains("caseMode"), "{hint}");
}

#[test]
fn a_nul_keeps_the_matches_before_it_and_warns() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
        root.path().join("mixed.txt"),
        b"alpha before\0alpha after\nalpha before\n",
    )
    .expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
    );
    assert_ne!(result.status, SearchStatus::Empty);
    let body = serde_json::to_value(&result).expect("serialize");
    assert_eq!(body["files"][0]["matches"][0]["line"], 1, "{body}");
    assert_eq!(body["stats"]["totalMatches"], 1, "{body}");
    // The warning names the file it could not search past the NUL.
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.starts_with("binaryFileSkipped: mixed.txt was searched")),
        "{:?}",
        result.warnings
    );
}

/// Run each `expandValues*` read of a search result and return the
/// concatenated content it fetched.
fn follow_expansions(result: &LocalSearchResult, policy: &PathPolicy) -> (usize, String) {
    let next = result.next.clone().unwrap_or_default();
    let mut reads = 0;
    let mut content = String::new();
    for (name, read) in next.as_object().into_iter().flatten() {
        if !name.starts_with("expandValues") {
            continue;
        }
        reads += 1;
        assert_eq!(read["tool"], "localFetch", "{read}");
        let mut query = read["query"]["queries"][0].clone();
        query["mainGoal"] = serde_json::json!("test");
        query["reasoning"] = serde_json::json!("test");
        let query: crate::tools::local_fetch::LocalFetchQuery =
            serde_json::from_value(query).expect("localFetch query");
        let fetched = crate::tools::local_fetch::execute_local_fetch(
            &query,
            policy,
            &ContentSecurity::new(),
            &NeverCancel,
            &crate::tools::local_fetch::LocalFetchRegex::default(),
            None,
        );
        assert_eq!(fetched.error, None, "{read}");
        content.push_str(fetched.content.as_deref().unwrap_or(""));
    }
    (reads, content)
}

/// A value clipped to matchContentLength carries `next.expandValues`:
/// a read of its source lines that returns it whole, one per file.
#[test]
fn clipped_values_carry_reads_that_return_them_whole() {
    let root = tempfile::tempdir().expect("fixture directory");
    let long = format!("{} alpha {}", "x".repeat(3000), "y".repeat(3000));
    fs::write(
        root.path().join("long.txt"),
        format!("short alpha\n{long}\nalpha z\n"),
    )
    .expect("fixture");
    let other = format!("alpha {}", "q".repeat(900));
    fs::write(root.path().join("other.txt"), format!("{other}\n")).expect("fixture");
    fs::write(root.path().join("plain.txt"), "alpha\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha", "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");
    assert!(body.to_string().contains("\"truncated\":true"), "{body}");
    let (reads, content) = follow_expansions(&result, &policy);
    assert_eq!(reads, 2, "{body}");
    assert!(content.contains(&long), "{content:.300}");
    assert!(content.contains(&other), "{content:.300}");
    // An unclipped file needs no read.
    assert!(!content.contains("plain"), "{content:.300}");
    let next = result.next.clone().unwrap_or_default();
    let ranges = next
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name.starts_with("expandValues"))
        .map(|(_, read)| read["query"]["queries"][0]["ranges"].clone())
        .collect::<Vec<_>>();
    assert!(ranges.contains(&serde_json::json!(["2-2"])), "{ranges:?}");
    assert!(ranges.contains(&serde_json::json!(["1-1"])), "{ranges:?}");
}

/// Clipped rows with context read their whole windows; overlapping
/// windows merge, and more than ten ranges continue in a second read.
#[test]
fn clipped_context_windows_merge_and_split_across_reads() {
    let root = tempfile::tempdir().expect("fixture directory");
    let mut text = String::new();
    for line in 1..=200 {
        if line % 10 == 0 {
            text.push_str(&format!("alpha {}\n", "w".repeat(400)));
        } else {
            text.push_str(&format!("line {line}\n"));
        }
    }
    fs::write(root.path().join("many.txt"), &text).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha",
                "regex": LocalSearchQueryRegex::Literal, "contextLines": 1, "matchContentLength": 100}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    let next = result.next.clone().unwrap_or_default();
    assert_eq!(
        next["expandValues"]["query"]["queries"][0]["ranges"][0], "9-11",
        "{next}"
    );
    assert_eq!(
        next["expandValues"]["query"]["queries"][0]["ranges"]
            .as_array()
            .map(Vec::len),
        Some(10),
        "{next}"
    );
    assert_eq!(
        next["expandValues2"]["query"]["queries"][0]["ranges"],
        serde_json::json!([
            "109-111", "119-121", "129-131", "139-141", "149-151", "159-161", "169-171", "179-181",
            "189-191", "199-201"
        ]),
        "{next}"
    );
    let (_, content) = follow_expansions(&result, &policy);
    let hit = format!("alpha {}", "w".repeat(400));
    assert_eq!(content.matches(hit.as_str()).count(), 20, "{content:.300}");
    for line in (10..=190).step_by(10) {
        assert!(content.contains(&format!("line {}\n", line + 1)), "{line}");
    }
}

/// A caller-sized page keeps its rows at any value width: one widened
/// copy of the query returns every clipped value of the page whole.
#[test]
fn clipped_values_on_a_grid_page_widen_the_same_page() {
    let root = tempfile::tempdir().expect("fixture directory");
    for index in 0..4 {
        fs::write(
            root.path().join(format!("f{index}.txt")),
            format!("alpha {}\n", "v".repeat(300 + index)),
        )
        .expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha",
                "regex": LocalSearchQueryRegex::Literal, "pageSize": 2, "matchContentLength": 50}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");
    let next = result.next.clone().unwrap_or_default();
    let mut names = next
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    names.sort();
    // The first page also reads its top file (the natural next step).
    assert_eq!(names, ["expandValues", "nextPage", "read"], "{next}");
    // The snapshot names the value width: the widened page runs fresh.
    assert!(
        next["expandValues"]["query"]["queries"][0]
            .get("snapshot")
            .is_none(),
        "{next}"
    );
    let widened = &next["expandValues"]["query"]["queries"][0];
    assert_eq!(next["expandValues"]["tool"], "localSearch", "{next}");
    assert_eq!(widened["page"], 1, "{widened}");
    let longest = body["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["matches"][0]["originalChars"].as_u64())
        .max()
        .expect("clipped");
    assert_eq!(widened["matchContentLength"], longest, "{widened}");
    let mut widened = widened.clone();
    widened["mainGoal"] = serde_json::json!("test");
    widened["reasoning"] = serde_json::json!("test");
    let again = execute_local_search(
        &serde_json::from_value(widened).expect("widened query"),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
    )
    .expect("widened search");
    let again = serde_json::to_value(&again).expect("serialize");
    assert!(!again.to_string().contains("\"truncated\":true"), "{again}");
    let paths = |value: &serde_json::Value| {
        value["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| file["path"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(paths(&again), paths(&body), "the same rows, whole");
}

/// A clipped multiline value hides its line span: its file is searched
/// again alone with room for the longest clipped value.
#[test]
fn clipped_multiline_values_widen_a_file_scoped_search() {
    let root = tempfile::tempdir().expect("fixture directory");
    let body = (0..60).map(|n| format!("  item{n},\n")).collect::<String>();
    fs::write(
        root.path().join("list.js"),
        format!("const list = [\n{body}];\n"),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(),
                "matchString": "const list = \\[[^\\]]*\\]", "multiline": "on"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    let body_json = serde_json::to_value(&result).expect("serialize");
    assert!(
        body_json.to_string().contains("\"truncated\":true"),
        "{body_json}"
    );
    let next = result.next.clone().unwrap_or_default();
    let widened = &next["expandValues"]["query"]["queries"][0];
    assert_eq!(next["expandValues"]["tool"], "localSearch", "{next}");
    assert_eq!(widened["path"], "list.js", "{next}");
    let original = body_json["files"][0]["matches"][0]["originalChars"]
        .as_u64()
        .expect("originalChars");
    assert_eq!(widened["matchContentLength"], original, "{next}");
    for key in ["page", "matchPage", "snapshot", "pageSize"] {
        assert!(widened.get(key).is_none(), "{key}: {widened}");
    }
    let mut widened = widened.clone();
    widened["path"] = serde_json::json!(root.path().join("list.js").to_string_lossy());
    let again = execute_local_search(
        &ls_query(widened, None),
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
    )
    .expect("widened search");
    let again = serde_json::to_value(&again).expect("serialize");
    assert!(!again.to_string().contains("\"truncated\":true"), "{again}");
    assert!(again.to_string().contains("item59"), "{again}");
}

/// Every file cut at a NUL after real text is a coverage gap: the
/// warning names each one, files sharing a directory grouped under it,
/// and none summarized as a count.
#[test]
fn every_text_cut_file_is_named_grouped_by_directory() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("deep/dir")).expect("fixture dir");
    for index in 0..8 {
        fs::write(
            root.path().join(format!("deep/dir/mixed{index}.txt")),
            b"alpha before\0alpha after\n",
        )
        .expect("fixture");
    }
    fs::write(root.path().join("top.txt"), b"alpha before\0alpha after\n").expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
    );
    assert!(result.is_partial);
    let warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("binaryFileSkipped:"))
        .expect("binary warning");
    let names = (0..8)
        .map(|index| format!("mixed{index}.txt"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        warning.starts_with(&format!(
            "binaryFileSkipped: top.txt, deep/dir/{{{names}}} were searched"
        )),
        "{warning}"
    );
    assert!(!warning.contains("more"), "{warning}");
}

/// Past a handful, text-cut binary files are a count by extension with a
/// listing that names every one (`binarySkipped`), not an unbounded
/// inline warning: a build tree of `.rlib`s stays a short row.
#[test]
fn many_text_cut_files_are_a_count_with_a_lossless_listing() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("deps")).expect("fixture dir");
    for index in 0..300 {
        fs::write(
            root.path()
                .join(format!("deps/libcrate_with_a_long_name_{index:03}.rlib")),
            b"alpha before\0alpha after\n",
        )
        .expect("fixture");
    }
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "alpha", "regex": "literal", "resultView": "files", "defaultExcludes": false}),
        None,
    );
    let (policy, security) = policy_for(root.path());
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    let warning = result
        .warnings
        .iter()
        .find(|w| w.starts_with("binaryFileSkipped:"))
        .expect("binary warning");
    assert!(
        warning.starts_with("binaryFileSkipped: 300 files (.rlib 300; structureSearch"),
        "{warning}"
    );
    assert!(!warning.contains("libcrate"), "{warning}");
    let listing = result
        .next
        .as_ref()
        .and_then(|next| next.get("binarySkipped"))
        .expect("listing lead");
    assert_eq!(
        listing["query"]["queries"][0]["extensions"],
        serde_json::json!(["rlib"])
    );
    assert!(warning.len() < 200, "{warning}");
}

/// Files binary from their leading bytes (a font's magic, then a NUL)
/// are outside a text search, as rg skips them: no partial result, a
/// count grouped by extension instead of every path, and an exact
/// structureSearch continuation that lists them all.
#[test]
fn leading_nul_binaries_are_counted_with_a_listing_continuation() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("fonts/a")).expect("fixture dir");
    for index in 0..30 {
        fs::write(
            root.path()
                .join(format!("fonts/a/Font-{index}-0123456789abcdef.woff2")),
            b"wOF2\0\x01\0\0alpha",
        )
        .expect("fixture");
    }
    fs::write(
        root.path().join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .expect("fixture");
    fs::write(root.path().join("a.txt"), "alpha\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");
    assert!(!result.is_partial, "{body}");
    assert!(!result.terminal_limit, "{body}");
    assert_eq!(result.status, SearchStatus::Success, "{body}");
    assert_eq!(result.stats.cap_reason, None, "{body}");
    assert_eq!(
        result.warnings,
        [
            "binarySkipped: 31 binary files not searched (.woff2 30, .png 1); structureSearch operation:\"files\" with these extensions lists them."
        ],
        "{body}"
    );
    let text = body.to_string();
    assert!(!text.contains("Font-0"), "{text}");

    let listing = &body["next"]["binarySkipped"];
    assert_eq!(listing["tool"], "structureSearch", "{body}");
    let query = listing["query"]["queries"][0].clone();
    assert_eq!(query["operation"], "files", "{listing}");
    assert_eq!(
        query["extensions"],
        serde_json::json!(["woff2", "png"]),
        "{listing}"
    );
    let listed = list_structure(query, &policy, &security);
    for index in 0..30 {
        assert!(
            listed.contains(&format!("Font-{index}-0123456789abcdef.woff2")),
            "{listed}"
        );
    }
    assert!(listed.contains("logo.png"), "{listed}");
    assert!(!listed.contains("a.txt"), "{listed}");
}

/// An extensionless binary cannot be named by `extensions`, so the
/// listing continuation matches basenames instead and still reaches it.
#[test]
fn an_extensionless_leading_nul_binary_is_listed_by_basename() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("store"), b"SQLite format 3\0alpha").expect("fixture");
    fs::write(root.path().join("x.woff2"), b"wOF2\0\x01\0\0alpha").expect("fixture");
    fs::write(root.path().join("README"), "plain text\n").expect("fixture");
    fs::write(root.path().join("a.txt"), "alpha\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "alpha".to_string(), "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert!(!result.is_partial);
    assert!(
        result.warnings[0]
            .starts_with("binarySkipped: 2 binary files not searched (no extension 1, .woff2 1)"),
        "{:?}",
        result.warnings
    );
    let query = result.next.as_ref().expect("next")["binarySkipped"]["query"]["queries"][0].clone();
    assert!(query.get("extensions").is_none(), "{query}");
    let listed = list_structure(query, &policy, &security);
    assert!(listed.contains("store"), "{listed}");
    assert!(listed.contains("x.woff2"), "{listed}");
    assert!(!listed.contains("a.txt"), "{listed}");
}

#[test]
fn opaque_binary_files_in_scope_do_not_make_a_search_partial() {
    // An object-file header puts a NUL before any text: rg skips such
    // files and nothing text-searchable is lost, so only their count is
    // disclosed.
    let body = search_fixture(
        &[
            ("addon.node", "\u{7f}ELF\u{2}\u{1}\u{1}\u{0}needle\n"),
            ("a.txt", "needle\n"),
        ],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string()}),
            None,
        ),
    );
    assert_ne!(body["isPartial"], true, "{body}");
    assert_ne!(body["terminalLimit"], true, "{body}");
    assert_ne!(body["status"], "partial", "{body}");
    assert_eq!(
        body["warnings"],
        serde_json::json!([
            "binarySkipped: 1 binary file not searched (.node 1); structureSearch operation:\"files\" with these extensions lists it."
        ]),
        "{body}"
    );
    assert!(
        body["stats"]
            .get("capReason")
            .is_none_or(serde_json::Value::is_null),
        "{body}"
    );

    // Targeted directly, the empty result still says why.
    let root = tempfile::tempdir().expect("fixture directory");
    let target = root.path().join("addon.node");
    fs::write(&target, b"\x7fELF\x02\x01\x01\0needle\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": target.to_string_lossy().into_owned(), "matchString": "needle".to_string()}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert!(
        result.hints.iter().any(|h| h.contains("binary")),
        "{:?}",
        result.hints
    );
}

#[test]
fn an_empty_result_with_a_binary_cut_is_partial_not_empty() {
    let body = search_fixture(
        &[(
            "blob.dat",
            "a text line\nanother\n\u{0}needle after the nul\n",
        )],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string()}),
            None,
        ),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert!(
        body["hints"].as_array().is_some_and(|hints| hints
            .iter()
            .any(|h| h.as_str().is_some_and(|h| h.contains("NUL byte")))),
        "{body}"
    );
}

#[test]
fn relevance_cap_keeps_hot_file_and_marks_first_and_final_pages_partial() {
    let root = tempfile::tempdir().expect("fixture directory");
    for i in 0..=10_000 {
        fs::write(root.path().join(format!("a{i:05}.txt")), "hit\n").expect("fixture");
    }
    fs::write(root.path().join("zzz-hot.txt"), "hit\n".repeat(40)).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "hit".to_string(), "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert_eq!(result.files[0].path, "zzz-hot.txt");
    let stats = &result.stats;
    assert_eq!(stats.files_searched, 10_002);
    assert_eq!(stats.files_matched, 10_002);
    assert_eq!(stats.total_occurrences, 10_001 + 40);
    assert!(result.is_partial);
    assert_eq!(result.status, SearchStatus::Partial);
    assert!(
        stats
            .cap_reason
            .as_deref()
            .is_some_and(|r| r.contains("maxCollectedFiles")),
        "{stats:?}"
    );
    let last_page = result.pagination.as_ref().expect("file pages").total_pages;
    let last_request = ls_query(
        serde_json::json!({"page": last_page, "snapshot": result.source_snapshot.as_ref().expect("snapshot")}),
        Some(&request),
    );
    let last = execute_local_search(&last_request, &policy, &security, &NeverCancel, None, None)
        .expect("last collected page");
    assert!(last.is_partial);
    assert!(last.terminal_limit);
    assert!(last.next.is_none());
    assert_eq!(last.stats.cap_reason.as_deref(), Some("maxCollectedFiles"));
}

#[test]
fn cancellation_during_the_walk_reports_cancelled() {
    use crate::tools::cancel::CancellationCheck;
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
    let error = execute_local_search(&request, &policy, &security, &cancel, None, None)
        .expect_err("cancelled mid-walk");
    assert_eq!(error.code, "cancelled");
    assert!(
        cancel.0.load(Ordering::SeqCst) < 40,
        "walk polled every file"
    );
}

#[test]
fn clipped_secret_guard_fails_closed_when_the_source_cannot_be_reread() {
    let security = ContentSecurity::new();
    let mut file = octocode_engine::types::RipgrepFile {
        path: "gone.txt".into(),
        match_count: 1,
        matches: vec![octocode_engine::types::RipgrepMatch {
            line: 1,
            column: 0,
            value: "…token = ghp_abcdefghijklmnop".into(),
            count: None,
            kind: None,
            score_hint: None,
            rank: None,
            original_chars: Some(400),
        }],
    };
    let missing = tempfile::tempdir()
        .expect("fixture directory")
        .path()
        .join("gone.txt");
    let verified = verify::guard_clipped_secrets(
        &mut file,
        &missing,
        None,
        0..10,
        &security,
        false,
        &NeverCancel,
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(!file.matches[0].value.contains("ghp_"));
    assert!(file.matches[0].value.contains("REDACTED"));
}

/// A source that shrank or was replaced after the search no longer holds
/// the matched line, so the clipped value cannot be verified.
#[test]
fn clipped_secret_guard_fails_closed_when_the_match_line_is_gone() {
    let security = ContentSecurity::new();
    let root = tempfile::tempdir().expect("fixture directory");
    let source = root.path().join("shrunk.txt");
    fs::write(&source, "one line now\n").expect("fixture");
    let mut file = octocode_engine::types::RipgrepFile {
        path: "shrunk.txt".into(),
        match_count: 1,
        matches: vec![octocode_engine::types::RipgrepMatch {
            line: 40,
            column: 0,
            value: "…token = ghp_abcdefghijklmnop".into(),
            count: None,
            kind: None,
            score_hint: None,
            rank: None,
            original_chars: Some(400),
        }],
    };
    let verified = verify::guard_clipped_secrets(
        &mut file,
        &source,
        None,
        0..10,
        &security,
        false,
        &NeverCancel,
    )
    .expect("not cancelled");
    assert_eq!(verified, verify::Verification::Unverified);
    assert!(
        !file.matches[0].value.contains("ghp_"),
        "{}",
        file.matches[0].value
    );
}

/// Without `regex`, text with no regex operator (`| \\ [ ] * + ? ^ $ { }`)
/// is literal, so `.unwrap()` and an empty group `()` find only that text;
/// an operator keeps it a regex, and an explicit `regex` always wins. An
/// inferred mode the two readings disagree on is disclosed once.
#[test]
fn an_unset_regex_reads_operator_free_text_literally() {
    let files = [("a.rs", "x.unwrap();\nunwrap_or(1);\nfoo_unwrap()\n")];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    let lines = |body: &serde_json::Value| body["stats"]["totalMatchedLines"].as_u64().unwrap_or(0);
    let warned = |body: &serde_json::Value, text: &str| {
        body["warnings"].as_array().is_some_and(|all| {
            all.iter()
                .any(|w| w.as_str().is_some_and(|w| w.contains(text)))
        })
    };
    let literal = run(serde_json::json!({"matchString": ".unwrap()"}));
    assert_eq!(lines(&literal), 1, "{literal}");
    assert!(warned(&literal, "ran as literal text"), "{literal}");
    assert!(literal["next"].get("searchLiteral").is_none(), "{literal}");
    let group = run(serde_json::json!({"matchString": "()"}));
    assert_eq!(lines(&group), 2, "{group}");
    let alternation = run(serde_json::json!({"matchString": "unwrap_or|foo_"}));
    assert_eq!(lines(&alternation), 2, "{alternation}");
    // An obvious regex ran as one: no inferred-regex note.
    assert!(!warned(&alternation, "ran as a regex"), "{alternation}");
    let explicit = run(serde_json::json!({"matchString": ".unwrap()", "regex": "rust"}));
    assert_eq!(lines(&explicit), 2, "{explicit}");
    let exact = run(serde_json::json!({"matchString": "unwrap_or|foo_", "regex": "literal"}));
    assert_eq!(lines(&exact), 0, "{exact}");
    for quiet in [
        &explicit,
        &exact,
        &run(serde_json::json!({"matchString": "unwrap"})),
    ] {
        assert!(
            !warned(quiet, "ran as literal text") && !warned(quiet, "ran as a regex"),
            "{quiet}"
        );
    }
}

/// An empty search whose text lives only in ignored or hidden files says
/// so, and its lead finds them.
#[test]
fn an_empty_search_names_matches_under_ignored_and_hidden_paths() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "logs/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("logs")).expect("ignored dir");
    fs::create_dir_all(root.path().join(".notes")).expect("hidden dir");
    fs::write(root.path().join("logs/log.txt"), "needle-here\n").expect("fixture");
    fs::write(root.path().join(".notes/x.txt"), "needle-here\n").expect("fixture");
    fs::write(root.path().join("src.txt"), "other\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle-here"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    // The probe's finding is the row's first hint (rows keep one).
    let hint = &result.hints[0];
    assert!(
        hint.starts_with("Run hints.includeIgnored: 2 file(s) match in ignored, hidden"),
        "{hint}"
    );
    assert!(hint.len() <= 120, "{hint}");
    let next = result.next.as_ref().expect("lead");
    let lead = &next["includeIgnored"];
    assert_eq!(lead["query"]["queries"][0]["noIgnore"], true, "{lead}");
    assert_eq!(lead["query"]["queries"][0]["hidden"], true, "{lead}");
    let retried: LocalSearchQuery =
        serde_json::from_value(lead["query"]["queries"][0].clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None)
        .expect("retry");
    assert_eq!(found.files.len(), 2, "{found:?}");
    // Nothing to find elsewhere: no probe hint and no lead.
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "absent-everywhere"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert!(result.next.is_none(), "{:?}", result.next);
    assert_eq!(result.hints.len(), 1, "{:?}", result.hints);
}

/// An empty search whose text lives only in a default-excluded directory
/// (`build/`) is not a proven absence: the hint names the directory and the
/// lead searches it with `defaultExcludes:false`.
#[test]
fn an_empty_search_names_default_excluded_directories_that_match() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir_all(root.path().join("build")).expect("build dir");
    fs::write(root.path().join("build/out.js"), "getElectronVersion()\n").expect("fixture");
    fs::write(root.path().join("src.js"), "other\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "getElectronVersion"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    let hint = &result.hints[0];
    assert!(
        hint.starts_with("Run hints.includeIgnored") && hint.contains("(build/)"),
        "{hint}"
    );
    let lead = &result.next.as_ref().expect("lead")["includeIgnored"]["query"]["queries"][0];
    assert_eq!(lead["defaultExcludes"], false, "{lead}");
    let retried: LocalSearchQuery = serde_json::from_value(lead.clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None)
        .expect("retry");
    assert_eq!(found.files.len(), 1, "{found:?}");
}

/// Skipped binaries are disclosed once, on the first page: a later page
/// repeats neither the warning nor the listing lead.
#[test]
fn skipped_binaries_are_disclosed_on_the_first_page_only() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
        root.path().join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .expect("png");
    fs::write(root.path().join("a.txt"), "needle\n").expect("fixture");
    fs::write(root.path().join("b.txt"), "needle\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let first = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "pageSize": 1, "sort": "path"}),
        None,
    );
    let page1 =
        execute_local_search(&first, &policy, &security, &NeverCancel, None, None).expect("page 1");
    assert!(
        page1
            .warnings
            .iter()
            .any(|w| w.starts_with("binarySkipped")),
        "{:?}",
        page1.warnings
    );
    assert!(
        page1
            .next
            .as_ref()
            .is_some_and(|next| next.get("binarySkipped").is_some())
    );
    let next = page1.next.as_ref().expect("next")["nextPage"]["query"]["queries"][0].clone();
    let second: LocalSearchQuery = serde_json::from_value(next).expect("page 2 query");
    let page2 = execute_local_search(&second, &policy, &security, &NeverCancel, None, None)
        .expect("page 2");
    assert!(
        !page2
            .warnings
            .iter()
            .any(|w| w.starts_with("binarySkipped")),
        "{:?}",
        page2.warnings
    );
    assert!(
        page2
            .next
            .as_ref()
            .is_none_or(|next| next.get("binarySkipped").is_none())
    );
}

/// Bytes of a binary header that happen to match are not hits: no row and
/// no read lead a text read would fail on.
#[test]
fn a_match_inside_a_binary_header_yields_no_row_and_no_read() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(
        root.path().join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
    )
    .expect("png");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "PNG"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect("search");
    assert!(result.files.is_empty(), "{:?}", result.files);
    assert!(
        result
            .next
            .as_ref()
            .is_none_or(|next| next.get("read").is_none())
    );
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
    assert_eq!(
        body["next"]["nextPage"]["query"]["queries"][0]["page"], 2,
        "{body}"
    );
    assert!(
        body["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string(),
        "{body}"
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
        let mut request = ls_query(
            serde_json::json!({"path": path.to_string_lossy().into_owned(), "matchString": "function ", "matchPageSize": 100}),
            None,
        );
        let mut seen = std::collections::BTreeSet::new();
        let mut pages = 0;
        loop {
            pages += 1;
            assert!(pages < 50, "walk does not end");
            let result = execute_local_search(
                &request,
                &policy,
                &security,
                &NeverCancel,
                None,
                Some(20_000),
            )
            .unwrap_or_else(|error| panic!("page {pages} of {relative}: {error:?}"));
            for file in &result.files {
                for row in file.matches.iter().flatten() {
                    assert!(
                        seen.insert((file.path.clone(), row.line)),
                        "{}:{} twice",
                        file.path,
                        row.line
                    );
                }
            }
            let Some(next) = result.next.as_ref().and_then(|next| {
                next.get("nextMatchPage")
                    .or_else(|| next.get("nextPage"))
                    .cloned()
            }) else {
                break;
            };
            let mut query = next["query"]["queries"][0].clone();
            query["path"] = serde_json::json!(relative);
            request = serde_json::from_value(query).expect("continuation query");
        }
        assert!(pages > 1, "{relative}: expected several pages");
        let expected = if relative == "." { 401 } else { 400 };
        assert_eq!(seen.len(), expected, "{relative}");
    }
}

/// N4: a page cut by a binary file (`binaryQuit`) is partial, but its shown
/// text hits stay one read away.
#[test]
fn partial_binary_cut_page_keeps_read_lead() {
    let body = search_fixture(
        &[
            (
                "a.rs",
                "fn a() {}\nlet needle = 1;\nlet needle2 = needle;\n",
            ),
            ("blob.dat", "needle\u{0}needle\n"),
        ],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(body["next"]["read"]["tool"], "localFetch", "{body}");
    assert!(
        body["next"]["read"]["query"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs")),
        "{body}"
    );
}

/// N4/D7: a warning never names a lead the lead cap may drop.
#[test]
fn binary_warning_never_names_a_capped_lead() {
    let body = search_fixture(
        &[
            ("a.rs", "fn needle() {}\nneedle();\n"),
            ("img.png", "\u{0}\u{1}needle"),
        ],
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert!(
        !body["warnings"].to_string().contains("hints.binarySkipped"),
        "{body}"
    );
}

/// Q5: the inferred-regex note is kept for texts whose literal reading is
/// plausible, and dropped for obvious regexes.
#[test]
fn inferred_regex_note_only_when_ambiguous() {
    use super::leads::unambiguous_regex;
    for text in [
        "register.*Tool",
        "a|b",
        "\\bfoo",
        "x.+y",
        "fn read|struct",
        "pub fn [a-z_]+\\(",
        "^use ",
        "id_[0-9]",
    ] {
        assert!(unambiguous_regex(text), "{text}");
    }
    for text in [
        "a?.b",
        "$scope",
        "arr[0]",
        "x + y",
        "*ptr",
        "a || b",
        "list[i]?",
        "read(path)?",
    ] {
        assert!(!unambiguous_regex(text), "{text}");
    }
    let obvious = search_fixture(
        &[("a.rs", "register_x_Tool\n")],
        ls_query(serde_json::json!({"matchString": "register.*Tool"}), None),
    );
    assert!(
        !obvious["warnings"].to_string().contains("ran as a regex"),
        "{obvious}"
    );
}

/// N8a: the withheld notice lives once, in warnings; the empty hint does
/// not repeat it and gives no spelling advice.
#[test]
fn withheld_notice_appears_once_on_empty_rows() {
    let body = search_fixture(
        &[
            ("src/secrets/a.ts", "export const ISecretX = 1;\n"),
            ("src/b.ts", "export const other = 1;\n"),
        ],
        ls_query(serde_json::json!({"matchString": "ISecretX"}), None),
    );
    assert_eq!(body["stats"]["totalMatches"], 0, "{body}");
    let text = body.to_string();
    assert_eq!(text.matches("withheld by path policy").count(), 1, "{body}");
    let hint = body["hints"][0].as_str().expect("empty hint");
    assert!(hint.starts_with("No matches"), "{hint}");
    assert!(!hint.contains("shorter term"), "{hint}");
    assert!(hint.chars().count() <= 120, "{hint}");
}

/// LS6: a missing path leads to a tree of its nearest existing parent.
#[test]
fn missing_path_leads_to_nearest_parent_tree() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src/native")).expect("dirs");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().join("src/nativ").to_string_lossy(), "matchString": "x"}),
        None,
    );
    let error = execute_local_search(&request, &policy, &security, &NeverCancel, None, None)
        .expect_err("missing path");
    assert_eq!(error.code, "pathNotFound");
    let next = error.next.expect("viewTree lead");
    assert_eq!(next["viewTree"]["tool"], "structureSearch", "{next}");
    assert_eq!(
        next["viewTree"]["query"]["queries"][0]["path"], "src",
        "{next}"
    );
}

/// LS7: a scope-miss row leads to a listing with the same globs.
#[test]
fn scope_miss_leads_to_files_listing_with_same_globs() {
    let body = search_fixture(
        &[("a.rs", "needle\n")],
        ls_query(
            serde_json::json!({"matchString": "needle", "include": ["*.zzz"]}),
            None,
        ),
    );
    assert_eq!(body["stats"]["filesScanned"], 0, "{body}");
    let lead = &body["next"]["viewStructure"];
    assert_eq!(lead["tool"], "structureSearch", "{body}");
    let listing = &lead["query"]["queries"][0];
    assert_eq!(listing["operation"], "files", "{body}");
    assert_eq!(listing["include"], serde_json::json!(["*.zzz"]), "{body}");
}

/// X7: a bare word include also searches inside directories named by it.
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

/// LS3 stage 1: a large page keeps every run's owner, with its last line.
#[test]
fn large_page_keeps_enclosing_labels() {
    let source: String = (0..6)
        .map(|f| format!("fn f{f}() {{\n{}}}\n", "    helper();\n".repeat(10)))
        .collect();
    let body = search_fixture(
        &[("src/m.rs", &source)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let rows = body["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 60, "{body}");
    let label = regex::Regex::new(r"^\w+ \w+@\d+-\d+$").expect("regex");
    for (index, row) in rows.iter().enumerate() {
        if index % 10 == 0 {
            let name = row["in"].as_str().unwrap_or_default();
            assert!(label.is_match(name), "row {index}: {body}");
        } else {
            assert!(row.get("in").is_none(), "row {index}: {body}");
        }
    }
}

/// LS2: declaration rows carry `declaration:true`; other rows carry nothing.
#[test]
fn declaration_rows_are_marked_sparsely() {
    let body = search_fixture(
        &[(
            "m.rs",
            "fn target() {}\n// target in prose\nfn caller() { target(); }\n",
        )],
        ls_query(serde_json::json!({"matchString": "target"}), None),
    );
    let rows = body["files"][0]["matches"].as_array().expect("rows");
    let by_line = |line: u64| {
        rows.iter()
            .find(|row| row["line"] == line)
            .unwrap_or_else(|| panic!("line {line}: {body}"))
    };
    assert_eq!(by_line(1)["declaration"], true, "{body}");
    assert!(by_line(2).get("declaration").is_none(), "{body}");
    assert!(by_line(3).get("declaration").is_none(), "{body}");
}
