//! Container shapes: whole reads with window controls, nested envelopes, encoded containers and scalar array items.

use super::assert_executable;
use crate::support::{self, Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use octocode_native::contracts;
use serde_json::json;

/// A whole-file read with a window control: the repair drops exactly the
/// controls, keeping the whole read.
#[tokio::test]
async fn a_full_read_with_window_controls_leads_to_the_whole_read() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "one\ntwo\n");
    let runtime = workspace.runtime(&[]);
    let input =
        json!({"queries":[{"path": file, "fullContent": true, "length": 100, "unit": "lines"}]});
    let error = runtime
        .run("full".into(), "localFetch".into(), input)
        .await
        .expect_err("fullContent takes no window");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localFetch", repair);
    assert!(
        repair.get("confidence").is_none(),
        "exact repair is unqualified: {payload}"
    );
    assert_eq!(
        repair["query"],
        json!({"queries":[{"path": file, "fullContent": true}]}),
        "{payload}"
    );
    runtime.close().await;
}

/// A row that is itself an envelope is refused, never unwrapped: the
/// repair lifts its rows and keeps its envelope fields beside them.
#[tokio::test]
async fn a_nested_envelope_leads_to_the_flat_call() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "one\n");
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[{"responseLength": 3000, "queries":[{"path": file}]}]});
    let error = runtime
        .run("nested".into(), "localFetch".into(), input)
        .await
        .expect_err("nested envelope");
    let payload = error.payload.as_deref().expect("payload");
    assert!(!payload.to_string().contains("prepare_many"), "{payload}");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localFetch", repair);
    assert_eq!(
        repair["query"],
        json!({"queries":[{"path": file}], "responseLength": 3000}),
        "{payload}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn encoded_containers_are_rejected_and_offer_only_schema_valid_repairs() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, input, decoded) in [
        (
            "ghSearchCode",
            json!({"queries":[{"owner":"o","repo":"r","keywords":"[\"one\",\"two\"]"}]}),
            json!({"queries":[{"owner":"o","repo":"r","keywords":["one","two"]}]}),
        ),
        (
            "localFetch",
            json!({"queries":"[{\"path\":\"a.rs\",\"ranges\":[\"1-2\"]}]"}),
            json!({"queries":[{"path":"a.rs","ranges":["1-2"]}]}),
        ),
    ] {
        let error = runtime
            .run("encoded".into(), tool.into(), input.clone())
            .await
            .expect_err("strict admission rejects JSON text");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        let repair = support::checks::hint(error.payload.as_deref().expect("payload"), "repair");
        assert_executable(tool, &repair);
        assert_eq!(repair["query"], decoded);
        assert!(
            contracts::prepare_many_and_validate(tool, input).is_err(),
            "repair never changes admission"
        );
    }
    for encoded in ["[\"unfinished", "{}", "[null]"] {
        let input = json!({"queries":[{"owner":"o","repo":"r","keywords":encoded}]});
        let error = runtime
            .run("invalid-encoded".into(), "ghSearchCode".into(), input)
            .await
            .expect_err("invalid input");
        assert!(
            support::checks::hint(error.payload.as_deref().expect("payload"), "repair").is_null()
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn scalar_array_items_offer_a_complete_repair_without_changing_admission() {
    let workspace = Workspace::new();
    let file = workspace.write("source.txt", "one\ntwo\n");
    let runtime = workspace.runtime(&[]);
    for (tool, input, expected) in [
        (
            "ghSearchCode",
            json!({"queries":[{"owner":"owner","repo":"repo","keywords":"a phrase","pageSize":"10"}]}),
            json!({"queries":[{"owner":"owner","repo":"repo","keywords":["a phrase"],"pageSize":10}]}),
        ),
        (
            "ghSearchHistory",
            json!({"queries":[{"operation":"issue","owner":"owner","repo":"repo","keywords":"exact phrase"}]}),
            json!({"queries":[{"operation":"issue","owner":"owner","repo":"repo","keywords":["exact phrase"]}]}),
        ),
        (
            "localFetch",
            json!({"queries":[{"path":file,"ranges":"1-2"}]}),
            json!({"queries":[{"path":file,"ranges":["1-2"]}]}),
        ),
    ] {
        let error = runtime
            .run("scalar-array".into(), tool.into(), input.clone())
            .await
            .expect_err("strict admission rejects scalar array fields");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        let repair = support::checks::hint(error.payload.as_deref().expect("payload"), "repair");
        assert_executable(tool, &repair);
        assert_eq!(repair["query"], expected);
        assert!(contracts::prepare_many_and_validate(tool, input).is_err());
        if tool == "localFetch" {
            runtime
                .run("repaired".into(), tool.into(), repair["query"].clone())
                .await
                .expect("repair executes against the real local file");
        }
    }
    runtime.close().await;
}
