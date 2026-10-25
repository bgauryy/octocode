use super::*;
use serde_json::json;

#[test]
fn every_tool_keeps_complete_row_data_without_hoisting_or_pruning() {
    let evidence = json!({"path":"/workspace/a","content":"exact\n😀","facts":[1,2,3],"nested":{"kind":"related","tool":"provider-text","input":{"evidence":true}}});
    for tool in ToolId::ALL {
        let row = result_row(3, evidence.clone(), None);
        assert_eq!(row, json!({"index":3,"data":evidence}));
        assert_eq!(envelope(tool, vec![row.clone()]), json!({"results":[row]}));
    }
}

#[test]
fn partial_diagnostics_keep_their_reasons_and_full_guidance() {
    let mut row = json!({"data":{"hints":["existing"],"partialReasons":["old"]}});
    let diagnostics = crate::response::result::ToolDiagnostics {
        partial: true,
        codes: vec!["scanFailed".into()],
        hints: vec!["complete guidance ".repeat(100)],
    };
    attach_diagnostics(&mut row, diagnostics);
    let data = &row["data"];
    assert_eq!(data["partialReasons"], json!(["old", "scanFailed"]));
    assert_eq!(data["hints"].as_array().expect("hints").len(), 2);
    assert_eq!(data["hints"][1], "complete guidance ".repeat(100));
}

#[test]
fn explicit_common_status_consumes_only_its_equal_producer_copy() {
    let evidence = json!({"status":"empty","provider":{"status":"empty"}});
    let row = result_row(0, evidence.clone(), Some("empty"));
    assert_eq!(
        row,
        json!({"index":0,"status":"empty","data":{"provider":{"status":"empty"}}})
    );
    for status in [None, Some("complete")] {
        let row = result_row(0, evidence.clone(), status);
        assert_eq!(row["data"], evidence);
    }
}
