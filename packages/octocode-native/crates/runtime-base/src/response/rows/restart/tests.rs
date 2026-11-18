use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn a_stale_row_of_any_tool_requires_its_restart() {
    let query = json_fixture(
        r#"{
            "path": "src/lib.rs",
            "readProgress": {"unit": "lines", "offset": 3}
        }"#,
    );
    let related = crate::response::result::restart_query(ToolId::LocalFetch, &query);
    let mut data = json!({"errorCode": "staleSnapshot", "error": "stale", "related": [related]});
    require_restart(ToolId::LocalFetch, &mut data, &query);
    assert!(data.get("related").is_none(), "{data}");
    assert_eq!(data["next"]["restart"]["kind"], "continue");
    assert_eq!(
        data["next"]["restart"]["input"]["queries"][0],
        json_fixture(r#"{"path": "src/lib.rs"}"#),
        "private progress never restarts: {data}"
    );
    // A tool's own required restart (a saved plan's request) stands.
    let plan = json_fixture(
        r#"{
            "kind": "continue",
            "tool": "clasify",
            "input": {"queries": [{"operation": "sections"}]}
        }"#,
    );
    let mut named =
        json!({"errorCode": "staleSnapshot", "error": "stale", "next": {"restart": plan.clone()}});
    require_restart(
        ToolId::Clasify,
        &mut named,
        &json_fixture(r#"{"assessment": "token"}"#),
    );
    assert_eq!(named["next"]["restart"], plan, "{named}");
    let mut other = json_fixture(r#"{"errorCode": "notFound", "error": "missing"}"#);
    require_restart(ToolId::LocalFetch, &mut other, &query);
    assert!(other.get("next").is_none());
}
