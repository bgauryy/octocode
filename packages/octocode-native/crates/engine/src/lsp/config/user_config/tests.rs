use super::*;

fn entry(extra: &str) -> String {
    format!(r#"{{"command":"clangd","languageId":"c"{extra}}}"#)
}

#[test]
fn a_wrong_entry_costs_only_itself() {
    let content = format!(
        r#"{{"languageServers":{{".c":{},".py":{{"command":"x"}},"go":{}}}}}"#,
        entry(""),
        entry("")
    );
    let parsed = parse_user_config(&content);
    assert_eq!(parsed.servers.keys().collect::<Vec<_>>(), [".c"]);
    let on = |key: &str| {
        parsed
            .problems
            .iter()
            .filter(|(entry, _)| entry.as_deref() == Some(key))
            .count()
    };
    assert_eq!((on(".py"), on("go")), (1, 1));
}

#[test]
fn a_whole_file_problem_says_where_and_routes_nothing() {
    let parsed = parse_user_config("{\n  \"languageServers\": {,}\n}");
    assert!(parsed.servers.is_empty());
    assert!(
        parsed.problems[0].1.contains("line 2"),
        "{:?}",
        parsed.problems
    );
    let parsed = parse_user_config(r#"{"servers":{}}"#);
    assert!(parsed.problems[0].1.contains("languageServers"));
}

#[test]
fn unknown_fields_are_reported_and_keep_the_entry() {
    let content = format!(
        r#"{{"languageServers":{{".c":{}}}}}"#,
        entry(r#","arg":["--x"]"#)
    );
    let parsed = parse_user_config(&content);
    assert!(parsed.servers.contains_key(".c"));
    assert!(parsed.problems[0].1.contains("`arg`"));
}

#[test]
fn a_rejected_launch_is_reported_not_silently_replaced() {
    let content =
        r#"{"languageServers":{".c":{"command":"bash","args":["-c","x"],"languageId":"c"}}}"#;
    let parsed = parse_user_config(content);
    assert!(parsed.servers.is_empty());
    assert!(parsed.problems[0].1.contains("shell"));
}

#[test]
fn problems_never_echo_values() {
    let secret = "s3cr3t-token-value";
    let content = format!(
        r#"{{"languageServers":{{".c":{{"command":"clangd","languageId":"c","env":{{"K":7}},"maxMemoryMb":"{secret}"}}}}}}"#
    );
    let parsed = parse_user_config(&content);
    assert!(!format!("{:?}", parsed.problems).contains(secret));
    assert!(!parsed.problems.is_empty());
}

#[test]
fn problems_reach_only_the_extensions_they_change() {
    let home = tempfile::tempdir().expect("home");
    std::fs::write(
        home.path().join("lsp-servers.json"),
        r#"{"languageServers":{".c":{"command":"x"}}}"#,
    )
    .expect("config");
    let options = LspDiscoveryOptions {
        octocode_home: Some(home.path().to_path_buf()),
        ..LspDiscoveryOptions::default()
    };
    assert_eq!(user_config_problems_for(".c", "/w", &options).len(), 1);
    // A header routes with its language family.
    assert_eq!(user_config_problems_for(".h", "/w", &options).len(), 1);
    assert!(user_config_problems_for(".py", "/w", &options).is_empty());
}

/// One entry routes its language's other spellings to the same server.
#[test]
fn an_entry_routes_its_language_family() {
    for (entry, sibling) in [
        (".c", ".h"),
        (".c", ".cpp"),
        (".cpp", ".hpp"),
        (".cu", ".h"),
        (".py", ".pyi"),
        (".scala", ".sbt"),
        (".ts", ".mts"),
        (".js", ".jsx"),
    ] {
        assert!(
            family_siblings(sibling).contains(&entry),
            "{entry} {sibling}"
        );
    }
    assert!(!family_siblings(".py").contains(&".ts"));
    assert!(family_siblings(".go").is_empty());
}

#[test]
fn a_project_file_nothing_trusts_is_reported_ignored() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(root.path().join(".octocode")).expect("dir");
    std::fs::write(root.path().join(PROJECT_CONFIG), "{}").expect("config");
    let root = root.path().to_string_lossy().into_owned();
    let reports = user_config_reports(&root, &LspDiscoveryOptions::default());
    assert_eq!(reports.len(), 1);
    assert_eq!(
        (reports[0].source, reports[0].state),
        ("project", "ignored")
    );
    let trusted = LspDiscoveryOptions {
        trust_project_config: true,
        ..LspDiscoveryOptions::default()
    };
    assert_eq!(user_config_reports(&root, &trusted)[0].state, "read");
}
