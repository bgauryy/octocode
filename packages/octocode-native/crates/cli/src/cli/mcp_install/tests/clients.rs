use super::*;

#[test]
fn client_picker_retries_invalid_input_and_uses_the_shared_registry() {
    let mut input = std::io::Cursor::new(b"0\nnot-a-number\n1\n");
    let mut output = Vec::new();
    let labels = labels();
    let selected =
        super::super::select_client(&labels, &mut input, &mut output).expect("select client");
    assert_eq!(selected, Some(mcp_clients::CLIENTS[0].id));
    let text = String::from_utf8(output).expect("UTF-8 output");
    for client in &mcp_clients::CLIENTS {
        assert!(text.contains(client.id));
    }
    assert_eq!(text.matches("Enter a number from").count(), 2);
}

#[test]
fn client_picker_cancels_on_eof_or_empty_input() {
    for input in [b"".as_slice(), b"\n", b" \r\n"] {
        assert_eq!(
            super::super::select_client(
                &labels(),
                &mut std::io::Cursor::new(input),
                &mut Vec::new()
            )
            .expect("cancel selection"),
            None
        );
    }
}

#[test]
fn client_picker_bounds_input() {
    let input = vec![b'1'; 1025];
    assert!(
        super::super::select_client(&labels(), &mut std::io::Cursor::new(input), &mut Vec::new())
            .is_err()
    );
}

#[test]
fn passed_env_names_are_config_contract_names() {
    assert!(super::super::local_enabled_env().is_some());
    assert!(octocode_native::config::ENV_TOKEN_VARS.contains(&super::super::PASS_ENV_TOKEN));
    let spec = super::super::server_spec(&super::super::InstallArgs {
        enable_local: Some(false),
        ..default_args()
    });
    assert_eq!(
        spec.env
            .get(super::super::local_enabled_env().unwrap_or_default()),
        Some(&"false".to_owned())
    );
}

#[test]
fn client_ids_resolve_exactly_and_near_misses_are_named() {
    assert_eq!(spec("claude-desktop").id, "claude-desktop");
    assert_eq!(spec("cursor").id, "cursor");
    assert!(mcp_clients::client("claude").is_none());
    assert!(mcp_clients::client("vscode").is_none());
    let message = super::super::unknown_client_message("claude");
    assert!(
        message.contains("claude-code") && message.contains("claude-desktop"),
        "{message}"
    );
}

#[test]
fn client_formats_route_by_extension() {
    assert_eq!(spec("cursor").format, ConfigFormat::Json);
    assert_eq!(spec("codex").format, ConfigFormat::Toml);
}

#[test]
fn codex_has_a_config_path() {
    assert!(
        spec("codex")
            .home_path()
            .expect("codex config path")
            .ends_with(".codex/config.toml")
    );
}

#[test]
fn client_paths_match_their_documented_configuration_files() {
    for (ide, suffix) in [
        ("kiro", ".kiro/settings/mcp.json"),
        ("vscode-continue", ".continue/mcpServers/octocode.json"),
        ("opencode", "opencode/opencode.json"),
    ] {
        let path = spec(ide).home_path().expect("path");
        assert!(
            path.ends_with(suffix)
                || ide == "opencode" && path.ends_with("opencode/opencode.jsonc"),
            "{ide}"
        );
    }
}
