mod executor;
mod manifest;
mod types;
pub use executor::execute_local_search;
pub use types::LocalSearchError;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        policy::path::{PathPolicy, PathPolicyConfig},
        security::{ContentSecurity, SecurityRegistry},
        tools::local_fetch::NeverCancel,
    };
    use regex::Regex;
    use std::{
        fs,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

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
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let request = LocalSearchRequest {
            path: root.path().to_string_lossy().into_owned(),
            search_text: "needle.*".into(),
            result_view: Some(ResultView::MatchOnly),
            match_content_length: Some(30),
            max_matches_per_file: Some(1),
            unique: Some(UniqueMode::Count),
            ..Default::default()
        };
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
        assert_eq!(body["files"][0]["totalMatchRows"], 2);
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
        let continued = LocalSearchRequest {
            match_page: Some(2),
            snapshot: first.source_snapshot,
            ..request
        };
        let second = execute_local_search(&continued, &policy, &security, &NeverCancel)
            .expect("second page");
        let body = serde_json::to_value(second).expect("serialize");
        let matched = &body["files"][0]["matches"][0];
        assert_eq!(matched["originalChars"], 4107);
        assert_eq!(matched["returnedChars"], 30);
        assert_eq!(matched["count"], 1);
        assert_eq!(body["files"][0]["totalMatchRows"], 2);
        assert_eq!(body["files"][0]["pagination"]["hasMore"], false);
        assert!(body.get("next").is_none());

        for (limit, expected_chars) in [(None, 500), (Some(1), 1), (Some(4105), 4105)] {
            let bounded = LocalSearchRequest {
                match_page: Some(1),
                snapshot: None,
                match_content_length: limit,
                ..continued.clone()
            };
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

    #[test]
    fn content_view_snippet_carries_truncation_indicator() {
        // Fix 7: a content-view match on a line longer than matchContentLength is
        // clipped by the engine; the runtime must surface truncated/originalChars
        // (previously only the matchOnly path did), plus the truncation warning.
        let root = tempfile::tempdir().expect("fixture directory");
        let long_line = format!("needle {}", "x".repeat(600));
        fs::write(root.path().join("big.txt"), format!("{long_line}\n")).expect("fixture");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("policy");
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let request = LocalSearchRequest {
            path: root.path().to_string_lossy().into_owned(),
            search_text: "needle".into(),
            ..Default::default()
        };
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
    fn excludes_sensitive_binary_and_symlink_descendants_before_projection() {
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
        fs::write(root.join("denied.txt"), "unmatched policy file\n").expect("denied fixture");
        fs::create_dir(root.join("vault")).expect("vault dir");
        fs::write(root.join("vault/nested.txt"), "needle hidden\n").expect("vault fixture");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", root.join("escape"))
                .expect("symlink fixture");
        }
        let mut registry = SecurityRegistry::default();
        registry
            .add_ignored_file_patterns([Regex::new(r"denied\.txt$").expect("file regex")])
            .expect("file policy");
        registry
            .add_ignored_path_patterns([Regex::new(r"/vault(?:/|$)").expect("path regex")])
            .expect("path policy");
        let policy = PathPolicy::with_registry(
            PathPolicyConfig {
                workspace_root: Some(root.clone()),
                additional_roots: vec![],
                include_home: false,
                home_dir: None,
            },
            &registry,
        )
        .expect("policy");
        let security = ContentSecurity::new(Arc::new(registry));
        let request = LocalSearchRequest {
            path: root.to_string_lossy().into_owned(),
            search_text: "needle".into(),
            hidden: Some(true),
            no_ignore: Some(true),
            sort: Some(SortMode::Path),
            ..Default::default()
        };
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel).expect("search");
        assert_eq!(
            result
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            vec!["safe.txt"]
        );
        assert_eq!(result.stats.files_searched, 2);
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
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let request = LocalSearchRequest {
            path: root.to_string_lossy().into_owned(),
            search_text: "needle".into(),
            ..Default::default()
        };
        let initial = execute_local_search(&request, &policy, &security, &NeverCancel)
            .expect("initial search");
        let snapshot = initial.source_snapshot.expect("identity on final page");
        let mut continued = request.clone();
        continued.snapshot = Some(snapshot.clone());
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
        empty.search_text = "absent".into();
        empty.snapshot = Some(snapshot);
        assert_eq!(
            execute_local_search(&empty, &policy, &security, &NeverCancel)
                .expect_err("query-scope/empty mismatch")
                .code,
            "staleSnapshot"
        );
        continued.snapshot = Some("forged".into());
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
            classify_search(true, false, false, false, 0, 0, true),
            (SearchStatus::Empty, false)
        );
        assert_eq!(
            classify_search(false, true, false, false, 0, 1, true),
            (SearchStatus::Partial, true)
        );
        assert_eq!(
            classify_search(false, false, true, false, 0, 1, false),
            (SearchStatus::Partial, false)
        );
        assert_eq!(
            classify_search(false, false, true, false, 0, 1, true),
            (SearchStatus::Partial, true)
        );
        assert_eq!(
            classify_search(false, false, false, false, 0, 1, true),
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
        let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
        let request = LocalSearchRequest {
            path: oversized.to_string_lossy().into_owned(),
            search_text: "needle".into(),
            ..Default::default()
        };

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
}
