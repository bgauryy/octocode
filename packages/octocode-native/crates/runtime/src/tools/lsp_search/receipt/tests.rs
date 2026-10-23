use super::settle_source;
use serde_json::{Value, json};

fn settled(lsp: Value, anchored: bool) -> Value {
    let mut lsp = lsp.as_object().cloned().expect("an lsp object");
    settle_source(&mut lsp, anchored);
    Value::Object(lsp)
}

/// A native scan that answered for the server keeps its label on every
/// row, anchored or not; a server answer is implied on anchored rows.
#[test]
fn a_degraded_source_survives_the_receipt_stage() {
    for anchored in [true, false] {
        let row = settled(json!({"source": "native-graph-facts"}), anchored);
        assert_eq!(row["source"], "native-graph-facts", "anchored={anchored}");
    }
    assert!(
        settled(json!({"source": "lsp"}), true)
            .get("source")
            .is_none()
    );
    assert!(settled(json!({}), true).get("source").is_none());
    assert_eq!(settled(json!({}), false)["source"], "lsp");
}
