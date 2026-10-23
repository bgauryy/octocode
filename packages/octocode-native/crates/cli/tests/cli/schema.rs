use super::*;

/// `schema --view full` lists every enabled tool with its field list, one line
/// on a pipe, small enough to read whole.
#[test]
fn schema_lists_compact_field_lists_for_every_enabled_tool() {
    let workspace = Workspace::new();
    // Persistent storage: the clone cache (ghCloneRepo) needs it.
    let output = workspace
        .cli()
        .env("OCTOCODE_STORAGE_MODE", "persistent")
        .args(["schema", "--view", "full"])
        .output()
        .expect("schema");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output).trim_end().lines().count(), 1);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("schema JSON");
    // Union tools list every mode instead of an empty field list.
    for tool in value["tools"].as_array().expect("tools array") {
        assert_ne!(tool["fields"], "[]", "{}", tool["name"]);
        assert!(tool.get("inputSchema").is_none(), "{tool}");
    }
    let fields_of = |name: &str| {
        value["tools"]
            .as_array()
            .and_then(|tools| tools.iter().find(|tool| tool["name"] == name))
            .and_then(|tool| tool["fields"].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    assert!(fields_of("ghStructure").contains("owner*"));
    assert!(fields_of("ghSearchCode").contains("owner*"));
    // Several row forms list each as a flat row with its selector value
    // inline (the embedded CLI presentation precomputes the lists from the
    // core Zod rows).
    assert!(fields_of("astSearch").contains("[operation:\"match\", path*, rule*"));
    assert!(fields_of("structureSearch").starts_with("[operation?:\"tree\", path*"));
    assert!(!fields_of("lspSearch").contains("anchored["));
    let clone = workspace
        .cli()
        .env("OCTOCODE_STORAGE_MODE", "persistent")
        .args(["schema", "ghCloneRepo"])
        .output()
        .expect("clone schema");
    let clone: serde_json::Value = serde_json::from_slice(&clone.stdout).expect("clone JSON");
    assert_eq!(clone["availability"]["enabled"], true, "{clone}");
}

/// The binary serves discovery itself: the catalog with agent instructions,
/// every view and `--select` of one tool, and `<tool> --help` with the
/// required fields and a runnable example. No Node is involved.
#[test]
fn schema_and_tool_help_are_served_by_the_binary() {
    let workspace = Workspace::new();
    let json = |args: &[&str]| -> serde_json::Value {
        let output = workspace.cli().args(args).output().expect("schema");
        assert!(output.status.success(), "{args:?}: {}", stdout(&output));
        serde_json::from_str(stdout(&output)).expect("schema JSON")
    };

    let compact = json(&["schema"]);
    let catalog = json(&["schema", "--view", "full"]);
    assert!(compact.to_string().len() < catalog.to_string().len());
    assert_eq!(compact["toolCount"], catalog["toolCount"]);
    assert_eq!(compact["commands"]["full"], "octocode schema --view full");
    assert_eq!(catalog["kind"], "octocode.toolCatalog");
    for setup in ["auth", "mcp", "skills", "graph"] {
        assert!(catalog["commands"]["setup"][setup].is_string(), "{setup}");
    }
    assert!(
        catalog["instructions"]
            .as_str()
            .is_some_and(|text| text.contains("<local>"))
    );
    let listed: Vec<&str> = catalog["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(listed.contains(&"localSearch"), "{listed:?}");
    // Disabled tools stay out of the listing, as in MCP tools/list.
    assert_eq!(catalog["toolCount"], listed.len());

    let query = json(&["schema", "localSearch", "--view", "query"]);
    assert_eq!(query, json(&["schema", "localSearch"]));
    assert_eq!(query["name"], "localSearch");
    assert_eq!(
        json(&["schema", "astSearch", "--select", "operation=symbols"]),
        json(&[
            "schema",
            "astSearch",
            "--view",
            "query",
            "--select",
            "operation=symbols"
        ])
    );
    assert!(query["querySchema"]["required"].is_array(), "{query}");
    assert_eq!(query["availability"]["enabled"], true);
    assert_eq!(query["run"], "octocode localSearch '<json>'");
    let variants = json(&["schema", "lspSearch", "--view", "variants"]);
    assert!(
        variants["variants"]
            .as_array()
            .is_some_and(|items| items.len() > 1)
    );
    let selected = json(&[
        "schema",
        "astSearch",
        "--view",
        "query",
        "--select",
        "operation=symbols",
    ]);
    // One operation's row: its own fields, no branches.
    assert!(
        selected["querySchema"]["properties"]["symbolName"].is_object(),
        "{selected}"
    );
    // A beta tool is still described, with its gate.
    let gated = json(&["schema", "clasify", "--view", "variants"]);
    assert_eq!(gated["availability"]["enabled"], false);

    for (args, message) in [
        (vec!["schema", "localSerch"], "Unknown tool: localSerch"),
        (
            vec![
                "schema",
                "lspSearch",
                "--view",
                "full",
                "--select",
                "operation=callers",
            ],
            "--select requires the query view",
        ),
        (
            vec!["schema", "lspSearch", "--view", "query", "--select", "nope"],
            "--select expects operation=VALUE",
        ),
        (
            vec![
                "schema",
                "lspSearch",
                "--view",
                "query",
                "--select",
                "operation=zzz",
            ],
            "Unknown operation: zzz",
        ),
    ] {
        let output = workspace.cli().args(&args).output().expect("schema error");
        assert_eq!(exit_code(&output), Some(2), "{args:?}");
        let error: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
        assert!(
            error["error"]
                .as_str()
                .is_some_and(|text| text.contains(message)),
            "{args:?}: {error}"
        );
    }
    // --view takes only the three views.
    let bad_view = workspace
        .cli()
        .args(["schema", "localSearch", "--view", "bad"])
        .output()
        .expect("schema bad view");
    assert_eq!(exit_code(&bad_view), Some(2));

    let help = workspace
        .cli()
        .args(["localSearch", "--help"])
        .output()
        .expect("tool help");
    assert!(help.status.success(), "{}", stderr(&help));
    let help = stdout(&help);
    assert_eq!(help.matches("Required: ").count(), 1, "{help}");
    let required = help
        .lines()
        .find(|line| line.starts_with("Required: "))
        .expect("required line");
    assert!(
        required.contains("path") && required.contains("matchString"),
        "{help}"
    );
    assert!(
        help.contains("Examples:\n  octocode localSearch registerHandler src -w")
            && help.contains("\n  octocode localSearch '{"),
        "{help}"
    );
    assert!(
        help.trim_end()
            .ends_with("Fields: octocode schema localSearch (complete: --view full)"),
        "{help}"
    );

    // A bare `octocode` on a pipe is the catalog.
    let bare = workspace.cli().output().expect("bare");
    assert!(bare.status.success(), "{}", stderr(&bare));
    let bare: serde_json::Value = serde_json::from_str(stdout(&bare)).expect("bare JSON");
    assert_eq!(bare["kind"], "octocode.toolCatalog");
}
