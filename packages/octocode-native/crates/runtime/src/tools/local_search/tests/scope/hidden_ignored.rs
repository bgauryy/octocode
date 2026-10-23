//! Hidden, ignored and pruned entries: what reaches them and what a search with
//! hits discloses about them.

use super::*;

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
