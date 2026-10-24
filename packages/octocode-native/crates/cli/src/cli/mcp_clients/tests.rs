use super::*;
use serde_json::json;
use std::collections::BTreeSet;

fn spec(id: &str) -> &'static ClientSpec {
    client(id).expect("supported client")
}

fn dirs(codex_home: Option<&str>) -> Dirs {
    Dirs {
        home: PathBuf::from("/h"),
        app_support: PathBuf::from("/as"),
        config: PathBuf::from("/c"),
        codex_home: codex_home.map(PathBuf::from),
    }
}

fn path_in(dirs: &Dirs, id: &str) -> PathBuf {
    (spec(id).home)(dirs)
}

#[test]
fn the_registry_has_one_row_per_unique_client_id() {
    let ids = CLIENTS.iter().map(|spec| spec.id).collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), CLIENTS.len());
    for spec in &CLIENTS {
        assert_eq!(client(spec.id).map(|found| found.id), Some(spec.id));
    }
    assert!(client("").is_none());
    assert!(client("Cursor").is_none(), "ids match exactly");
}

#[test]
fn every_client_config_path_follows_its_documented_location() {
    let d = dirs(None);
    let storage = "/as/Code/User/globalStorage";
    let expected = [
        ("cursor", "/h/.cursor/mcp.json"),
        ("claude-desktop", "/as/Claude/claude_desktop_config.json"),
        ("claude-code", "/h/.claude.json"),
        ("windsurf", "/h/.codeium/windsurf/mcp_config.json"),
        ("trae", "/as/Trae/mcp.json"),
        ("antigravity", "/h/.gemini/antigravity/mcp_config.json"),
        (
            "vscode-cline",
            &format!("{storage}/saoudrizwan.claude-dev/settings/cline_mcp_settings.json"),
        ),
        (
            "vscode-roo",
            &format!("{storage}/rooveterinaryinc.roo-cline/settings/mcp_settings.json"),
        ),
        ("vscode-continue", "/h/.continue/mcpServers/octocode.json"),
        ("zed", "/c/zed/settings.json"),
        ("opencode", "/c/opencode/opencode.json"),
        ("gemini-cli", "/h/.gemini/settings.json"),
        ("kiro", "/h/.kiro/settings/mcp.json"),
        ("codex", "/h/.codex/config.toml"),
    ];
    assert_eq!(expected.len(), CLIENTS.len(), "every client is pinned");
    for (id, path) in expected {
        assert_eq!(path_in(&d, id), PathBuf::from(path), "{id}");
    }
}

#[test]
fn codex_home_moves_the_codex_config() {
    assert_eq!(
        path_in(&dirs(Some("/custom")), "codex"),
        PathBuf::from("/custom/config.toml")
    );
}

#[test]
fn opencode_prefers_plain_json_and_falls_back_to_an_existing_jsonc() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut d = dirs(None);
    d.config = root.path().to_path_buf();
    let folder = root.path().join("opencode");
    std::fs::create_dir_all(&folder).expect("folder");
    assert_eq!(path_in(&d, "opencode"), folder.join("opencode.json"));
    std::fs::write(folder.join("opencode.jsonc"), "{}").expect("jsonc");
    assert_eq!(path_in(&d, "opencode"), folder.join("opencode.jsonc"));
    std::fs::write(folder.join("opencode.json"), "{}").expect("json");
    assert_eq!(path_in(&d, "opencode"), folder.join("opencode.json"));
}

#[test]
fn home_path_is_rooted_in_the_user_home_directory() {
    let Some(home) = std::env::home_dir() else {
        return;
    };
    assert_eq!(
        spec("cursor").home_path(),
        Some(home.join(".cursor").join("mcp.json"))
    );
    assert_eq!(
        spec("claude-code").home_path(),
        Some(home.join(".claude.json"))
    );
}

#[test]
fn app_support_follows_the_platform_convention() {
    let home = Path::new("/h");
    if cfg!(target_os = "macos") {
        assert_eq!(
            app_support_dir(home),
            PathBuf::from("/h/Library/Application Support")
        );
    } else if cfg!(windows) {
        assert!(app_support_dir(home).is_absolute() || std::env::var_os("APPDATA").is_some());
    } else if std::env::var_os("XDG_CONFIG_HOME").is_none() {
        assert_eq!(app_support_dir(home), PathBuf::from("/h/.config"));
    }
}

#[test]
fn scopes_list_home_first_then_the_scopes_a_client_supports() {
    let scopes = |id| spec(id).scopes().collect::<Vec<_>>();
    assert_eq!(scopes("claude-desktop"), ["home"]);
    assert_eq!(scopes("cursor"), ["home", "workspace"]);
    assert_eq!(scopes("claude-code"), ["home", "workspace", "local"]);
    for spec in &CLIENTS {
        assert_eq!(spec.scopes().next(), Some("home"), "{}", spec.id);
        assert_eq!(
            spec.scopes().any(|scope| scope == "workspace"),
            spec.workspace.is_some(),
            "{}",
            spec.id
        );
        assert_eq!(
            spec.scopes().any(|scope| scope == "local"),
            spec.local,
            "{}",
            spec.id
        );
    }
}

#[test]
fn the_config_dir_override_only_applies_to_clients_that_declare_one() {
    for spec in CLIENTS.iter().filter(|spec| spec.config_dir_env.is_none()) {
        assert!(!spec.config_dir_overridden(), "{}", spec.id);
    }
    let declared = spec("claude-code");
    assert_eq!(declared.config_dir_env, Some("CLAUDE_CONFIG_DIR"));
    assert_eq!(
        declared.config_dir_overridden(),
        std::env::var_os("CLAUDE_CONFIG_DIR").is_some()
    );
}

#[test]
fn the_server_entry_is_keyed_by_the_clients_server_map() {
    assert_eq!(spec("cursor").server_keys(), ["mcpServers", "octocode"]);
    assert_eq!(spec("zed").server_keys(), ["context_servers", "octocode"]);
    assert_eq!(spec("codex").server_keys(), ["mcp_servers", "octocode"]);
}

#[test]
fn entry_shapes_name_their_env_keys() {
    for (shape, env) in [
        (EntryShape::Stdio, "env"),
        (EntryShape::Untyped, "env"),
        (EntryShape::Opencode, "environment"),
    ] {
        assert_eq!(shape.env_key(), env, "{shape:?}");
    }
}

#[test]
fn only_opencode_documents_with_a_servers_map_are_nested() {
    let opencode = spec("opencode");
    assert!(opencode.nested_servers(&json!({"mcp": {"servers": {}}})));
    assert!(!opencode.nested_servers(&json!({"mcp": {"octocode": {}}})));
    assert!(!opencode.nested_servers(&json!({})));
    assert!(!spec("cursor").nested_servers(&json!({"mcp": {"servers": {}}})));
}

#[test]
fn enable_flags_are_declared_where_the_client_documents_one() {
    let flagged = |flag| {
        CLIENTS
            .iter()
            .filter(|spec| spec.enable == Some(flag))
            .map(|spec| spec.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        flagged(EnableFlag::Disabled),
        ["vscode-cline", "vscode-roo", "kiro"]
    );
    assert_eq!(flagged(EnableFlag::Enabled), ["opencode", "codex"]);
}
