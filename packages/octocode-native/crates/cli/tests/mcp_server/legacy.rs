//! The MCP listing under legacy setting names: `ENABLE_LOCAL`, `ENABLE_CLONE`
//! and `.octocoderc` `local.enableClone` switch the same tools as their
//! current names, from every source, under the same workspace trust. The
//! resolver's matrix is `config::legacy` in runtime-base.

use super::server::{Server, tool_names};
use crate::support::Workspace;
use serde_json::json;

/// Settings of one run, by source.
#[derive(Default)]
struct Scenario {
    process: &'static [(&'static str, &'static str)],
    home_env: Option<&'static str>,
    workspace_env: Option<&'static str>,
    home_rc: Option<&'static str>,
}

/// Whether `localSearch` and `ghCloneRepo` are listed under `scenario`.
fn listed(scenario: &Scenario) -> (bool, bool) {
    let workspace = Workspace::new();
    let write = |path: std::path::PathBuf, text: Option<&str>| {
        if let Some(text) = text {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    };
    write(workspace.home.join(".env"), scenario.home_env);
    write(
        workspace.workspace.join(".octocode/.env"),
        scenario.workspace_env,
    );
    write(workspace.home.join(".octocoderc"), scenario.home_rc);
    let mut command = workspace.cli();
    // The fixture sets the current local switch; each scenario sets its own.
    command
        .env_remove("OCTOCODE_ENABLE_LOCAL")
        .envs(scenario.process.iter().copied());
    let mut server = Server::spawn(command);
    server.request(
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        }),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let list = server.request("tools/list", json!({}));
    let names = tool_names(&list);
    let shown = (
        names.contains(&"localSearch"),
        names.contains(&"ghCloneRepo"),
    );
    assert!(server.finish().success());
    shown
}

#[test]
fn legacy_switches_list_the_same_tools_as_their_current_names() {
    let legacy_off_on: &[(&str, &str)] = &[("ENABLE_LOCAL", "false"), ("ENABLE_CLONE", "true")];
    let cases = [
        ("defaults", Scenario::default(), (true, false)),
        (
            "current names",
            Scenario {
                process: &[
                    ("OCTOCODE_ENABLE_LOCAL", "false"),
                    ("OCTOCODE_ENABLE_CLONE", "true"),
                ],
                ..Scenario::default()
            },
            (false, true),
        ),
        (
            "legacy names in the process",
            Scenario {
                process: legacy_off_on,
                ..Scenario::default()
            },
            (false, true),
        ),
        (
            "legacy names in the home .env",
            Scenario {
                home_env: Some("ENABLE_LOCAL=false\nENABLE_CLONE=true\n"),
                ..Scenario::default()
            },
            (false, true),
        ),
        (
            "legacy path in the home .octocoderc",
            Scenario {
                home_rc: Some(r#"{"local":{"enabled":false,"enableClone":true}}"#),
                ..Scenario::default()
            },
            (false, true),
        ),
        (
            "a workspace narrows (local off) but never widens (clone on)",
            Scenario {
                workspace_env: Some("ENABLE_LOCAL=false\nENABLE_CLONE=true\n"),
                ..Scenario::default()
            },
            (false, false),
        ),
        (
            "the current name wins in one source",
            Scenario {
                process: &[
                    ("ENABLE_LOCAL", "false"),
                    ("OCTOCODE_ENABLE_LOCAL", "true"),
                    ("ENABLE_CLONE", "true"),
                    ("OCTOCODE_ENABLE_CLONE", "false"),
                ],
                ..Scenario::default()
            },
            (true, false),
        ),
        (
            "a higher source wins whatever its spelling",
            Scenario {
                process: legacy_off_on,
                home_env: Some("OCTOCODE_ENABLE_LOCAL=true\nOCTOCODE_ENABLE_CLONE=false\n"),
                ..Scenario::default()
            },
            (false, true),
        ),
    ];
    for (name, scenario, expected) in &cases {
        assert_eq!(listed(scenario), *expected, "{name}");
    }
}
