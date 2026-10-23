use super::continue_main::inspect_continue_main;
use super::document::{has_inline_comment, render_toml};
use super::*;

fn spec(id: &str) -> &'static ClientSpec {
    mcp_clients::client(id).expect("supported client")
}

fn fixture(client: &'static ClientSpec, dir: &Path) -> Target {
    Target {
        client,
        scope: "home".into(),
        path: dir.join(match client.format {
            ConfigFormat::Json => "agent.json",
            ConfigFormat::Toml => "agent.toml",
        }),
        keys: client.server_keys(),
    }
}

fn initial(client: &ClientSpec) -> &'static str {
    match client.format {
        ConfigFormat::Json => {
            "{\n // Keep my theme\n \"theme\": \"dark\",\n \"other\": {\"token\":\"unrelated-secret\"}\n}\n"
        }
        ConfigFormat::Toml => {
            "# Keep my theme\ntheme = \"dark\"\n[mcp_servers.other]\ncommand = \"unrelated-secret\"\nargs = []\n"
        }
    }
}

#[test]
fn every_agent_updates_redacts_preserves_and_rejects_stale_edits() {
    for client in &CLIENTS {
        let id = client.id;
        let dir = tempfile::tempdir().expect("tempdir");
        let canonical = dir.path().canonicalize().expect("canonical path");
        let target = fixture(client, &canonical);
        let original = initial(client);
        std::fs::write(&target.path, original).expect("fixture");
        let discovered = inspect(&target);
        assert_eq!(discovered["status"], "absent", "{id}: {discovered}");
        assert_eq!(discovered["writable"], true, "{id}: {discovered}");
        let mut request = json!({"revision":discovered["revision"],"patch":{"env":{"API_KEY":"octocode-secret"},"method":"bunx"}});
        if client.enable.is_some() {
            request["patch"]["enabled"] = json!(false);
        }
        let result =
            update(&target, &request, false).unwrap_or_else(|error| panic!("{id}: {error}"));
        assert_eq!(
            result["status"],
            if client.enable.is_some() {
                "disabled"
            } else {
                "configured"
            },
            "{id}: {result}"
        );
        assert_eq!(result["entry"]["method"], "bunx");
        assert_eq!(result["entry"]["envKeys"], json!(["API_KEY"]));
        let sanitized = result.to_string();
        assert!(!sanitized.contains("octocode-secret") && !sanitized.contains("unrelated-secret"));
        let contents = std::fs::read_to_string(&target.path).expect("read");
        assert!(contents.contains("Keep my theme"), "{id}: {contents}");
        assert!(contents.contains("unrelated-secret"), "{id}: {contents}");
        assert!(matches!(
            update(&target, &request, false).expect_err("stale"),
            ManageError::Conflict(_)
        ));
        assert_eq!(
            std::fs::read_to_string(&target.path).expect("read"),
            contents
        );
        let result = update(
            &target,
            &{
                let mut patch =
                    json!({"revision":result["revision"],"patch":{"env":{"API_KEY":null}}});
                if client.enable.is_some() {
                    patch["patch"]["enabled"] = json!(true);
                }
                patch
            },
            false,
        )
        .expect("enable");
        assert_eq!(result["status"], "configured");
        assert_eq!(result["entry"]["envKeys"], json!([]));
        let result =
            update(&target, &json!({"revision":result["revision"]}), true).expect("remove");
        assert_eq!(result["status"], "absent");
        let root = parse_document(
            client.format,
            &std::fs::read_to_string(&target.path).expect("read"),
        )
        .expect("parse");
        assert_eq!(root["theme"], "dark");
    }
}

#[test]
fn all_absent_formats_can_be_created_then_removed() {
    for client in ["cursor", "codex"] {
        let dir = tempfile::tempdir().expect("dir");
        let target = fixture(spec(client), &dir.path().canonicalize().expect("canonical"));
        let result = update(
            &target,
            &json!({"revision":config_revision(None),"patch":{"method":"npx"}}),
            false,
        )
        .expect("create");
        update(&target, &json!({"revision":result["revision"]}), true).expect("remove");
    }
}

#[test]
fn malformed_and_ambiguous_files_are_read_only_without_parser_secret_echoes() {
    for (client, text) in [
        ("cursor", "{\"token\":\"dont-echo-secret\""),
        ("codex", "token = \"dont-echo-secret"),
    ] {
        let dir = tempfile::tempdir().expect("dir");
        let target = fixture(spec(client), &dir.path().canonicalize().expect("canonical"));
        std::fs::write(&target.path, text).expect("fixture");
        let result = inspect(&target);
        assert_eq!(result["status"], "invalid");
        assert_eq!(result["writable"], false);
        assert!(!result.to_string().contains("dont-echo-secret"));
    }
    let text = "mcp_servers = { octocode = { command = \"npx\", args = [\"octocode-mcp\"] } }\n";
    assert!(
        render_toml(
            text,
            &["mcp_servers".into(), "octocode".into()],
            Some(&default_entry(spec("codex"), Method::Npx))
        )
        .is_err()
    );
}

#[test]
fn untrusted_command_args_and_env_values_never_reach_projection() {
    for client in &CLIENTS {
        let projected = projection(
            client,
            Some(
                &json!({"command":["secret-executable","--token=secret-arg"],"cmd":"secret-command","args":["secret-arg"],"env":{"API":"secret-env"},"environment":{"API":"secret-env"},"envs":{"API":"secret-env"}}),
            ),
        );
        assert!(!projected.to_string().contains("secret"));
    }
}

#[test]
fn protected_env_keys_are_rejected_without_any_write() {
    let dir = tempfile::tempdir().expect("dir");
    let target = fixture(
        spec("cursor"),
        &dir.path().canonicalize().expect("canonical"),
    );
    let error=update(&target,&json!({"revision":config_revision(None),"patch":{"env":{"NODE_OPTIONS":"--import attacker"}}}),false).expect_err("protected");
    assert!(!target.path.exists());
    assert!(!error.to_string().contains("attacker"));
}

#[test]
fn concurrent_updates_have_at_most_one_success() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().canonicalize().expect("canonical");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads:Vec<_>=(0..2).map(|index| {
            let path=path.clone(); let barrier=barrier.clone();
            std::thread::spawn(move || { let target=fixture(spec("cursor"),&path); barrier.wait(); update(&target,&json!({"revision":config_revision(None),"patch":{"env":{"API":index.to_string()}}}),false) })
        }).collect();
    let successes = threads
        .into_iter()
        .map(|thread| thread.join().expect("thread"))
        .filter(Result::is_ok)
        .count();
    assert_eq!(successes, 1);
}

#[cfg(unix)]
#[test]
fn symlink_target_is_rejected_and_external_file_untouched() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().canonicalize().expect("canonical");
    let target = fixture(spec("cursor"), &path);
    let external = path.join("external.json");
    std::fs::write(&external, "{}").expect("fixture");
    std::os::unix::fs::symlink(&external, &target.path).expect("symlink");
    assert_eq!(inspect(&target)["writable"], false);
    assert!(
        update(
            &target,
            &json!({"revision":config_revision(Some("{}")),"patch":{}}),
            false
        )
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(external).expect("read"), "{}");
}

#[test]
fn nested_claude_local_entry_preserves_sibling_projects() {
    let text = "{\"projects\":{\"/a\":{\"theme\":\"dark\"},\"/b\":{\"mcpServers\":{\"other\":{\"secret\":\"keep\"}}}}}";
    let keys = [
        "projects".into(),
        "/a".into(),
        "mcpServers".into(),
        "octocode".into(),
    ];
    let client = spec("claude-code");
    let rendered = render_document(
        client,
        text,
        &keys,
        Some(&default_entry(client, Method::Npx)),
    )
    .expect("render");
    let root = parse_document(ConfigFormat::Json, &rendered).expect("parse");
    assert_eq!(root["projects"]["/a"]["theme"], "dark");
    assert_eq!(
        root["projects"]["/b"]["mcpServers"]["other"]["secret"],
        "keep"
    );
}
#[test]
fn opencode_v2_updates_the_servers_mapping_and_disabled_flag() {
    let dir = tempfile::tempdir().expect("dir");
    let target = fixture(
        spec("opencode"),
        &dir.path().canonicalize().expect("canonical"),
    );
    let text = r#"{"mcp":{"servers":{"other":{"command":["keep-secret"]}}},"theme":"dark"}"#;
    std::fs::write(&target.path, text).expect("fixture");
    let result=update(&target,&json!({"revision":config_revision(Some(text)),"patch":{"enabled":false,"env":{"API":"secret"}}}),false).expect("update V2");
    assert_eq!(result["status"], "disabled");
    let root = parse_document(
        ConfigFormat::Json,
        &std::fs::read_to_string(&target.path).expect("read"),
    )
    .expect("parse");
    assert_eq!(root["mcp"]["servers"]["octocode"]["disabled"], true);
    assert!(root["mcp"].get("octocode").is_none());
    assert_eq!(
        root["mcp"]["servers"]["other"]["command"],
        json!(["keep-secret"])
    );
}

#[test]
fn unsupported_enable_toggle_is_explicit_and_does_not_write() {
    let dir = tempfile::tempdir().expect("dir");
    let target = fixture(
        spec("claude-code"),
        &dir.path().canonicalize().expect("canonical"),
    );
    assert!(
        update(
            &target,
            &json!({"revision":config_revision(None),"patch":{"enabled":false}}),
            false
        )
        .expect_err("unsupported")
        .to_string()
        .contains("supported enable flag")
    );
    assert!(!target.path.exists());
}
#[test]
fn absent_continue_main_files_produce_no_rows() {
    let dir = tempfile::tempdir().expect("dir");
    let client = spec("vscode-continue");
    assert!(continue_main_rows(client, dir.path()).is_empty());
    std::fs::write(dir.path().join("config.yaml"), "mcpServers: []\n").expect("yaml fixture");
    assert!(
        continue_main_rows(client, dir.path()).is_empty(),
        "config.yaml is not inspected"
    );
    std::fs::write(dir.path().join("config.json"), "{}").expect("fixture");
    let rows = continue_main_rows(client, dir.path());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["scope"], "home-config-json");
}
#[test]
fn continue_main_json_is_a_sanitized_read_only_source() {
    let dir = tempfile::tempdir().expect("dir");
    let directory = dir.path().canonicalize().expect("canonical");
    let path = directory.join("config.json");
    std::fs::write(&path,r#"{"models":[{"apiKey":"unrelated-secret"}],"mcpServers":[{"name":"octocode","command":"npx","args":["octocode-mcp@latest","secret-argument"],"env":{"TOKEN":"secret-token"}}]}"#).expect("fixture");
    let result = inspect_continue_main(spec("vscode-continue"), &path);
    assert_eq!(result["configured"], true);
    assert_eq!(result["writable"], false);
    assert_eq!(result["entries"][0]["envKeys"], json!(["TOKEN"]));
    assert!(!result.to_string().contains("secret"));
    std::fs::write(&path,r#"{"experimental":{"modelContextProtocolServers":[{"transport":{"command":"npx","args":["octocode-mcp@latest"],"env":{"TOKEN":"secret-token"}}}]}}"#).expect("fixture");
    let result = inspect_continue_main(spec("vscode-continue"), &path);
    assert_eq!(result["configured"], true);
    assert_eq!(result["writable"], false);
    assert!(!result.to_string().contains("secret-token"));
    std::fs::write(&path, r#"{"mcpServers":[{"uses":"organization/block"}]}"#)
        .expect("import fixture");
    let result = inspect_continue_main(spec("vscode-continue"), &path);
    assert_eq!(result["status"], "unknown");
    assert!(result["configured"].is_null());
}
#[test]
fn jsonc_updates_preserve_comments_inside_unchanged_octocode_fields() {
    let text = "{\n \"mcpServers\": {\n \"octocode\": {\n // Keep launch explanation\n \"command\": \"npx\",\n \"args\": [\"octocode-mcp@latest\"],\n \"env\": {\n // Keep key explanation\n \"API\": \"old\",\n // Keep unrelated key\n \"OTHER\": \"keep\"\n }}}}\n";
    let root = parse_document(ConfigFormat::Json, text).expect("parse");
    let mut next = root["mcpServers"]["octocode"].clone();
    next["env"]["API"] = json!("new");
    let rendered = render_document(
        spec("cursor"),
        text,
        &["mcpServers".into(), "octocode".into()],
        Some(&next),
    )
    .expect("render");
    assert!(rendered.contains("// Keep launch explanation"));
    assert!(rendered.contains("// Keep key explanation"));
    assert!(rendered.contains("// Keep unrelated key"));
    assert_eq!(
        parse_document(ConfigFormat::Json, &rendered).expect("parse")["mcpServers"]["octocode"]["env"]
            ["API"],
        "new"
    );
}
#[test]
fn toml_inline_comment_layouts_are_explicitly_read_only() {
    let toml = "[mcp_servers.octocode]\ncommand = \"npx\" # keep runner explanation\nargs = [\"octocode-mcp@latest\"]\n";
    let error = render_toml(
        toml,
        &["mcp_servers".into(), "octocode".into()],
        Some(&default_entry(spec("codex"), Method::Npx)),
    )
    .expect_err("read only");
    assert!(error.contains("Inline comments"));
    assert!(!has_inline_comment("token = \"abc#def\""));
}
