use super::*;
use serde_json::json;

#[test]
fn partial_result_store_merges_array_and_object_chunks_in_protocol_order() {
    let store = PartialResultStore::default();
    let locations = store.begin("locations".to_owned());
    store.record("locations", json!([{"uri":"a"}]));
    store.record("locations", json!([{"uri":"b"}]));
    assert_eq!(
        locations.finish(json!([{"uri":"c"}])).expect("merge"),
        json!([{"uri":"a"},{"uri":"b"},{"uri":"c"}])
    );

    let diagnostics = store.begin("diagnostics".to_owned());
    store.record("diagnostics", json!({"items":[{"message":"first"}]}));
    assert_eq!(
        diagnostics
            .finish(json!({"kind":"full","items":[{"message":"last"}]}))
            .expect("merge"),
        json!({
            "kind":"full",
            "items":[{"message":"first"},{"message":"last"}]
        })
    );
    assert_eq!(store.len(), 0);
}

#[test]
fn dropped_token_guard_releases_the_buffer() {
    let store = PartialResultStore::default();
    let guard = store.begin("t".to_owned());
    store.record(guard.token(), json!(["chunk"]));
    assert_eq!(store.len(), 1);
    drop(guard);
    assert_eq!(store.len(), 0, "a dropped request must not leak its buffer");
    // Late chunks for the released token are ignored.
    store.record("t", json!(["late"]));
    assert_eq!(store.len(), 0);
}

#[test]
fn overflow_fails_the_merge() {
    let store = PartialResultStore::default();
    let guard = store.begin("big".to_owned());
    store.record("big", json!("x".repeat(MAX_PARTIAL_RESULT_BYTES)));
    assert!(guard.finish(Value::Null).is_err());
}
