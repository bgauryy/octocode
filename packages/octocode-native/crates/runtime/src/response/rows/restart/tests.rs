use super::*;
use serde_json::json;

#[test]
fn a_stale_row_of_any_tool_requires_its_restart() {
    let query = json!({"path": "src/lib.rs", "readProgress": {"unit": "lines", "offset": 3}});
    let related = crate::response::result::restart_query(ToolId::LocalFetch, &query);
    let mut data = json!({"errorCode": "staleSnapshot", "error": "stale", "related": [related]});
    require_restart(ToolId::LocalFetch, &mut data, &query);
    assert!(data.get("related").is_none(), "{data}");
    assert_eq!(data["next"]["restart"]["kind"], "continue");
    assert_eq!(
        data["next"]["restart"]["input"]["queries"][0],
        json!({"path": "src/lib.rs"}),
        "private progress never restarts: {data}"
    );
    let mut other = json!({"errorCode": "notFound", "error": "missing"});
    require_restart(ToolId::LocalFetch, &mut other, &query);
    assert!(other.get("next").is_none());
}
