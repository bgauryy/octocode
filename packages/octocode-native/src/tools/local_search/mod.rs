mod executor;
mod manifest;
mod types;
pub use executor::execute_local_search;
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
}
