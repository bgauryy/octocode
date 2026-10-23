//! Empty pages the walk itself limited: depth cuts, ignored, hidden and
//! symlinked paths, and probes for files without a match.

use super::*;

/// A `maxDepth` cut is disclosed once with the same search at every depth;
/// an empty directory at the limit hides nothing and is not counted.
#[test]
fn a_max_depth_cut_is_disclosed_with_a_deeper_lead() {
    let files = [
        ("top.txt", "needle\n"),
        ("a/b/deep.txt", "needle\n"),
        ("a/c/x.txt", "other\n"),
    ];
    let run = |fields: serde_json::Value| search_fixture(&files, ls_query(fields, None));
    let cut = run(serde_json::json!({"matchString": "needle", "maxDepth": 1}));
    assert_eq!(cut["matchedLineCount"], 1, "{cut}");
    assert!(
        cut["warnings"]
            .to_string()
            .contains("1 dir(s) at maxDepth were not entered"),
        "{cut}"
    );
    let lead = &cut["next"]["expandDepth"];
    let row = lead.pointer("/query/queries/0").unwrap_or(&lead["query"]);
    assert!(row.get("maxDepth").is_none(), "{row}");
    assert_eq!(run(row.clone())["matchedLineCount"], 2, "{row}");
    let whole = run(serde_json::json!({"matchString": "needle", "maxDepth": 3}));
    assert!(whole["next"].get("expandDepth").is_none(), "{whole}");
}

/// An empty `filesWithout` row (every searched file matches) names skipped
/// paths only for files there that lack a match, never the matching files
/// it already searched.
#[test]
fn an_empty_files_without_row_probes_for_files_without_a_match() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    // Every file the default walk skips matches too (the hidden `.gitignore`).
    fs::write(root.path().join(".gitignore"), "# the\nskip/\n").expect("gitignore");
    fs::write(root.path().join("a.txt"), "the\n").expect("a");
    let (policy, security) = policy_for(root.path());
    let path = root.path().to_string_lossy().into_owned();
    let run = |fields: serde_json::Value| {
        let request = ls_query(fields, None);
        execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
            .expect("search")
    };
    let row = serde_json::json!({"path": path, "matchString": "the", "resultView": "filesWithout"});
    let plain = run(row.clone());
    assert!(plain.files.is_empty());
    // The check searched them and found no such file: the hint says so
    // (outside the `.git/` the path policy withholds from every walk).
    assert_eq!(
        plain.hints,
        [
            "Skipped paths (1 hidden) hold no file lacking a match outside what their check withheld (warnings)."
        ],
    );
    assert!(
        plain
            .next
            .as_ref()
            .is_none_or(|next| next.get("includeSkipped").is_none())
    );
    fs::create_dir(root.path().join("skip")).expect("ignored dir");
    fs::write(root.path().join("skip/b.txt"), "zzz\n").expect("b");
    let found = run(row);
    let hints = found.hints.join(" | ");
    assert!(
        hints.contains("1 file that lacks a match behind skipped paths (.gitignore:2 'skip/')"),
        "{hints}"
    );
}

/// An include whose files all sit in ignored paths is not a scope miss: the
/// row names the skip cause and its lifting lead, and neither claims that
/// include/exclude matched no file nor leads to a listing that skips them too.
#[test]
fn an_include_matching_only_ignored_files_names_the_skip_not_the_scope() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "repos/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("repos/go")).expect("ignored dir");
    fs::write(root.path().join("repos/go/a.go"), "func (s *T) Run() {}\n").expect("go");
    fs::write(root.path().join("notes.txt"), "other\n").expect("text");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(),
            "matchString": "func (s *T)", "include": ["*.go"]}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    let hints = result.hints.join(" | ");
    assert!(
        hints.contains("that matches behind skipped paths"),
        "{hints}"
    );
    assert!(
        !hints.contains("include/exclude matched no file"),
        "{hints}"
    );
    let next = result.next.as_ref().expect("lead");
    assert_eq!(
        next["includeSkipped"]["query"]["queries"][0]["includeIgnored"], true,
        "{next}"
    );
    assert!(next.get("viewTree").is_none(), "{next}");
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
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    // The probe's finding is the row's first hint (rows keep one).
    let hint = &result.hints[0];
    assert!(
        hint == "2 files that match behind skipped paths (.gitignore:1 'logs/', hidden .notes/).",
        "{hint}"
    );
    assert!(hint.len() <= 120, "{hint}");
    let body = serde_json::to_value(&result).expect("result");
    let entries = body["skippedPaths"]["entries"].as_array().expect("skips");
    for (path, cause, reason) in [
        ("logs", "ignored", ".gitignore:1 'logs/'"),
        (".notes", "hidden", "hidden .notes/"),
    ] {
        assert!(
            entries.iter().any(
                |entry| entry["path"] == skipped_source_path(root.path(), path)
                    && entry["cause"] == cause
                    && entry["reason"] == reason
            ),
            "{body}"
        );
    }
    let next = result.next.as_ref().expect("lead");
    let lead = &next["includeSkipped"];
    assert_eq!(
        lead["query"]["queries"][0]["includeIgnored"], true,
        "{lead}"
    );
    assert_eq!(lead["query"]["queries"][0]["includeHidden"], true, "{lead}");
    let retried: LocalSearchQuery =
        serde_json::from_value(lead["query"]["queries"][0].clone()).expect("lead query");
    let found = execute_local_search(&retried, &policy, &security, &NeverCancel, None, None, None)
        .expect("retry");
    assert_eq!(found.files.len(), 2, "{found:?}");
    // Nothing to find elsewhere: no lead, and the hint says why none exists.
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "absent-everywhere"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.next.is_none(), "{:?}", result.next);
    // `.git/` is a hidden directory the path policy withholds from every
    // walk: absence there is unproven, and the warning names it.
    assert_eq!(
        result.hints,
        [
            "Skipped paths (1 ignored, 2 hidden) hold no match outside what their check withheld (warnings)."
        ],
    );
    assert_eq!(
        result.warnings,
        ["Skipped paths: 1 entry withheld by path policy: security-policy dirs .git/."],
    );
}

/// An empty search whose only unsearched scope is an ignored directory of
/// symlinks: the check of the skipped paths searched them and could not
/// follow the links, so no lead lifts the skip; the hint says so and the
/// warning counts what the check withheld.
#[cfg(unix)]
#[test]
fn an_empty_search_over_skipped_symlinks_says_why_no_lead_exists() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::create_dir(root.path().join(".git")).expect("repository marker");
    fs::write(root.path().join(".gitignore"), "agents/skills/\n").expect("gitignore");
    fs::create_dir_all(root.path().join("real")).expect("target dir");
    fs::write(root.path().join("real/x.md"), "needle-here\n").expect("fixture");
    fs::create_dir_all(root.path().join("agents/skills")).expect("ignored dir");
    fs::write(root.path().join("agents/README.md"), "other\n").expect("fixture");
    std::os::unix::fs::symlink("../../real", root.path().join("agents/skills/real"))
        .expect("symlink");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path": root.path().join("agents").to_string_lossy().into_owned(),
            "matchString": "needle-here"
        }),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert!(result.files.is_empty());
    assert!(result.next.is_none(), "{:?}", result.next);
    assert_eq!(
        result.hints,
        ["Skipped paths (1 ignored) hold no match outside what their check withheld (warnings)."],
    );
    assert!(
        result.warnings.iter().any(|warning| warning
            == "Skipped paths: 1 entry withheld by path policy: 1 symlink not followed."),
        "{:?}",
        result.warnings
    );
}
