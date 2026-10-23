use super::*;

#[test]
fn uri_path_compaction_preserves_field_order_and_appends_path() {
    let tool = ToolId::from_name("lspSearch").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({}),
            json!({"type":"documentSymbols","uri":"file:///repo/src/a.ts","lsp":{},"pagination":{"hasMore":false}}),
            None,
        )],
    );
    let keys = output["results"][0]["data"]
        .as_object()
        .expect("data object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(keys, ["type", "lsp", "pagination", "path"]);
}

/// localSearch per-file entries must always carry their own
/// totalMatchRows/returnedMatchRows: hoisting them into `shared` when the
/// values coincide (typical on page 1) made the row shape data-dependent,
/// so CLI page-1 output drifted from MCP and from page-2 output.
#[test]
fn match_row_accounting_is_never_hoisted_into_shared() {
    let tool = ToolId::from_name("localSearch").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({}),
            json!({"files":[
                {"path":"/repo/a.txt","matches":[{"line":1,"column":0,"value":"x"}],"totalMatchRows":1,"returnedMatchRows":1},
                {"path":"/repo/b.txt","matches":[{"line":1,"column":0,"value":"x"}],"totalMatchRows":1,"returnedMatchRows":1}
            ]}),
            None,
        )],
    );
    for file in output["results"][0]["data"]["files"]
        .as_array()
        .expect("files")
    {
        assert_eq!(file["totalMatchRows"], 1, "{output}");
        assert_eq!(file["returnedMatchRows"], 1, "{output}");
    }
    assert!(
        output
            .get("shared")
            .and_then(|shared| shared.get("totalMatchRows"))
            .is_none(),
        "{output}"
    );
}

#[test]
fn path_compaction_never_rewrites_evidence_or_executable_queries() {
    let data = json!({"path":"/repo/src/a.ts","content":"/repo/src/a.ts","pagination":{"hasMore":true},"next":{"continueWindow":{"tool":"localFetch","query":{"queries":[{"path":"/repo/src/a.ts","offset":2}]}}}});
    let tool = ToolId::from_name("localFetch").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({"extras":["diagnostics"]}),
            data,
            None,
        )],
    );
    assert_eq!(output["root"], "/repo/src");
    assert_eq!(output["results"][0]["data"]["path"], "a.ts");
    assert_eq!(output["results"][0]["data"]["content"], "/repo/src/a.ts");
    assert_eq!(
        output.pointer("/results/0/data/next/continueWindow/query/queries/0/path"),
        Some(&json!("/repo/src/a.ts"))
    );
    assert_eq!(
        output.pointer("/results/0/meta/diagnostics"),
        Some(&json!({"partial":true}))
    );
}

#[test]
fn shared_compaction_preserves_required_pull_request_row_fields() {
    let tool = ToolId::from_name("ghSearchHistory").expect("known tool");
    let output = envelope(
        tool,
        vec![result_row(
            tool,
            0,
            &json!({"operation":"pullRequest"}),
            json!({
                "type":"pullRequests",
                "pullRequests":[
                    {"number":1,"title":"One","state":"merged","author":"octocode","labels":[],"createdAt":"2026-01-01","mergedAt":"2026-01-02","commentsCount":0},
                    {"number":2,"title":"Two","state":"merged","author":"octocode","labels":[],"createdAt":"2026-01-03","mergedAt":"2026-01-04","commentsCount":0}
                ]
            }),
            None,
        )],
    );
    for row in output["results"][0]["data"]["pullRequests"]
        .as_array()
        .expect("pull requests")
    {
        for field in [
            "number",
            "title",
            "state",
            "author",
            "labels",
            "createdAt",
            "mergedAt",
            "commentsCount",
        ] {
            assert!(row.get(field).is_some(), "missing {field}: {output}");
        }
    }
}
