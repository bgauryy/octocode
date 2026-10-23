//! Match values: redacted secrets flagged, clipped values within the response
//! budget, and sensitive or symlinked paths excluded first.

use super::*;

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
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle.*".to_string(), "regex": "rust", "resultView": LocalSearchQueryResultView::MatchOnly, "matchContentLength": 30, "matchPageSize": 1, "unique": LocalSearchQueryUnique::Count}),
        None,
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("first page");
    let body = serde_json::to_value(&first).expect("serialize");
    let matched = &body["files"][0]["matches"][0];
    assert_eq!(
        matched["value"].as_str().expect("value").chars().count(),
        30
    );
    assert_eq!(matched["originalChars"], 4105);
    assert_eq!(matched["returnedChars"], 30);
    assert_eq!(matched["count"], 2);
    assert_eq!(body["matchCount"], 3);
    assert_eq!(body["matchedLineCount"], 3);
    assert!(body.get("partialReasons").is_none(), "{body}");
    assert_eq!(body["files"][0]["pagination"]["totalItems"], 2);
    assert!(
        !first
            .warnings
            .iter()
            .any(|warning| warning.contains("matchContentLength")),
        "{:?}",
        first.warnings
    );
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert_eq!(next["matchContentLength"], 30);
    assert_eq!(next["unique"], "count");
    assert_eq!(next["matchPage"], 2);
    let continued = ls_query(
        serde_json::json!({"matchPage": 2, "snapshot": first.source_snapshot}),
        Some(&request),
    );
    let second = execute_local_search(
        &continued,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
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
        let result =
            execute_local_search(&bounded, &policy, &security, &NeverCancel, None, None, None)
                .expect("default, minimal and exact-boundary caps");
        let body = serde_json::to_value(result).expect("serialize");
        let matched = &body["files"][0]["matches"][0];
        assert_eq!(
            matched["value"].as_str().expect("value").chars().count(),
            expected_chars
        );
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
        serde_json::json!({"matchString": "MIIEpQIB".to_string(), "resultView": LocalSearchQueryResultView::Paginated, "contextLines": 0}),
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
    crate::tools::test_support::checks::assert_absent(&clean, "redactedMatches");
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
        serde_json::json!({"matchString": "aGVsbG8".to_string(), "resultView": LocalSearchQueryResultView::Paginated, "contextLines": 0}),
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
// continuation cursor needed); the clip is reported by `originalChars`.
#[test]
fn oversized_match_is_clipped_to_response_budget_without_dropping_rows() {
    let root = tempfile::tempdir().expect("fixture directory");
    let line = format!("needle {}", "x".repeat(3_000_000));
    fs::write(root.path().join("giant.txt"), format!("{line}\n")).expect("fixture");
    let result = search_root(
        root.path(),
        serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::Paginated, "matchContentLength": 5_000_000, "contextLines": 0}),
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
    assert!(matches[0].clipped, "clip must be flagged clipped");
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
    // clipped by the engine; the runtime must surface originalChars
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
    assert!(
        matched["originalChars"].as_u64().expect("originalChars") >= 606,
        "{matched:?}"
    );
    assert!(matched["returnedChars"].as_u64().expect("returnedChars") <= 500);
    // The row's flags and its `expandValues` lead say it; no warning repeats them.
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| warning.contains("matchContentLength")),
        "{:?}",
        result.warnings
    );
    assert!(body["next"].get("expandValues").is_some(), "{body}");
}

#[test]
fn excludes_sensitive_and_symlink_descendants_before_projection() {
    let root_dir = crate::tools::test_support::Fixture::new();
    let root = root_dir.0.clone();
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
        serde_json::json!({"path": root.to_string_lossy().into_owned(), "matchString": "needle".to_string(), "includeHidden": true, "includeIgnored": true, "sort": LocalSearchQuerySort::Path}),
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
        // A binary file keeps the matches before its first NUL.
        vec!["binary.dat", "safe.txt"]
    );
    assert_eq!(result.stats.files_searched, 2);
    assert_eq!(
        result.stats.cap_reasons,
        [octocode_engine::types::CapReason::BinaryQuit]
    );
    assert_eq!(
        result.source_root,
        root.canonicalize().expect("canonical root")
    );
    fs::remove_dir_all(root).expect("cleanup");
}

/// A symlink inside the root that the walk does not follow is counted in
/// `skippedPaths`, not dropped unseen.
#[cfg(unix)]
#[test]
fn an_in_root_symlink_to_a_file_is_counted() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("real.txt"), "needle\n").expect("fixture");
    std::os::unix::fs::symlink(root.path().join("real.txt"), root.path().join("link.txt"))
        .expect("symlink");
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path(), "matchString": "needle", "regex": "literal"}),
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
    assert_eq!(result.files.len(), 1);
    let result = serde_json::to_value(&result).expect("result");
    assert_eq!(
        crate::tools::test_support::skipped_reasons(&result, "withheld"),
        [("symlink not followed".into(), 1)],
        "{result}"
    );
}
