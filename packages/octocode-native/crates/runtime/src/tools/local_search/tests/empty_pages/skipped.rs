//! Empty pages with skipped paths: grouping by rule, default excludes,
//! unproven probes and timed-out checks.

use super::*;

/// Skipped paths are disclosed once per cause and rule: directories one
/// ignore rule hides share one entry, its first path and `count`, while
/// `extras:["details"]` lists every path. Only skips that could hold a path
/// the row's `include` matches are named.
#[test]
fn skipped_paths_collapse_by_rule_and_stay_within_the_include_scope() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "repos/*\nlogs/\n").expect("gitignore");
    for dir in ["repos/a", "repos/b", "repos/c", "logs"] {
        fs::create_dir_all(root.path().join(dir)).expect("ignored dir");
        fs::write(root.path().join(dir).join("x.txt"), "other\n").expect("fixture");
    }
    fs::write(root.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(root.path());
    let path = root.path().to_string_lossy().into_owned();
    let ignored = |fields: serde_json::Value| {
        let request = ls_query(fields, None);
        // An unproven check names every ignored directory with its rule.
        let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("search")
        });
        let body = serde_json::to_value(&result).expect("result");
        body["skippedPaths"]["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["cause"] == "ignored")
            .cloned()
            .collect::<Vec<_>>()
    };
    let entry = |dir: &str, count: Option<u32>, reason: &str| {
        let mut entry = serde_json::json!({
            "path": skipped_source_path(root.path(), dir), "cause": "ignored", "reason": reason
        });
        if let Some(count) = count {
            entry["count"] = serde_json::json!(count);
        }
        entry
    };
    assert_eq!(
        ignored(serde_json::json!({"path": path, "matchString": "absent"})),
        [
            entry("logs", None, ".gitignore:2 'logs/'"),
            entry("repos/a", Some(3), ".gitignore:1 'repos/*'"),
        ],
    );
    assert_eq!(
        ignored(serde_json::json!({"path": path, "matchString": "absent", "extras": ["details"]})),
        [
            entry("logs", None, ".gitignore:2 'logs/'"),
            entry("repos/a", None, ".gitignore:1 'repos/*'"),
            entry("repos/b", None, ".gitignore:1 'repos/*'"),
            entry("repos/c", None, ".gitignore:1 'repos/*'"),
        ],
    );
    assert_eq!(
        ignored(
            serde_json::json!({"path": path, "matchString": "absent", "include": ["repos/b/**"]})
        ),
        [entry("repos/b", None, ".gitignore:1 'repos/*'")],
    );
}

/// An empty search whose text lives only in a default-excluded directory
/// (`build/`) is not a proven absence: the disclosure names the directory and the
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
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    let hint = &result.hints[0];
    assert!(
        hint == "1 file that matches behind skipped paths (build/).",
        "{hint}"
    );
    let body = serde_json::to_value(&result).expect("result");
    assert_eq!(
        body["skippedPaths"]["entries"],
        serde_json::json!([{"path":skipped_source_path(root.path(),"build"),"cause":"default-excluded","reason":"build/"}]),
        "{body}"
    );
    let lead = &result.next.as_ref().expect("lead")["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["defaultExcludes"], false, "{lead}");
    let retried: LocalSearchQuery = serde_json::from_value(lead.clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("retry");
    assert_eq!(found.files.len(), 1, "{found:?}");
}

/// A probe that runs out of time proves nothing about the ignored paths it
/// could not search: the row still names them and its lead lifts exactly the
/// ignore rules, under the key that says so. It is not a scope miss either.
#[test]
fn an_unproven_probe_names_the_ignored_paths_and_leads_with_include_ignored() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    // In a git work tree a default-excluded name is pruned where git
    // ignores it.
    fs::write(root.path().join(".gitignore"), "repos/*\nbuild/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("repos/go")).expect("ignored dir");
    fs::write(root.path().join("repos/go/a.go"), "func Run() {}\n").expect("go");
    fs::create_dir_all(root.path().join("build")).expect("build dir");
    fs::write(root.path().join("build/out.go"), "func Out() {}\n").expect("built");
    fs::write(root.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(),
            "matchString": "func", "include": ["*.go"]}),
        None,
    );
    let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    });
    assert!(result.files.is_empty());
    let hints = result.hints.join(" | ");
    let body = serde_json::to_value(&result).expect("result");
    assert_eq!(
        body["skippedPaths"]["entries"],
        serde_json::json!([{"path":skipped_source_path(root.path(),"repos/go"),"cause":"ignored","reason":".gitignore:1 'repos/*'"}]),
        "{body}"
    );
    assert!(
        result.hints.iter().all(|hint| hint.chars().count() <= 120),
        "{hints}"
    );
    assert!(!hints.contains("matched no file"), "{hints}");
    assert!(!hints.contains("No matches in"), "{hints}");
    let next = result.next.as_ref().expect("lead");
    let lead = &next["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["includeIgnored"], true, "{next}");
    assert!(next.get("viewTree").is_none(), "{next}");
    let retried: LocalSearchQuery = serde_json::from_value(lead.clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("retry");
    assert!(
        found.files.iter().any(|file| file.path.ends_with("a.go")),
        "{found:?}"
    );
    // No ignored path to name: an unproven probe says nothing about them.
    let plain = tempfile::tempdir().expect("fixture directory");
    fs::write(plain.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(plain.path());
    let request = ls_query(
        serde_json::json!({"path": plain.path().to_string_lossy().into_owned(), "matchString": "absent"}),
        None,
    );
    let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    });
    assert!(result.next.is_none(), "{:?}", result.next);
    // A hidden path is named too, and the lead lifts it.
    fs::create_dir(plain.path().join(".hid")).expect("hidden dir");
    fs::write(plain.path().join(".hid/a.txt"), "absent\n").expect("hidden");
    let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    });
    let warnings = result.warnings.join(" | ");
    assert!(warnings.contains("1 hidden"), "{warnings}");
    let next = result.next.as_ref().expect("lead");
    let lead = &next["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["includeHidden"], true, "{next}");
}

/// An empty page whose check of the skipped paths ran out of time did not
/// cover them: the row is partial (`skippedPathsUnchecked`), the gap is a
/// warning, not a hint, and it counts every skipped path of each cause the
/// walk left out (ignored files too, not only the ignored directories a
/// fresh walk names). The one lead lifts each of those causes and reaches
/// every unchecked match.
#[test]
fn an_empty_page_whose_skipped_path_check_timed_out_is_partial() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "*.log\n").expect("gitignore");
    fs::write(root.path().join("a.log"), "needle\n").expect("ignored file");
    fs::write(root.path().join("b.log"), "other\n").expect("ignored file");
    fs::create_dir(root.path().join(".hid")).expect("hidden dir");
    fs::write(root.path().join(".hid/a.txt"), "needle\n").expect("hidden file");
    fs::write(root.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let result = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    });
    assert!(result.files.is_empty());
    let body = serde_json::to_value(&result).expect("result");
    assert_eq!(body["isPartial"], true, "{body}");
    assert_eq!(
        body["partialReasons"],
        serde_json::json!(["skippedPathsUnchecked"]),
        "{body}"
    );
    assert!(body.get("terminalLimit").is_none(), "{body}");
    let warnings = result.warnings.join(" | ");
    assert!(
        warnings.contains("2 ignored") && warnings.contains("2 hidden"),
        "{warnings}"
    );
    assert!(
        result.hints.iter().all(|hint| !hint.contains("timed out")),
        "stated once, as a warning: {:?}",
        result.hints
    );
    let lead = &result.next.as_ref().expect("lead")["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["includeIgnored"], true, "{lead}");
    assert_eq!(lead["includeHidden"], true, "{lead}");
    let retried: LocalSearchQuery = serde_json::from_value(lead.clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("retry");
    assert_eq!(found.files.len(), 2, "{found:?}");
}
