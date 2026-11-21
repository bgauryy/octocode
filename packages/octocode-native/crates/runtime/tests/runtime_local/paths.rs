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
        Channel::Mcp => {
            let result = runtime
                .run_mcp("paths-mcp".into(), tool.into(), input)
                .await
                .expect("MCP result");
            crate::support::mcp_payload(&result)
        }
    };
    checks::assert_output_contract(tool, &value);
    value
}

#[tokio::test]
async fn local_evidence_paths_name_one_stated_root_on_both_channels() {
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
    // Rows name paths from the workspace, or from the root of the tree
    // outside it they lie in, stated once as `root`.
    let prefix = "nested/source";
    for (scope, root) in [
        (workspace.workspace.join(prefix), None),
        (outside.workspace.join(prefix), Some(&outside.workspace)),
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
            match root {
                // A read that restates no path states no root.
                Some(root) if tool != "localFetch" || data.get("path").is_some() => {
                    assert_eq!(data["root"], json!(root), "{data}");
                }
                _ => assert!(data.get("root").is_none(), "{data}"),
            }
            match tool {
                // A row states its path only when it differs from the request's.
                "localFetch" => {
                    if let Some(path) = data.get("path") {
                        assert_eq!(path, &json!(expected_file), "{data}");
                    }
                }
                // Outside the workspace the listed path is the request's own,
                // an echo the response drops.
                "structureSearch" => match data.get("path") {
                    Some(path) => assert_eq!(path, &json!(prefix), "{data}"),
                    None => assert!(root.is_some(), "{data}"),
                },
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
            "entryType":"file","limit":2
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
            if let Some(next) = page.pointer("/next/input") {
                pending.push(next.clone());
            }
        }
        assert!(seen.len() > 2, "walk expansion and saved pages both ran");
        assert_eq!(entries, expected);
    }
    runtime.close().await;
}

/// Every file a row names, absolute: its path joined to the stated root, or
/// to the workspace that answered.
fn named_files(
    data: &Value,
    workspace: &std::path::Path,
) -> std::collections::BTreeSet<std::path::PathBuf> {
    let base = data["root"]
        .as_str()
        .map_or_else(|| workspace.to_path_buf(), std::path::PathBuf::from);
    data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["path"].as_str())
        .map(|path| base.join(path))
        .collect()
}

/// The calls a response emits: each row's `related[]` and `next.*`, and the
/// response's own next page.
fn emitted_calls(output: &Value) -> Vec<Value> {
    let mut calls = Vec::new();
    for row in output["results"].as_array().into_iter().flatten() {
        let data = &row["data"];
        calls.extend(data["related"].as_array().into_iter().flatten().cloned());
        if let Some(next) = data["next"].as_object() {
            calls.extend(next.values().cloned());
        }
    }
    calls.extend(output.pointer("/next").cloned());
    calls
}

#[tokio::test]
async fn rows_join_their_stated_root_and_emitted_calls_run_unchanged() {
    let workspace = Workspace::new();
    let outside = Workspace::new();
    let elsewhere = Workspace::new();
    for fixture in [&workspace, &outside] {
        for index in 0..4 {
            fixture.write(
                &format!("nested/source/item_{index}.rs"),
                "pub fn evidence() {}\n",
            );
        }
    }
    let allowed = format!(
        "{},{}",
        workspace.workspace.display(),
        outside.workspace.display()
    );
    let runtime = workspace.runtime(&[("ALLOWED_PATHS", allowed.clone())]);
    // The same stores, run from another workspace (another cwd).
    let from_elsewhere = elsewhere.runtime(&[
        ("ALLOWED_PATHS", allowed),
        (
            "OCTOCODE_HOME",
            workspace.home.to_string_lossy().into_owned(),
        ),
        ("HOME", workspace.home.to_string_lossy().into_owned()),
    ]);
    let mut checked = 0;
    // Inside the workspace a call names paths as rows do and runs in that
    // workspace; outside it a call keeps its absolute paths and runs from
    // anywhere.
    for scope in [&workspace.workspace, &outside.workspace] {
        let inside = scope == &workspace.workspace;
        let source = scope.join("nested/source");
        for (tool, query) in [
            (
                "localSearch",
                json!({"path":source,"matchString":["evidence"],"limit":1}),
            ),
            (
                "structureSearch",
                json!({"operation":"files","path":source,"limit":1}),
            ),
            (
                "localSearch",
                json!({"path":source,"matchString":["absent_term_zz"]}),
            ),
            ("localFetch", json!({"path":source.join("missing.rs")})),
        ] {
            let output = invoke(&runtime, Channel::Cli, tool, json!({"queries":[query]})).await;
            let data = &output["results"][0]["data"];
            for file in named_files(data, &workspace.workspace) {
                assert!(file.is_file(), "{} names no file: {data}", file.display());
            }
            for call in emitted_calls(&output) {
                let target = call["tool"].as_str().expect("call tool");
                if !inside {
                    for row in call["input"]["queries"].as_array().into_iter().flatten() {
                        if let Some(path) = row["path"].as_str() {
                            assert!(std::path::Path::new(path).is_absolute(), "{call}");
                        }
                    }
                }
                let here = invoke(&runtime, Channel::Cli, target, call["input"].clone()).await;
                // A saved page serves the policy that saved it (its allowed
                // roots); a fresh call outside the workspace runs from anywhere.
                let other = if inside || call["input"].get("cursor").is_some() {
                    &runtime
                } else {
                    &from_elsewhere
                };
                let there = invoke(other, Channel::Cli, target, call["input"].clone()).await;
                let (here, there) = (&here["results"][0], &there["results"][0]);
                assert_eq!(here["status"], there["status"], "{call}: {here} / {there}");
                assert_eq!(
                    here["data"]["errorCode"], there["data"]["errorCode"],
                    "{call}: {here} / {there}"
                );
                let base = if std::ptr::eq(other, &runtime) {
                    &workspace.workspace
                } else {
                    &elsewhere.workspace
                };
                assert_eq!(
                    named_files(&here["data"], &workspace.workspace),
                    named_files(&there["data"], base),
                    "{call}: {here} / {there}"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 4,
        "the responses emitted calls to run: {checked}"
    );
    runtime.close().await;
    from_elsewhere.close().await;
}
