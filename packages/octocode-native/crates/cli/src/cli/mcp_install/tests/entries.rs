use super::*;

#[test]
fn server_is_npx_latest_with_yes_flag() {
    let server = server_entry(EntryShape::Stdio, &server_spec(&default_args()));
    assert_eq!(server["command"], "npx");
    assert_eq!(server["args"], json!(["-y", "octocode-mcp@latest"]));
    assert_ne!(spec("codex").format, ConfigFormat::Json);
}

#[test]
fn server_uses_bunx_when_specified() {
    let spec = server_spec(&super::super::InstallArgs {
        method: crate::cli::mcp_manage::Method::Bunx,
        ..default_args()
    });
    let server = server_entry(EntryShape::Stdio, &spec);
    assert_eq!(server["command"], "bunx");
    assert_eq!(server["args"], json!(["octocode-mcp@latest"]));
}

/// The module promise, held by the entry's inputs: every runner
/// (`--method`) in every client's shape starts the npm server through
/// that runner, never the native host (`octo`/`octocode`) or an `mcp`
/// subcommand. Only the env varies at run time, so this is checked here,
/// not on every install.
#[test]
fn every_method_in_every_shape_runs_the_npm_server() {
    for &method in <Method as clap::ValueEnum>::value_variants() {
        for client in &mcp_clients::CLIENTS {
            let server = server_entry(client.shape, &ServerSpec::new(method));
            let (command, args) =
                command_line(client.shape, &server).expect("an entry has a command line");
            let context = format!("{} {method:?} {server}", client.id);
            assert_eq!(Method::of_command(command), Some(method), "{context}");
            assert!(
                args.iter().all(|arg| arg.as_str() != Some("mcp")),
                "{context}"
            );
            assert!(
                args.iter()
                    .any(|arg| arg.as_str() == Some("octocode-mcp@latest")),
                "{context}"
            );
        }
    }
}

#[test]
fn codex_toml_renders_mcp_servers_table_and_merges() {
    let (rendered, already) = render_fixture(spec("codex"), None, &default_args()).expect("render");
    assert_eq!(already, Existing::Absent);
    // Valid TOML with the codex-native table shape.
    let parsed: toml::Value = toml::from_str(&rendered).expect("valid toml");
    let entry = &parsed["mcp_servers"]["octocode"];
    assert_eq!(entry["command"].as_str(), Some("npx"));
    assert_eq!(
        entry["args"].as_array().expect("args is array").len(),
        2,
        "npx -y octocode-mcp@latest"
    );

    // Merges into an existing config, preserving other servers and detecting reinstall.
    let existing = "[mcp_servers.other]\ncommand = \"foo\"\nargs = []\n";
    let (merged, already_present) =
        render_fixture(spec("codex"), Some(existing), &default_args()).expect("merge");
    let parsed: toml::Value = toml::from_str(&merged).expect("valid toml");
    assert!(
        parsed["mcp_servers"].get("other").is_some(),
        "preserves other"
    );
    assert!(parsed["mcp_servers"].get("octocode").is_some());
    assert_eq!(already_present, Existing::Absent);
    let (_again, now_present) =
        render_fixture(spec("codex"), Some(&merged), &default_args()).expect("re");
    assert_eq!(
        now_present,
        Existing::Same,
        "detects identical octocode entry"
    );
    let edited = merged.replace("npx", "node");
    let (_, differs) = render_fixture(spec("codex"), Some(&edited), &default_args()).expect("diff");
    assert_eq!(differs, Existing::Different, "detects a user-edited entry");
}

#[test]
fn reinstall_is_a_noop_when_identical_and_needs_force_when_edited() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical tempdir")
        .join("mcp.json");
    assert_eq!(
        super::super::install(&home(spec("cursor"), &path), &default_args()).expect("first"),
        0
    );
    let written = std::fs::read_to_string(&path).expect("read");
    assert_eq!(
        super::super::install(&home(spec("cursor"), &path), &default_args()).expect("again"),
        0
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        written,
        "no rewrite"
    );
    std::fs::write(&path, written.replace("npx", "node")).expect("edit");
    assert_eq!(
        super::super::install(&home(spec("cursor"), &path), &default_args()).expect("edited"),
        1
    );
}
#[test]
fn client_specific_json_shapes_are_valid_and_preserve_other_settings() {
    for (ide, key) in [
        ("cursor", "mcpServers"),
        ("zed", "context_servers"),
        ("opencode", "mcp"),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join("config.json");
        std::fs::write(&path, r#"{"theme":"dark","unrelated":{"keep":true}}"#).expect("fixture");
        assert_eq!(
            super::super::install(&home(spec(ide), &path), &default_args()).expect("install"),
            0
        );
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(root["theme"], "dark");
        assert_eq!(root["unrelated"]["keep"], true);
        assert!(root[key].get("octocode").is_some());
        assert!(super::super::valid_server(
            spec(ide),
            &root,
            &spec(ide).server_keys()
        ));
        if ide == "opencode" {
            assert_eq!(
                root[key]["octocode"]["command"],
                json!(["npx", "-y", "octocode-mcp@latest"])
            );
        }
        let written = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            super::super::install(&home(spec(ide), &path), &default_args()).expect("existing"),
            0,
            "identical reinstall is a no-op"
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), written);
    }
}
