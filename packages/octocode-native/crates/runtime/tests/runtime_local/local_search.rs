//! Search semantics and executable handoffs through the runtime.
use crate::support::{Run as _, Workspace, call, checks, row_data, row_status};
use serde_json::json;

#[tokio::test]
async fn literal_and_regex_matchers_have_explicit_distinct_semantics() {
    let workspace = Workspace::new();
    workspace.write("src/example.txt", "a.c\nabc\nx|y\nx\ny\n");
    let runtime = workspace.runtime(&[]);
    for (engine, expected) in [("literal", vec![1, 3]), ("rust", vec![1, 2, 3, 4, 5])] {
        let found=call(&runtime,"localSearch",json!({"path":workspace.workspace,"matchString":["a.c","x|y"],"regexEngine":engine,"contextLines":{"before":0,"after":0}})).await.expect("search");
        assert_eq!(row_status(&found), "success");
        let lines = row_data(&found)["files"][0]["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|row| row["line"].as_u64().expect("line"))
            .collect::<Vec<_>>();
        assert_eq!(lines, expected, "{engine}: {}", found.structured_content);
    }
    runtime.close().await;
}

#[tokio::test]
async fn excluded_source_is_reachable_by_an_explicit_related_request() {
    let workspace = Workspace::new();
    for name in [
        "src/core.c",
        "drivers/target/transport.c",
        "dist/bundle.js",
        "Cargo.lock",
        "vendor/lib.min.js",
    ] {
        workspace.write(name, "transport_free_cmd\n");
    }
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":["transport_free_cmd"],"view":"files"}),
    )
    .await
    .expect("search");
    let paths = |data: &serde_json::Value| {
        data["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|file| file["path"].as_str().expect("path").to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        paths(row_data(&found)),
        ["Cargo.lock".into(), "src/core.c".into()].into()
    );
    assert!(
        !row_data(&found)["skippedPaths"]["entries"]
            .as_array()
            .expect("disclosure")
            .is_empty()
    );
    let lead = checks::related_call(row_data(&found), "localSearch", |input| {
        input["queries"][0]["defaultExcludes"] == false
    });
    let all = call(&runtime, "localSearch", lead["input"].clone())
        .await
        .expect("recovery executes");
    assert_eq!(
        paths(row_data(&all)),
        [
            "src/core.c",
            "drivers/target/transport.c",
            "dist/bundle.js",
            "Cargo.lock",
            "vendor/lib.min.js"
        ]
        .map(str::to_owned)
        .into()
    );
    runtime.close().await;
}

#[tokio::test]
async fn long_unicode_search_values_arrive_complete_without_expansion_reads() {
    let workspace = Workspace::new();
    let line = format!(
        "{}needleHere{} END_OF_SOURCE",
        "é ".repeat(30_000),
        " tail".repeat(1_000)
    );
    workspace.write("src/minified.txt", format!("head\n{line}\ntail\n"));
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":["needleHere"]}),
    )
    .await
    .expect("search");
    let data = row_data(&found);
    assert_eq!(data["files"][0]["matches"][0]["value"], line);
    assert!(data["files"][0]["matches"][0].get("clipped").is_none());
    assert!(
        checks::leads(data)
            .into_iter()
            .all(|lead| lead.kind != "continue" || lead.tool != "localFetch")
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_reader_match_limit_discloses_the_limit_and_links_to_search() {
    let workspace = Workspace::new();
    let path = workspace.write(
        "hits.txt",
        (1..=100_005)
            .map(|n| format!("hit {n}\n"))
            .collect::<String>(),
    );
    let runtime = workspace.runtime(&[]);
    for mcp in [false, true] {
        let input = json!({"queries":[{"path":path,"matchString":["hit"],"regexEngine":"rust"}]});
        let envelope = if mcp {
            runtime
                .run_mcp("limit".into(), "localFetch".into(), input)
                .await
                .expect("MCP")["structuredContent"]
                .clone()
        } else {
            runtime
                .run("limit".into(), "localFetch".into(), input)
                .await
                .expect("CLI")
                .structured_content
                .into_value()
        };
        let data = &envelope["results"][0]["data"];
        assert!(
            data["warnings"]
                .as_array()
                .expect("limit disclosed")
                .iter()
                .any(|warning| warning
                    .as_str()
                    .is_some_and(|text| text.contains("match limit"))),
            "{data}"
        );
        let search = checks::related_call(data, "localSearch", |input| {
            input["queries"][0]["matchString"] == json!(["hit"])
        });
        let recovered = call(&runtime, "localSearch", search["input"].clone())
            .await
            .expect("search executes");
        assert_ne!(row_status(&recovered), "error");
        assert!(
            !checks::leads(&recovered.structured_content).is_empty(),
            "remaining matches are reachable"
        );
    }
    runtime.close().await;
}
