use super::*;
use serde_json::json;

#[test]
fn restarts_remove_only_declared_private_progress_and_are_new_work() {
    let mut row = json!({"path":"src","operation":"tree","maxDepth":4});
    for field in crate::contracts::state_fields(ToolId::StructureSearch) {
        row[field] = json!("state");
    }
    let restart = restart_query(ToolId::StructureSearch, &row);
    assert_eq!(restart["kind"], "related");
    assert_eq!(
        restart["input"],
        json!({"queries":[{"path":"src","operation":"tree","maxDepth":4}]})
    );
}

#[test]
fn explicit_kind_routes_error_work_independently_of_names() {
    let resume = Continuation::continued(ToolId::LocalFetch, json!({"path":"a"})).build();
    let related = Continuation::related(ToolId::LocalFetch, json!({"path":"b"})).build();
    let mut data = json!({"error":"source failed"});
    attach_calls(
        &mut data,
        json!({"restart":resume.clone(),"nextPage":related.clone()}),
    );
    assert_eq!(data["next"]["restart"], resume);
    assert_eq!(data["related"], json!([related]));
}

#[test]
fn stale_snapshots_keep_the_error_and_offer_only_explicit_reacquisition() {
    let restart = restart_query(ToolId::LocalFetch, &json!({"path":"a","snapshot":"prior"}));
    let row = stale_snapshot(restart.clone());
    assert_eq!(row["errorCode"], "staleSnapshot");
    assert!(row["error"].is_string());
    assert_eq!(row["related"], json!([restart]));
    assert!(row.get("next").is_none());
}

#[test]
fn field_changes_preserve_unmodified_values_and_remove_only_explicit_nulls() {
    assert_eq!(
        with_changes(
            json!({"path":"a","page":2,"snapshot":"s"}),
            &json!({"page":3,"snapshot":null})
        ),
        json!({"path":"a","page":3})
    );
}

#[test]
fn typed_call_bundles_serialize_only_by_explicit_semantics() {
    #[derive(serde::Serialize)]
    struct Output {
        #[serde(flatten, serialize_with = "super::serialize_calls")]
        calls: Option<serde_json::Map<String, Value>>,
    }
    let calls=serde_json::json!({
        "arbitraryEvidenceName":Continuation::continued(ToolId::LocalFetch,serde_json::json!({"path":"a"})).build(),
        "nextPage":Continuation::related(ToolId::StructureSearch,serde_json::json!({"path":"."})).build()
    }).as_object().unwrap().clone();
    let output = serde_json::to_value(Output { calls: Some(calls) }).unwrap();
    assert_eq!(output["next"]["arbitraryEvidenceName"]["kind"], "continue");
    assert!(output["next"].get("nextPage").is_none());
    assert_eq!(output["related"][0]["kind"], "related");
    assert_eq!(
        serde_json::to_value(Output { calls: None }).unwrap(),
        serde_json::json!({})
    );
}
