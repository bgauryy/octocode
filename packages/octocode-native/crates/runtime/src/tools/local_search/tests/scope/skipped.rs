//! Skipped-path disclosure: the rule or entry that held the matches, on the
//! first page only, outliving compact hints.

use super::*;

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
