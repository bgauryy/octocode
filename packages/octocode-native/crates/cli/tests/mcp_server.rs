//! `octocode mcp` over real stdio, one process per protocol era: the
//! handshake, the listing, a tool call, the protocol error of an unlisted
//! tool, and the exit on stdin close; `prewarm` covers the language-server
//! prewarm after `initialize`, `progress` a rate-limit wait's progress
//! notifications.

use crate::support;
use serde_json::{Value, json};
use server::{Server, assert_cli_json_parity, local_search, tool_names};
use support::Workspace;

mod graph_parity;
#[cfg(unix)]
mod prewarm;
mod progress;
mod server;

#[test]
fn serves_the_2025_era_over_stdio() {
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("a.txt"), "alpha\nbeta needle\n").unwrap();
    let mut server = Server::start(&workspace);

    let init = server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    let result = &init["result"];
    assert_eq!(result["serverInfo"]["name"], "octocode-mcp");
    assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
    let instructions = octocode_native::contracts::cli_presentation()
        .expect("embedded presentation")["instructions"]
        .as_str()
        .expect("canonical instructions");
    assert!(!instructions.is_empty());
    assert_eq!(result["instructions"], instructions);
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let list = server.request("tools/list", json!({}));
    let names = tool_names(&list);
    assert!(names.contains(&"localSearch"), "{names:?}");
    assert!(!names.contains(&"run"), "only canonical tools are listed");
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "localSearch")
        .unwrap();
    assert!(
        tool["inputSchema"].is_object(),
        "MCP declares its input schema"
    );
    assert!(list["result"].get("_meta").is_none());

    let call = server.request("tools/call", local_search("."));
    assert_eq!(call["result"]["isError"], false, "{call}");
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("a.txt") && text.contains("beta needle"),
        "{text}"
    );
    assert!(call["result"].get("structuredContent").is_none(), "{call}");
    assert!(serde_json::from_str::<Value>(text).unwrap().is_object());

    let one_value = server.request(
        "tools/call",
        json!({"name":"localSearch","arguments":{"queries":[{"path":".","matchString":"needle"}]}}),
    );
    assert_eq!(
        one_value["result"]["isError"], false,
        "one matcher has one reading: {one_value}"
    );
    let invalid = server.request(
        "tools/call",
        json!({"name":"localSearch","arguments":{"queries":[{"path":".","needle":"needle"}]}}),
    );
    assert_eq!(
        invalid["result"]["isError"], true,
        "an unknown field stays rejected: {invalid}"
    );
    let unlisted = server.request("tools/call", json!({"name": "noSuchTool", "arguments": {}}));
    assert_eq!(unlisted["error"]["code"], -32602);
    assert_eq!(unlisted["error"]["message"], "Tool noSuchTool not found");

    assert!(server.finish().success());
}

#[test]
fn every_result_shape_sends_json_once_and_matches_cli() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.workspace.join("a.txt"),
        (0..200)
            .map(|line| format!("needle {line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut server = Server::start(&workspace);
    server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    for (tool, input) in [
        ("localFetch", json!({"queries":[{"path":"a.txt"}]})),
        (
            "localFetch",
            json!({"queries":[{"path":"a.txt"},{"path":"missing.txt"}]}),
        ),
        ("localFetch", json!({"queries":[{"path":"missing.txt"}]})),
        (
            "localFetch",
            json!({"queries":[{"path":"a.txt","matchstring":"x"}]}),
        ),
    ] {
        assert_cli_json_parity(&workspace, &mut server, tool, input);
    }
    // A separate paged execution has a fresh opaque cursor and expiry. The
    // same-fixture projection test in cli/output.rs pins its exact bytes.
    let paged = server.request(
        "tools/call",
        json!({"name":"localSearch","arguments":{"queries":[{"path":"a.txt","matchString":["needle"]}],"responseChars":900}}),
    );
    assert!(
        paged["result"].get("structuredContent").is_none(),
        "{paged}"
    );
    assert_eq!(paged["result"]["content"].as_array().map(Vec::len), Some(1));
    assert!(
        paged["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("responsePagination"))
    );
    assert!(server.finish().success());
}

#[test]
fn serves_the_2026_era_over_stdio() {
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("a.txt"), "beta needle\n").unwrap();
    let mut server = Server::start(&workspace);
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
    });

    let discover = server.request("server/discover", json!({"_meta": meta}));
    let result = &discover["result"];
    assert!(
        result["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!("2026-07-28")),
        "{result}"
    );
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "octocode-mcp"
    );

    let list = server.request("tools/list", json!({"_meta": meta}));
    assert!(tool_names(&list).contains(&"localSearch"));
    assert_eq!(list["result"]["resultType"], "complete");
    assert_eq!(
        list["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["title"],
        "Octocode MCP"
    );

    let mut params = local_search(".");
    params["_meta"] = meta;
    let call = server.request("tools/call", params);
    assert_eq!(call["result"]["resultType"], "complete", "{call}");
    assert!(
        call["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("beta needle")
    );
    assert!(
        call["result"]["_meta"]
            .get("io.modelcontextprotocol/serverInfo")
            .is_some()
    );

    assert!(server.finish().success());
}

/// The instructions name the tool flow by URI: a host reads it on demand, so
/// an agent without the skills installed can still reach it.
#[test]
fn the_tool_flow_is_readable_as_a_resource() {
    let workspace = Workspace::new();
    let mut server = Server::start(&workspace);
    let init = server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    assert!(
        init["result"]["capabilities"]["resources"].is_object(),
        "{init}"
    );
    let uri = "octocode://research/tool-flow";
    assert!(
        init["result"]["instructions"]
            .as_str()
            .is_some_and(|text| text.contains(uri)),
        "{init}"
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let listed = server.request("resources/list", json!({}));
    let listed_uris: Vec<&str> = listed["result"]["resources"]
        .as_array()
        .expect("resources")
        .iter()
        .filter_map(|resource| resource["uri"].as_str())
        .collect();
    assert!(listed_uris.contains(&uri), "{listed}");
    let references = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../skills/octocode-research/references");
    // Follow every sibling link from the tool flow: each must be listed and
    // read back as its skill source file. A link with a path segment points
    // into the installed skill, outside the served references.
    let mut pending = vec![uri.to_string()];
    let mut read_uris = Vec::new();
    while let Some(current) = pending.pop() {
        if read_uris.contains(&current) {
            continue;
        }
        let read = server.request("resources/read", json!({"uri": current}));
        let text = read["result"]["contents"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{current} is not readable: {read}"));
        let file = current.rsplit('/').next().expect("file");
        let file = if file.ends_with(".md") {
            file.to_string()
        } else {
            format!("{file}.md")
        };
        assert_eq!(
            text,
            std::fs::read_to_string(references.join(&file)).expect("skill file"),
            "{current}"
        );
        assert!(
            listed_uris.contains(&current.as_str()),
            "{current} unlisted"
        );
        for target in markdown_links(text) {
            let path = target.split('#').next().unwrap_or_default();
            if !path.is_empty() && !path.contains('/') {
                pending.push(format!("octocode://research/{path}"));
            }
        }
        read_uris.push(current);
    }
    assert!(read_uris.len() > 1, "the tool flow links no reference");
    let unknown = server.request("resources/read", json!({"uri": "octocode://nope"}));
    assert!(unknown["error"].is_object(), "{unknown}");
    assert!(server.finish().success());
}

/// The target of every inline `[text](target)` link in a markdown text.
fn markdown_links(text: &str) -> Vec<&str> {
    text.split("](")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .collect()
}

#[test]
fn a_rejected_call_leaves_the_server_serving() {
    let workspace = Workspace::new();
    let mut server = Server::start(&workspace);
    server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let empty = server.request(
        "tools/call",
        json!({"name": "localSearch", "arguments": {"queries": []}}),
    );
    assert_eq!(empty["result"]["isError"], true, "{empty}");
    assert_eq!(server.request("ping", json!({}))["result"], json!({}));

    assert!(server.finish().success());
}
