use super::*;
use serde_json::json;

#[test]
fn unresolved_anchors_preserve_requested_spelling_and_offer_explicit_related_work() {
    let q = query(
        json!({"operation":"references","path":"/repo/a.ts","symbolName":"renderX","line":100,"occurrence":99}),
    );
    let mut row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "no exact occurrence",
        true,
    );
    anchor_recovery(&mut row, &q, Some("function render() {}\nrender();"));
    assert!(row.get("next").is_none());
    let related = row["related"]
        .as_array()
        .expect("source and search recovery");
    assert_eq!(related.len(), 2);
    for call in related {
        assert_eq!(call["kind"], "related");
        crate::contracts::validate(call["tool"].as_str().expect("tool"), call["input"].clone())
            .expect("runnable related input");
    }
    let search = &related
        .iter()
        .find(|call| call["tool"] == "lspSearch")
        .expect("search")["input"]["queries"][0];
    assert_eq!(search["symbolQuery"], "renderX");
    assert!(search.get("occurrence").is_none());
    assert!(search.get("line").is_none());
}
