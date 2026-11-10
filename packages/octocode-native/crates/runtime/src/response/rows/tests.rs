use super::*;
use crate::id::ToolId;
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
    let mut row = json!({"data":{"hints":["existing"],"partialReasons":["filesSkipped"]}});
    let diagnostics = crate::response::result::ToolDiagnostics {
        partial_reasons: vec![crate::response::paging::PartialReason::ProviderResultCap],
        hints: vec!["complete guidance ".repeat(100)],
        ..Default::default()
    };
    attach_diagnostics(&mut row, diagnostics);
    let data = &row["data"];
    assert_eq!(data["isPartial"], true);
    assert_eq!(
        data["partialReasons"],
        json!(["filesSkipped", "providerResultCap"])
    );
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

/// One error shape: whichever order a tool built `error` and `errorCode` in,
/// the row states them first and in the same order, the rest unchanged.
#[test]
fn every_error_row_leads_with_its_code_then_its_message() {
    let rest = json!({"httpStatus":404,"hints":["check the name"]});
    for built in [
        json!({"error":"gone","errorCode":"notFound","httpStatus":404,"hints":["check the name"]}),
        json!({"httpStatus":404,"errorCode":"notFound","hints":["check the name"],"error":"gone"}),
    ] {
        let row = result_row(0, built, Some("error"));
        let keys = row["data"]
            .as_object()
            .expect("data")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys[..2], ERROR_LEAD);
        for (name, value) in rest.as_object().expect("rest") {
            assert_eq!(row["data"][name], *value);
        }
        assert_eq!(keys.len(), 4);
    }
}
