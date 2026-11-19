//! Saved response pages preserve evidence across source removal and process restarts.
use super::lines;
use crate::support::{self, Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use serde_json::json;

#[tokio::test]
async fn immutable_pages_survive_source_removal_and_persistent_restart() {
    for restart in [false, true] {
        let workspace = Workspace::new();
        let file = workspace.write("saved.txt", lines("captured"));
        let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
        let mut runtime = workspace.runtime(&env);
        let first = runtime
            .run(
                "first".into(),
                "localSearch".into(),
                json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":800}),
            )
            .await
            .expect("capture");
        let mut next = first.structured_content["responsePagination"]["next"]["input"].clone();
        support::assert_cursor_only(&next);
        std::fs::remove_file(file).expect("remove live source");
        if restart {
            runtime.close().await;
            runtime = workspace.runtime(&env);
        }
        let mut lines = Vec::new();
        let collect = |output: &serde_json::Value, lines: &mut Vec<u64>| {
            for row in output["results"].as_array().into_iter().flatten() {
                for file in row["data"]["files"].as_array().into_iter().flatten() {
                    for hit in file["matches"].as_array().into_iter().flatten() {
                        lines.push(hit["line"].as_u64().expect("line"));
                    }
                }
            }
        };
        collect(&first.structured_content, &mut lines);
        for page in 2..=250 {
            let output = runtime
                .run(format!("page-{page}"), "localSearch".into(), next)
                .await
                .expect("immutable page")
                .structured_content;
            assert_eq!(output["responsePagination"]["replayed"], true);
            assert!(output.to_string().contains("captured"));
            collect(&output, &mut lines);
            if let Some(input) = output.pointer("/responsePagination/next/input") {
                next = input.clone();
            } else {
                break;
            }
        }
        lines.sort_unstable();
        assert_eq!(
            lines,
            (1..=200).collect::<Vec<_>>(),
            "all captured lines exactly once"
        );
        runtime.close().await;
    }
}

#[tokio::test]
async fn a_lost_memory_session_expires_without_reacquiring_the_source() {
    let workspace = Workspace::new();
    let file = workspace.write("saved.txt", lines("captured"));
    let env = [("OCTOCODE_STORAGE_MODE", "memory".into())];
    let runtime = workspace.runtime(&env);
    let first = runtime
        .run(
            "capture".into(),
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":800}),
        )
        .await
        .expect("capture");
    let next = first.structured_content["responsePagination"]["next"]["input"].clone();
    support::assert_cursor_only(&next);
    runtime.close().await;
    std::fs::remove_file(file).expect("remove source");
    let runtime = workspace.runtime(&env);
    let error = runtime
        .run("lost".into(), "localSearch".into(), next)
        .await
        .expect_err("lost session");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    runtime.close().await;
}

#[tokio::test]
async fn symbol_fragments_preserve_every_alias_and_member_once() {
    let workspace = Workspace::new();
    let aliases = (0..80)
        .map(|index| format!("Alias{index:03}_{}", "exported_name_".repeat(10)))
        .collect::<Vec<_>>();
    let methods = (0..5)
        .map(|index| format!("  method{index}(): string {{ return \"member {index}\"; }}\n"))
        .collect::<String>();
    let exports = aliases
        .iter()
        .map(|name| format!("  Container as {name},\n"))
        .collect::<String>();
    let file = workspace.write(
        "class.ts",
        format!("export class Container {{\n{methods}}}\nexport {{\n{exports}}};\n"),
    );
    let runtime = workspace.runtime(&[]);
    let mut pending =
        Some(json!({"queries":[{"operation":"symbols","path":file}],"responseChars":1000}));
    let mut seen_aliases = Vec::new();
    let mut seen_members = Vec::new();
    let mut pages = 0;
    let mut oversized = false;
    while let Some(input) = pending.take() {
        assert!(pages < 200, "finite symbol fragments");
        let output = runtime
            .run(format!("symbol-{pages}"), "astSearch".into(), input)
            .await
            .expect("symbol fragment")
            .structured_content;
        support::checks::assert_output_contract("astSearch", &output);
        oversized |= output.get("oversized").is_some();
        // Each declaration is a line-first outline string (Q5, formed by the
        // hoist stage); a member follows its parent one depth deeper, so a
        // later part that holds only members still names each one.
        for row in output["results"].as_array().expect("rows") {
            for line in row["data"]["symbols"]
                .as_array()
                .unwrap_or_else(|| panic!("symbols: {output}"))
            {
                let (depth, symbol) = line
                    .as_str()
                    .and_then(octocode_runtime_base::response::symbol_outline::outline_row)
                    .unwrap_or_else(|| panic!("an outline line: {line}"));
                if depth == 0 {
                    assert_eq!(symbol["symbolName"], "Container");
                    for alias in symbol["exportedAs"].as_array().into_iter().flatten() {
                        seen_aliases.push(alias.as_str().unwrap().to_owned());
                    }
                } else {
                    seen_members.push((
                        symbol["symbolName"].as_str().unwrap().to_owned(),
                        symbol["line"].as_u64().unwrap(),
                    ));
                }
            }
        }
        pending = output.pointer("/responsePagination/next/input").cloned();
        pages += 1;
    }
    // Q5: the declaration's outline string, every alias in it, is one
    // indivisible value: it rides a page alone and that page states it
    // oversized; its members page after it.
    assert!(pages > 1, "the members page after the declaration");
    assert!(
        oversized,
        "the indivisible outline string is disclosed as oversized"
    );
    let mut expected = aliases;
    expected.push("Container".into());
    assert_eq!(seen_aliases, expected);
    assert_eq!(
        seen_members,
        (0..5)
            .map(|index| (format!("method{index}"), index + 2))
            .collect::<Vec<_>>()
    );
    runtime.close().await;
}
