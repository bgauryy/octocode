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
fn document_language_ids_follow_only_the_selected_custom_launch() {
    let home = tempfile::tempdir().expect("home");
    std::fs::write(
        home.path().join("lsp-servers.json"),
        r#"{"languageServers":{
            ".ts":{"command":"custom-server","languageId":"typescript"},
            ".tsx":{"command":"custom-server","languageId":"typescriptreact"},
            ".js":{"command":"custom-server","args":["--other"],"languageId":"custom-js"},
            ".h":{"command":"other-server","languageId":"cpp"}
        }}"#,
    )
    .expect("config");
    let options = LspDiscoveryOptions {
        octocode_home: Some(home.path().to_path_buf()),
        ..Default::default()
    };
    let root = home.path().to_string_lossy();
    let expected = std::collections::BTreeMap::from([
        (".ts".to_owned(), "typescript".to_owned()),
        (".tsx".to_owned(), "typescriptreact".to_owned()),
    ]);
    for extension in [".ts", ".tsx", ".mts"] {
        let routed = user_server_for_extension(extension, &root, &options).expect("route");
        assert_eq!(routed.config.document_language_ids, expected, "{extension}");
    }
    let other = user_server_for_extension(".js", &root, &options).expect("other route");
    assert_eq!(
        other.config.document_language_ids,
        std::collections::BTreeMap::from([(".js".to_owned(), "custom-js".to_owned())])
    );
}

#[test]
fn document_language_ids_keep_project_configuration_precedence() {
    let home = tempfile::tempdir().expect("home");
    let project = tempfile::tempdir().expect("project");
    std::fs::create_dir_all(project.path().join(".octocode")).expect("config directory");
    std::fs::write(
        home.path().join("lsp-servers.json"),
        r#"{"languageServers":{
            ".js":{"command":"custom-server","languageId":"home-js"},
            ".jsx":{"command":"custom-server","languageId":"home-jsx"}
        }}"#,
    )
    .expect("home config");
    std::fs::write(
        project.path().join(PROJECT_CONFIG),
        r#"{"languageServers":{
            ".js":{"command":"custom-server","languageId":"project-js"}
        }}"#,
    )
    .expect("project config");
    let options = LspDiscoveryOptions {
        trust_project_config: true,
        octocode_home: Some(home.path().to_path_buf()),
        ..Default::default()
    };
    let routed = user_server_for_extension(".jsx", &project.path().to_string_lossy(), &options)
        .expect("project sibling route");
    assert_eq!(
        routed.config.language_id.as_deref(),
        Some("javascriptreact")
    );
    assert_eq!(
        routed.config.document_language_ids,
        std::collections::BTreeMap::from([(".js".to_owned(), "project-js".to_owned())])
    );
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

/// A Swift package outside any repository: `Package.swift` two levels above
/// the source. The built-in rules know no Swift marker and root the file's own
/// directory; the entry's `rootMarkers` root the package.
#[test]
fn root_markers_of_the_routing_entry_root_the_package() {
    let home = tempfile::tempdir().expect("home");
    let package = tempfile::tempdir().expect("package");
    let sources = package.path().join("Sources").join("P");
    std::fs::create_dir_all(&sources).expect("sources");
    std::fs::write(
        package.path().join("Package.swift"),
        "// swift-tools-version:5.9\n",
    )
    .expect("manifest");
    let file = sources.join("lib.swift");
    std::fs::write(&file, "public func f() {}\n").expect("source");
    let file = file.to_string_lossy().into_owned();
    let options = LspDiscoveryOptions {
        octocode_home: Some(home.path().to_path_buf()),
        ..Default::default()
    };
    let config = |entry: &str| {
        std::fs::write(
            home.path().join("lsp-servers.json"),
            format!(r#"{{"languageServers":{{".swift":{entry}}}}}"#),
        )
        .expect("config");
    };
    let built_in = crate::lsp::workspace::resolve_workspace_root_for_file(file.clone())
        .expect("built-in root");
    if built_in != sources.to_string_lossy() {
        return; // an ancestor of the temp dir holds a project marker
    }
    config(r#"{"command":"sourcekit-lsp","languageId":"swift"}"#);
    assert_eq!(
        workspace_root_for_file(&file, &options).expect("root"),
        built_in
    );
    config(r#"{"command":"sourcekit-lsp","languageId":"swift","rootMarkers":["Package.swift"]}"#);
    assert_eq!(
        workspace_root_for_file(&file, &options).expect("root"),
        package.path().to_string_lossy()
    );
    // A marker nothing holds leaves the built-in root.
    config(r#"{"command":"sourcekit-lsp","languageId":"swift","rootMarkers":["Nothing.here"]}"#);
    assert_eq!(
        workspace_root_for_file(&file, &options).expect("root"),
        built_in
    );
}

/// Inside a repository the search stops at its root: a marker above it belongs
/// to something else.
#[test]
fn root_markers_stay_in_the_repository() {
    let home = tempfile::tempdir().expect("home");
    let outer = tempfile::tempdir().expect("outer");
    std::fs::write(outer.path().join("Package.swift"), "").expect("outer manifest");
    let repo = outer.path().join("repo");
    let sources = repo.join("Sources");
    std::fs::create_dir_all(repo.join(".git")).expect("git");
    std::fs::create_dir_all(&sources).expect("sources");
    let file = sources.join("lib.swift").to_string_lossy().into_owned();
    std::fs::write(&file, "").expect("source");
    std::fs::write(
        home.path().join("lsp-servers.json"),
        r#"{"languageServers":{".swift":{"command":"sourcekit-lsp","languageId":"swift","rootMarkers":["Package.swift"]}}}"#,
    )
    .expect("config");
    let options = LspDiscoveryOptions {
        octocode_home: Some(home.path().to_path_buf()),
        ..Default::default()
    };
    assert_eq!(
        workspace_root_for_file(&file, &options).expect("root"),
        repo.to_string_lossy()
    );
}

#[test]
fn a_root_marker_that_climbs_or_is_absolute_costs_the_entry() {
    for markers in [
        r#"["../x"]"#,
        r#"["/abs"]"#,
        r#"[""]"#,
        r#""Package.swift""#,
    ] {
        let content = format!(
            r#"{{"languageServers":{{".c":{}}}}}"#,
            entry(&format!(r#","rootMarkers":{markers}"#))
        );
        let parsed = parse_user_config(&content);
        assert!(parsed.servers.is_empty(), "{markers}");
        assert!(parsed.problems[0].1.contains("rootMarkers"), "{markers}");
    }
}
