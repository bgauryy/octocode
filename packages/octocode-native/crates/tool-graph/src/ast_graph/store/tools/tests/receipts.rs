use super::*;
use octocode_test_support::json_fixture;

/// `data` as the response stage serves it: the guidance policy keeps only
/// the related calls that add to the row.
fn served(tool: ToolId, request: &Value, data: Value) -> Value {
    let mut structured = json!({"results": [{"index": 0, "data": data}]});
    octocode_runtime_base::response::workflow_lead::retain_guidance(
        tool,
        &mut structured,
        &json!({"queries": [request]}),
    );
    structured["results"][0]["data"].take()
}

/// A build of an unchanged scope answers with the snapshot it already has,
/// and says so; a build that rebuilt says nothing.
#[test]
fn an_unchanged_scope_says_its_snapshot_was_reused() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "export const a = 1;\n");
    let request = json_fixture(r#"{"path": "app"}"#);
    let first = call(&dir.0, ToolId::GraphBuild, request.clone());
    assert!(first.get("reused").is_none(), "{first}");
    let again = call(&dir.0, ToolId::GraphBuild, request.clone());
    assert_eq!(again["graph"], first["graph"], "{again}");
    assert_eq!(again["reused"], true, "{again}");
    assert_eq!(
        served(ToolId::GraphBuild, &request, again)["reused"],
        true,
        "the served row keeps it"
    );
    dir.write("app/a.ts", "export const a = 2;\n");
    let rebuilt = call(&dir.0, ToolId::GraphBuild, request);
    assert_ne!(rebuilt["graph"], first["graph"], "{rebuilt}");
    assert!(rebuilt.get("reused").is_none(), "{rebuilt}");
}

/// A build's diagnostics counts lead to their rows: each code's graphQuery
/// `diagnostics` call survives the response stage and lists that code's
/// diagnostics when run as written.
#[test]
fn a_build_with_diagnostics_leads_to_each_codes_rows() {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "import { gone } from './missing';\nexport const a = gone;\n",
    );
    let request = json_fixture(r#"{"path": "app"}"#);
    let built = call(&dir.0, ToolId::GraphBuild, request.clone());
    let codes = built["diagnostics"]
        .as_object()
        .unwrap_or_else(|| panic!("diagnostics: {built}"))
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert!(!codes.is_empty(), "{built}");
    let served = served(ToolId::GraphBuild, &request, built.clone());
    let leads = octocode_runtime_base::test_support::checks::leads(&served)
        .into_iter()
        .filter(|lead| {
            lead.tool == "graphQuery" && lead.query["queries"][0]["operation"] == "diagnostics"
        })
        .collect::<Vec<_>>();
    for code in &codes {
        let lead = leads
            .iter()
            .find(|lead| lead.query["queries"][0]["target"] == code.as_str())
            .unwrap_or_else(|| panic!("a lead to {code}: {served}"));
        let row = &lead.query["queries"][0];
        assert_eq!(row["graph"], built["graph"], "{row}");
        let listed = call(&dir.0, ToolId::GraphQuery, row.clone());
        let rows = listed["rows"].as_array().cloned().unwrap_or_default();
        assert!(!rows.is_empty(), "{code}: {listed}");
        assert!(
            rows.iter()
                .all(|diagnostic| diagnostic["code"] == code.as_str()),
            "{code}: {listed}"
        );
    }
}
