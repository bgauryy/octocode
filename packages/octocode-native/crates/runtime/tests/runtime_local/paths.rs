use crate::support::{Run, Workspace, checks};
use octocode_native::runtime::Channel;
use serde_json::{Value, json};

async fn invoke(
    runtime: &octocode_native::runtime::ToolRuntime,
    channel: Channel,
    tool: &str,
    input: Value,
) -> Value {
    let value = match channel {
        Channel::Cli => runtime
            .run("paths-cli".into(), tool.into(), input)
            .await
            .expect("CLI result")
            .structured_content
            .into_value(),
        Channel::Mcp => runtime
            .run_mcp("paths-mcp".into(), tool.into(), input)
            .await
            .expect("MCP result")["structuredContent"]
            .clone(),
    };
    checks::assert_output_contract(tool, &value);
    value
}

#[tokio::test]
async fn local_evidence_paths_share_workspace_and_external_spelling_on_both_channels() {
    let workspace = Workspace::new();
    let outside = Workspace::new();
    for fixture in [&workspace, &outside] {
        fixture.write("nested/source/lib.rs", "pub fn evidence() {}\n");
        fixture.write("nested/source/.hidden.rs", "pub fn hidden() {}\n");
        fixture.write("nested/source/.aws/lib.rs", "pub fn withheld() {}\n");
    }
    let runtime = workspace.runtime(&[(
        "ALLOWED_PATHS",
        outside.workspace.to_string_lossy().into_owned(),
    )]);
    for (scope, prefix) in [
        (
            workspace.workspace.join("nested/source"),
            "nested/source".to_owned(),
        ),
        (
            outside.workspace.join("nested/source"),
            outside
                .workspace
                .join("nested/source")
                .to_string_lossy()
                .into_owned(),
        ),
    ] {
        for (tool, query) in [
            ("structureSearch", json!({"operation":"files","path":scope})),
            (
                "localSearch",
                json!({"path":scope,"matchString":["evidence"]}),
            ),
            ("astSearch", json!({"operation":"symbols","path":scope})),
            ("localFetch", json!({"path":scope.join("lib.rs")})),
        ] {
            let input = json!({"queries":[query]});
            let cli = invoke(&runtime, Channel::Cli, tool, input.clone()).await;
            let mcp = invoke(&runtime, Channel::Mcp, tool, input).await;
            assert_eq!(cli, mcp, "both interfaces expose the same {tool} evidence");
            let data = &cli["results"][0]["data"];
            let expected_file = format!("{prefix}/lib.rs");
            match tool {
                "localFetch" => assert_eq!(data["path"], expected_file, "{data}"),
                "structureSearch" => assert_eq!(data["path"], prefix, "{data}"),
                _ => assert_eq!(data["files"][0]["path"], expected_file, "{data}"),
            }
            let skipped = data["skippedPaths"]["entries"].as_array();
            if matches!(tool, "structureSearch" | "localSearch") {
                assert!(
                    skipped.is_some(),
                    "{tool} discloses its skipped scope: {data}"
                );
            }
            if let Some(skipped) = skipped {
                assert!(!skipped.is_empty(), "{data}");
                for entry in skipped {
                    let path = entry["path"].as_str().expect("skipped path");
                    assert!(
                        path == prefix || path.starts_with(&format!("{prefix}/")),
                        "{entry}"
                    );
                }
            }
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn saved_local_pages_keep_relative_evidence_and_complete_the_same_scope_on_both_channels() {
    let workspace = Workspace::new();
    let expected = (0..18)
        .map(|index| {
            let path = format!("nested/source/item_{index:02}.rs");
            workspace.write(&path, "pub fn evidence() {}\n");
            path
        })
        .collect::<std::collections::BTreeSet<_>>();
    workspace.write("nested/source/.hidden.rs", "pub fn hidden() {}\n");
    let runtime = workspace.runtime(&[]);
    for channel in [Channel::Cli, Channel::Mcp] {
        let mut pending = vec![json!({"queries":[{
            "operation":"files","path":workspace.workspace.join("nested/source"),
            "entryType":"file","maxEntries":2
        }],"responseChars":500})];
        let mut seen = std::collections::BTreeSet::new();
        let mut entries = std::collections::BTreeSet::new();
        while let Some(input) = pending.pop() {
            assert!(seen.insert(input.to_string()), "continuations cannot cycle");
            let page = invoke(&runtime, channel, "structureSearch", input).await;
            for row in page["results"].as_array().expect("rows") {
                let data = &row["data"];
                for group in data["files"].as_array().expect("file groups") {
                    assert_eq!(group["dir"], "nested/source");
                    for entry in group["files"].as_array().expect("entries") {
                        let entry = entry.as_str().expect("entry text");
                        let name = entry.rsplit_once(" (").map_or(entry, |(name, _)| name);
                        assert!(
                            entries.insert(format!("nested/source/{name}")),
                            "duplicate: {entry}"
                        );
                    }
                }
                if let Some(next) = data.get("next").and_then(Value::as_object) {
                    for call in next.values() {
                        assert_eq!(call["tool"], "structureSearch");
                        pending.push(call["input"].clone());
                    }
                }
            }
            if let Some(next) = page.pointer("/responsePagination/next/input") {
                pending.push(next.clone());
            }
        }
        assert!(seen.len() > 2, "walk expansion and saved pages both ran");
        assert_eq!(entries, expected);
    }
    runtime.close().await;
}
