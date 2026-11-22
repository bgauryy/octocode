//! A graph walk reads the same bytes on the CLI and on MCP: every page,
//! cursor and expiry aside.

use super::server::Server;
use crate::support::Workspace;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// Zero the values that name a run, not a result: opaque cursors and their
/// expiry.
fn normalised(page: &str) -> Value {
    fn walk(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, inner) in map.iter_mut() {
                    if key == "expiresAt" {
                        *inner = Value::Null;
                    } else {
                        walk(inner);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(walk),
            Value::String(text) if text.starts_with("cursor") => *text = "CURSOR".into(),
            _ => {}
        }
    }
    let mut value: Value = serde_json::from_str(page).expect("JSON page");
    walk(&mut value);
    value
}

fn next_input(page: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(page).expect("JSON page");
    let next = &value["next"]["input"];
    let row = &value["next"]["input"];
    [next, row]
        .into_iter()
        .find(|input| input.is_object())
        .cloned()
}

#[test]
fn graph_issues_walk_matches_on_cli_and_mcp() {
    let workspace = Workspace::new();
    for file in 0..40 {
        std::fs::write(
            workspace.workspace.join(format!("m{file}.ts")),
            format!("import {{ g }} from './missing{file}';\nexport function f{file}() {{ return g({file}); }}\n"),
        )
        .unwrap();
    }
    let build = workspace
        .cli()
        .args([
            "__graph",
            "graphBuild",
            &json_fixture(r#"{"queries": [{"path": "."}]}"#).to_string(),
        ])
        .output()
        .expect("graphBuild");
    assert!(build.status.success(), "{build:?}");

    let mut command = workspace.cli();
    command.arg("__graph").env("OCTOCODE_BETA", "true");
    let mut server = Server::spawn(command);
    server.request(
        "initialize",
        json_fixture(
            r#"{
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }"#,
        ),
    );
    server.send(&json_fixture(
        r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#,
    ));

    let listing = server.request("tools/list", json!({}));
    let names: Vec<_> = listing["result"]["tools"]
        .as_array()
        .expect("graph tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(names, ["graphBuild", "graphQuery"]);
    let rejected = server.request("tools/call", json!({"name": "localFetch", "arguments": {}}));
    assert_eq!(rejected["error"]["code"], -32602);

    // Each surface follows its own cursors; saved work must outlive a CLI call.
    let first = json_fixture(
        r#"{
            "queries": [{"operation": "issues"}],
            "responseChars": 1500
        }"#,
    );
    let walk = |mut call: Box<dyn FnMut(&Value) -> String>| {
        let (mut input, mut pages) = (first.clone(), Vec::new());
        while pages.len() < 200 {
            let page = call(&input);
            let next = next_input(&page);
            pages.push(normalised(&page));
            match next {
                Some(next) => input = next,
                None => break,
            }
        }
        pages
    };
    let mcp_pages = walk(Box::new(|input| {
        let call = server.request("tools/call", json!({"name":"graphQuery","arguments":input}));
        call["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP text")
            .to_owned()
    }));
    // Persistent storage starts a warm server: the fixture owns its socket
    // namespace and stops it when the test ends.
    let persistent = workspace.persistent_cli();
    let cli_pages = walk(Box::new(|input| {
        let cli = persistent
            .cli()
            .args(["__graph", "graphQuery", &input.to_string(), "--json"])
            .output()
            .expect("CLI --json");
        let cli = String::from_utf8(cli.stdout).expect("UTF-8");
        cli.strip_suffix('\n').unwrap_or(&cli).to_owned()
    }));
    assert!(
        mcp_pages.len() > 1,
        "the walk must span pages: {mcp_pages:?}"
    );
    assert_eq!(mcp_pages, cli_pages);
    assert!(server.finish().success());
}

#[test]
fn research_mcp_cannot_enable_graph_with_beta_or_direct_calls() {
    let workspace = Workspace::new();
    let mut server = Server::start_with(&workspace, &[("OCTOCODE_BETA", "true")]);
    server.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}));
    server.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let list = server.request("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("research tools");
    assert!(tools.iter().any(|t| t["name"] == "localSearch"));
    for name in ["graphBuild", "graphQuery"] {
        assert!(!tools.iter().any(|t| t["name"] == name));
        let call = server.request("tools/call", json!({"name":name,"arguments":{}}));
        assert_eq!(call["error"]["code"], -32602);
        let resource = server.request(
            "resources/read",
            json!({"uri": format!("octocode://contract/output/{name}")}),
        );
        assert!(resource.get("error").is_some());
    }
}
