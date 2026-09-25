mod executor;
mod manifest;
mod types;
pub use executor::execute_local_search;
pub use types::LocalSearchError;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test query from wire JSON over a base query (or a minimal valid one);
    /// `null` removes a field so its contract default applies.
    fn ls_query(fields: serde_json::Value, base: Option<&LocalSearchQuery>) -> LocalSearchQuery {
        let mut value = base.map_or_else(
            || serde_json::json!({"path": "_", "searchText": "_", "reasoning": "test"}),
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
    use crate::{
        policy::path::{PathPolicy, PathPolicyConfig},
        security::ContentSecurity,
        tools::local_fetch::NeverCancel,
    };
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        (root, policy, security)
    }

    #[test]
    fn context_lines_scale_the_default_match_content_length() {
        let (root, policy, security) = context_fixture();
        let base = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "needle".to_string(), "contextLines": 4}),
            None,
        );
        let run = |request: &LocalSearchQuery| {
            let result =
                execute_local_search(request, &policy, &security, &NeverCancel).expect("search");
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
            serde_json::json!({"contextLines": null, "resultView": ResultView::Detailed}),
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "needle.*".to_string(), "resultView": ResultView::MatchOnly, "matchContentLength": 30, "maxMatchesPerFile": 1, "unique": UniqueMode::Count}),
            None,
        );
        let first =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("first page");
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
        assert_eq!(body["stats"]["totalOccurrences"], 3);
        assert_eq!(body["stats"]["matchedLines"], 3);
        assert_eq!(body["stats"]["capped"], false);
        assert_eq!(body["files"][0]["pagination"]["totalMatches"], 2);
        assert!(
            first
                .warnings
                .iter()
                .any(|warning| warning.contains("matchContentLength"))
        );
        let next = &body["next"]["nextMatchPage"]["query"];
        assert_eq!(next["matchContentLength"], 30);
        assert_eq!(next["unique"], "count");
        assert_eq!(next["matchPage"], 2);
        let continued = ls_query(
            serde_json::json!({"matchPage": 2, "snapshot": first.source_snapshot}),
            Some(&request),
        );
        let second = execute_local_search(&continued, &policy, &security, &NeverCancel)
            .expect("second page");
        let body = serde_json::to_value(second).expect("serialize");
        let matched = &body["files"][0]["matches"][0];
        assert_eq!(matched["originalChars"], 4107);
        assert_eq!(matched["returnedChars"], 30);
        assert_eq!(matched["count"], 1);
        // The last match page has nowhere further to route: no per-file paging.
        assert!(body["files"][0].get("pagination").is_none(), "{body}");
        assert!(body.get("next").is_none());

        for (limit, expected_chars) in [(None, 200), (Some(1), 1), (Some(4105), 4105)] {
            let bounded = ls_query(
                serde_json::json!({"matchPage": 1, "snapshot": null, "matchContentLength": limit}),
                Some(&continued.clone()),
            );
            let result = execute_local_search(&bounded, &policy, &security, &NeverCancel)
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        // Content view returns the whole matched line (not just the match span),
        // so a hit inside the base64 body would surface the key material.
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "MIIEpQIB".to_string(), "resultView": ResultView::Detailed, "contextLines": 0}),
            None,
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
        let body_json = serde_json::to_value(&result).expect("serialize");
        let value = body_json["files"][0]["matches"][0]["value"]
            .as_str()
            .unwrap_or_default();
        assert!(
            !value.contains("interiorKeyBody"),
            "private key body leaked via a localSearch match: {value}"
        );
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "aGVsbG8".to_string(), "resultView": ResultView::Detailed, "contextLines": 0}),
            None,
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "needle".to_string(), "resultView": ResultView::Detailed, "matchContentLength": 5_000_000, "contextLines": 0}),
            None,
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "needle".to_string()}),
            None,
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
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
            std::os::unix::fs::symlink("/etc/passwd", root.join("escape"))
                .expect("symlink fixture");
        }
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.to_string_lossy().into_owned(), "searchText": "needle".to_string(), "hidden": true, "noIgnore": true, "sort": SortMode::Path}),
            None,
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.to_string_lossy().into_owned(), "searchText": "needle".to_string()}),
            None,
        );
        let initial = execute_local_search(&request, &policy, &security, &NeverCancel)
            .expect("initial search");
        let snapshot = initial.source_snapshot.expect("identity on final page");
        let mut continued = request.clone();
        continued.snapshot = Some(snapshot.parse().expect("snapshot"));
        execute_local_search(&continued, &policy, &security, &NeverCancel)
            .expect("unchanged snapshot");

        fs::write(&source, "needle two\n").expect("same-size mutation");
        let stale = execute_local_search(&continued, &policy, &security, &NeverCancel)
            .expect_err("changed results must reject snapshot");
        assert_eq!(stale.code, "staleSnapshot");
        assert_eq!(
            stale.message,
            "Search snapshot cannot be continued (resultsChanged); restart the search."
        );
        let restart = stale.next.expect("restart");
        assert_eq!(restart["restart"]["tool"], "localSearch");
        assert!(restart["restart"]["query"].get("snapshot").is_none());
        assert_eq!(restart["restart"]["query"]["page"], 1);
        assert_eq!(restart["restart"]["query"]["matchPage"], 1);

        let mut empty = request.clone();
        empty.search_text = "absent".parse().expect("searchText");
        empty.snapshot = Some(snapshot.parse().expect("snapshot"));
        assert_eq!(
            execute_local_search(&empty, &policy, &security, &NeverCancel)
                .expect_err("query-scope/empty mismatch")
                .code,
            "staleSnapshot"
        );
        continued.snapshot = Some("forged".parse().expect("snapshot"));
        assert_eq!(
            execute_local_search(&continued, &policy, &security, &NeverCancel)
                .expect_err("forged snapshot")
                .code,
            "staleSnapshot"
        );
        fs::remove_dir_all(root).expect("cleanup");
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

    /// Explicitly targeting a single file that the engine skips (over the
    /// per-file byte ceiling → capped:true, capReason:"maxFileSize",
    /// filesSearched:0) must explain the skip instead of returning a silent
    /// "no matches" false negative.
    #[test]
    fn skipped_single_file_target_explains_the_cap_instead_of_silent_empty() {
        let root = tempfile::tempdir().expect("fixture directory");
        let oversized = root.path().join("huge.txt");
        // Sparse file over the engine's 20 MiB default ceiling: the skip is
        // decided on metadata length, so no bytes need to be written.
        let file = fs::File::create(&oversized).expect("fixture");
        file.set_len(20 * 1024 * 1024 + 1).expect("sparse length");
        drop(file);
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": oversized.to_string_lossy().into_owned(), "searchText": "needle".to_string()}),
            None,
        );

        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
        let body = serde_json::to_value(&result).expect("serialize");

        assert_eq!(body["stats"]["filesSearched"], 0, "{body}");
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
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
            Some(&request),
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
        serde_json::to_value(&result).expect("serialize")
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

    // A clipped value (matchOnly span, matchWindow, matchContentLength) must not
    // leak a partial secret that no longer matches the secret patterns.
    #[test]
    fn clipped_values_never_leak_partial_secrets() {
        let token = "ghp_1234567890abcdefghijklmnopqrstuvwxyzAB";
        let file = format!("token: {token} end\n");
        let cases = [
            ls_query(
                serde_json::json!({"searchText": "ghp_[0-9a-z]{20}".to_string(), "resultView": ResultView::MatchOnly}),
                None,
            ),
            ls_query(
                serde_json::json!({"searchText": "token".to_string(), "resultView": ResultView::MatchOnly, "matchWindow": 20}),
                None,
            ),
            ls_query(
                serde_json::json!({"searchText": "end".to_string(), "resultView": ResultView::MatchOnly, "matchWindow": 30}),
                None,
            ),
            ls_query(
                serde_json::json!({"searchText": "token".to_string(), "matchContentLength": 30}),
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
                serde_json::json!({"searchText": "needle".to_string(), "resultView": ResultView::MatchOnly}),
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
                serde_json::json!({"searchText": "interior".to_string(), "resultView": ResultView::MatchOnly, "matchWindow": 6}),
                None,
            ),
        );
        assert!(!all_values(&body).contains("interior"), "{body}");
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
                serde_json::json!({"searchText": "foo".to_string(), "pageSize": 2, "matchPage": 3, "maxMatchesPerFile": 2, "contextLines": 0, "sort": SortMode::Path}),
                None,
            ),
        );
        // Page 1 at matchPage 3: a.txt's 5th row is shown, nothing is left over.
        assert!(
            body["next"].get("nextMatchPage").is_none(),
            "{}",
            body["next"]
        );
        assert_eq!(body["next"]["nextPage"]["query"]["matchPage"], 1);
        let body = search_fixture(
            &files,
            ls_query(
                serde_json::json!({"searchText": "foo".to_string(), "pageSize": 2, "page": 2, "maxMatchesPerFile": 2, "contextLines": 0, "sort": SortMode::Path}),
                None,
            ),
        );
        assert_eq!(body["next"]["nextMatchPage"]["query"]["matchPage"], 2);
    }

    #[test]
    fn later_match_pages_omit_files_exhausted_on_earlier_pages() {
        let many = "foo\n".repeat(5);
        let files = [("a.txt", many.as_str()), ("b.txt", "foo\n")];
        let request = |match_page| {
            ls_query(
                serde_json::json!({"searchText": "foo".to_string(), "matchPage": match_page, "maxMatchesPerFile": 2, "contextLines": 0, "sort": SortMode::Path}),
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
                serde_json::json!({"searchText": "foo".to_string(), "resultView": ResultView::Files}),
                None,
            ),
        );
        assert!(body.get("next").is_none(), "{}", body["next"]);
        assert_ne!(body["status"], "partial", "{body}");
    }

    #[test]
    fn binary_files_do_not_mark_a_search_partial_or_terminal() {
        let body = search_fixture(
            &[("bin.dat", "foo\u{0}foo\n"), ("a.txt", "foo\n")],
            ls_query(serde_json::json!({"searchText": "foo".to_string()}), None),
        );
        assert!(body.get("terminalLimit").is_none(), "{body}");
        assert!(body.get("next").is_none_or(|next| next.is_null()), "{body}");
        assert_eq!(body["stats"]["capReason"], "binaryQuit");
        assert_eq!(body["stats"]["capped"], false, "{body}");
    }

    #[test]
    fn reverse_applies_to_default_relevance_order() {
        let files = [("a.txt", "foo\nfoo\nfoo\n"), ("b.txt", "foo\n")];
        let order = |reverse| {
            let body = search_fixture(
                &files,
                ls_query(
                    serde_json::json!({"searchText": "foo".to_string(), "reverse": reverse}),
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

    // Lean defaults: the paginated view returns only the hit line, 10 rows per
    // file page and 20 files per page; `detailed` keeps a ±3 context window.
    #[test]
    fn default_paginated_view_is_lean_and_detailed_keeps_context() {
        let file = numbered(30, &(1..=12).collect::<Vec<_>>());
        let many: Vec<(String, String)> = (0..25)
            .map(|i| (format!("f{i:02}.txt"), "line 1 needle\n".to_owned()))
            .collect();
        let mut fixtures: Vec<(&str, &str)> = vec![("a.txt", file.as_str())];
        fixtures.extend(many.iter().map(|(p, c)| (p.as_str(), c.as_str())));
        let body = search_fixture(
            &fixtures,
            ls_query(
                serde_json::json!({"searchText": "needle".to_string(), "sort": SortMode::Path}),
                None,
            ),
        );
        let files = body["files"].as_array().expect("files");
        assert_eq!(files.len(), 20, "{body}");
        let a = &files[0];
        assert_eq!(a["matches"].as_array().expect("rows").len(), 10, "{body}");
        assert_eq!(a["matches"][0]["value"], "line 1 needle");
        assert_eq!(a["pagination"]["totalMatches"], 12);
        let detailed = search_fixture(
            &[("a.txt", &numbered(30, &[20]))],
            ls_query(
                serde_json::json!({"searchText": "needle".to_string(), "resultView": ResultView::Detailed}),
                None,
            ),
        );
        assert_eq!(
            detailed["files"][0]["matches"][0]["value"],
            "line 17\nline 18\nline 19\nline 20 needle\nline 21\nline 22\nline 23"
        );
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
                serde_json::json!({"searchText": "needle".to_string(), "contextLines": 2}),
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
                    format!("line {n} needle")
                } else {
                    format!("line {n}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(matches[0]["value"], expected);
        assert_eq!(matches[1]["line"], 25);
        assert!(matches[1].get("matchLines").is_none(), "{body}");
        assert_eq!(
            matches[1]["value"],
            "line 23\nline 24\nline 25 needle\nline 26\nline 27"
        );
        // Counts stay per matched line.
        assert_eq!(body["stats"]["matchedLines"], 4);
    }

    #[test]
    fn windows_merge_at_file_edges_and_stay_apart_when_disjoint() {
        let file = numbered(6, &[1, 2, 6]);
        let body = search_fixture(
            &[("a.txt", &file)],
            ls_query(
                serde_json::json!({"searchText": "needle".to_string(), "contextLines": 1}),
                None,
            ),
        );
        let matches = body["files"][0]["matches"].as_array().expect("matches");
        assert_eq!(matches.len(), 2, "{body}");
        assert_eq!(matches[0]["value"], "line 1 needle\nline 2 needle\nline 3");
        assert_eq!(matches[0]["matchLines"], serde_json::json!([1, 2]));
        assert_eq!(matches[1]["value"], "line 5\nline 6 needle");
        // matchOnly never merges: it carries spans, not windows.
        let body = search_fixture(
            &[("a.txt", &file)],
            ls_query(
                serde_json::json!({"searchText": "needle".to_string(), "resultView": ResultView::MatchOnly}),
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
                serde_json::json!({"searchText": "needle".to_string(), "matchContentLength": 100}),
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
                serde_json::json!({"searchText": "foo".to_string(), "maxMatchesPerFile": 2, "contextLines": 0}),
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
        assert_eq!(b["pagination"]["totalMatches"], 3);
        assert!(b["pagination"].get("matchesPerPage").is_none(), "{b}");
        let next = &body["next"]["nextMatchPage"]["query"];
        assert!(next["snapshot"].is_string(), "{body}");
        // Multi-page file results still carry file pagination.
        let body = search_fixture(
            &[("a.txt", "foo\n"), ("b.txt", "foo\n")],
            ls_query(
                serde_json::json!({"searchText": "foo".to_string(), "pageSize": 1}),
                None,
            ),
        );
        assert_eq!(body["pagination"]["totalPages"], 2, "{body}");
        assert!(body["pagination"]["snapshot"].is_string(), "{body}");
    }

    // Continuations re-materialize only the context default the view uses: none
    // for context-free views, and the detailed view's own default (3) — a
    // hard-coded 2 changed the fingerprint and made every continuation stale.
    #[test]
    fn continuations_carry_the_view_context_default() {
        let file = "foo\n".repeat(3);
        let make = |view| {
            ls_query(
                serde_json::json!({"searchText": "foo".to_string(), "resultView": view, "maxMatchesPerFile": 1}),
                None,
            )
        };
        let body = search_fixture(&[("a.txt", &file)], make(ResultView::MatchOnly));
        let next = &body["next"]["nextMatchPage"]["query"];
        assert!(next.get("contextLines").is_none(), "{next}");

        let root = tempfile::tempdir().expect("fixture directory");
        fs::write(root.path().join("a.txt"), &file).expect("fixture");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new();
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
            Some(&make(ResultView::Detailed)),
        );
        let first =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("first page");
        let body = serde_json::to_value(&first).expect("serialize");
        let next = body["next"]["nextMatchPage"]["query"].clone();
        assert_eq!(next["contextLines"], 3, "{next}");
        let mut continued: LocalSearchQuery =
            serde_json::from_value(next).expect("continuation parses");
        continued.path = request.path.clone();
        execute_local_search(&continued, &policy, &security, &NeverCancel)
            .expect("detailed continuation is not stale");
    }

    fn policy_for(root: &std::path::Path) -> (PathPolicy, ContentSecurity) {
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
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

    #[cfg(unix)]
    fn unlock_dir(dir: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
    }

    #[cfg(unix)]
    #[test]
    fn a_scope_where_every_path_is_unreadable_is_an_access_error_not_empty() {
        let root = tempfile::tempdir().expect("fixture directory");
        let hidden = root.path().join("hidden");
        fs::create_dir(&hidden).expect("dir");
        fs::write(hidden.join("secret.txt"), "needle-only-here\n").expect("fixture");
        if !lock_dir(&hidden) {
            return;
        }
        let (policy, security) = policy_for(root.path());
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "needle-only-here".to_string(), "regex": RegexMode::Literal, "noIgnore": true}),
            None,
        );
        let outcome = execute_local_search(&request, &policy, &security, &NeverCancel);
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
        let hidden = root.path().join("hidden");
        fs::create_dir(&hidden).expect("dir");
        fs::write(hidden.join("secret.txt"), "needle-only-here\n").expect("fixture");
        if !lock_dir(&hidden) {
            return;
        }
        let (policy, security) = policy_for(root.path());
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "needle-only-here".to_string(), "regex": RegexMode::Literal, "noIgnore": true}),
            None,
        );
        let outcome = execute_local_search(&request, &policy, &security, &NeverCancel);
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
        let (policy, security) = policy_for(root.path());
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "alpha".to_string(), "regex": RegexMode::Literal}),
            None,
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
        assert_ne!(result.status, SearchStatus::Empty);
        let body = serde_json::to_value(&result).expect("serialize");
        assert_eq!(body["files"][0]["matches"][0]["line"], 1, "{body}");
        assert_eq!(body["stats"]["totalOccurrences"], 1, "{body}");
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.starts_with("binaryFileSkipped")),
            "{body}"
        );
    }

    #[test]
    fn an_empty_result_with_a_binary_cut_is_partial_not_empty() {
        let body = search_fixture(
            &[("blob.dat", "header\u{0}needle after the nul\n")],
            ls_query(
                serde_json::json!({"searchText": "needle".to_string()}),
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
    fn relevance_ranks_a_hot_file_beyond_the_path_sorted_cap() {
        let root = tempfile::tempdir().expect("fixture directory");
        for i in 0..=10_000 {
            fs::write(root.path().join(format!("a{i:05}.txt")), "hit\n").expect("fixture");
        }
        fs::write(root.path().join("zzz-hot.txt"), "hit\n".repeat(40)).expect("fixture");
        let (policy, security) = policy_for(root.path());
        let request = ls_query(
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "hit".to_string(), "regex": RegexMode::Literal}),
            None,
        );
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
        assert_eq!(result.files[0].path, "zzz-hot.txt");
        let stats = &result.stats;
        assert_eq!(stats.files_searched, 10_002);
        assert_eq!(stats.files_matched, 10_002);
        assert_eq!(stats.total_occurrences, 10_001 + 40);
        assert!(
            stats
                .cap_reason
                .as_deref()
                .is_some_and(|r| r.contains("maxCollectedFiles")),
            "{stats:?}"
        );
    }

    #[test]
    fn cancellation_during_the_walk_reports_cancelled() {
        use crate::tools::local_fetch::CancellationCheck;
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
            serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "searchText": "needle".to_string()}),
            None,
        );
        // The first check (before the walk) passes; the walk's polls cancel.
        let cancel = CancelAfter(AtomicU32::new(0), 3);
        let error = execute_local_search(&request, &policy, &security, &cancel)
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
                original_chars: Some(400),
            }],
        };
        let missing = tempfile::tempdir()
            .expect("fixture directory")
            .path()
            .join("gone.txt");
        let verified =
            executor::guard_clipped_secrets(&mut file, &missing, 0..10, &security, false);
        assert!(!verified);
        assert!(!file.matches[0].value.contains("ghp_"));
        assert!(file.matches[0].value.contains("REDACTED"));
    }
}
