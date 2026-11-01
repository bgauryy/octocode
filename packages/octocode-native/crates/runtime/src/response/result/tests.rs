use super::*;
use serde_json::json;

#[test]
fn related_calls_keep_distinct_work_once_in_first_seen_order() {
    let search = Continuation::related(
        ToolId::LocalSearch,
        json!({"path":"src","matchString":["exported"]}),
    )
    .build();
    let broader = Continuation::related(
        ToolId::LocalSearch,
        json!({"path":"src","matchString":["exported"],"includeIgnored":true}),
    )
    .build();
    let resume = Continuation::continued(ToolId::LocalFetch, json!({"path":"src/a.rs"})).build();
    let mut row = json!({"content":"unchanged evidence","related":[search.clone()]});
    attach_related(&mut row, search.clone());
    attach_related(&mut row, broader.clone());
    attach_calls(
        &mut row,
        json!({"anotherReason": search.clone(), "page": resume.clone()}),
    );
    assert_eq!(row["related"], json!([search, broader]));
    assert_eq!(row["next"]["page"], resume);
    assert_eq!(row["content"], "unchanged evidence");
}

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

/// Every tool's related call publishes without the private progress its
/// contract marks `x-cursor`, whatever row it was built from; continued work
/// keeps it for its cursor.
#[test]
fn related_calls_of_every_tool_publish_no_private_progress() {
    let mut private = 0;
    for tool in ToolId::ALL.iter().copied() {
        let fields = crate::contracts::state_fields(tool);
        private += fields.len();
        let mut row = json!({"path":"src"});
        for field in fields {
            row[field] = json!("state");
        }
        let related = Continuation::related(tool, row.clone()).build();
        let batch =
            Continuation::related_input(tool, json!({"queries":[row.clone(), row.clone()]}))
                .build();
        for published in [
            &related["input"]["queries"][0],
            &batch["input"]["queries"][1],
        ] {
            assert_eq!(published, &json!({"path":"src"}), "{tool:?}");
        }
        let continued = Continuation::continued(tool, row.clone()).build();
        for field in fields {
            assert_eq!(continued["input"]["queries"][0][field], "state", "{tool:?}");
        }
    }
    assert!(private > 0, "some tool declares private progress");
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
