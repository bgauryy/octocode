//! Coverage receipts and the one inspection notice for classified rows.
//!
//! Source acquisition can fail before a reader knows how many candidates its
//! selected view contains. The caller still needs an explicit unavailable
//! receipt, but there is no omitted body to recover and therefore no synthetic
//! unfiltered retry is added here.

use crate::id::ToolId;
use octocode_contract_types::{Selection, SelectionScope, SelectionStatus, SelectionUnit};
use serde_json::Value;

/// Add missing failure receipts and the notice for inspecting filtered evidence.
///
/// `response_query` contains only admitted rows. Contract-rejected siblings
/// are skipped while pairing those rows with the indexed result envelope.
pub(crate) fn decorate(tool: ToolId, structured: &mut Value, response_query: &Value) {
    let Some((scope, unit)) = receipt_kind(tool) else {
        return;
    };
    let queries: Vec<&Value> = match response_query.get("queries").and_then(Value::as_array) {
        Some(queries) => queries.iter().collect(),
        None => vec![response_query],
    };
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    let mut queries = queries.into_iter();
    for row in rows {
        if contract_rejection(row) {
            continue;
        }
        let Some(query) = queries.next() else {
            break;
        };
        // A refused request ran no selection: its repair, not unresolved
        // evidence, is the way on.
        if row.pointer("/data/errorCode").and_then(Value::as_str) == Some("invalidInput") {
            continue;
        }
        if query.get("clasifyContext").is_some()
            && row.get("status") == Some(&Value::String("error".into()))
            && let Some(data) = row.get_mut("data").and_then(Value::as_object_mut)
        {
            data.entry("selection").or_insert_with(|| {
                serde_json::to_value(unavailable(scope, unit, 0, "sourceUnavailable"))
                    .unwrap_or(Value::Null)
            });
        }
        if !crate::response::paging::continues_a_page(query) {
            inspection_hint(row);
        }
    }
}

/// The one static reminder that a classified view is not proof of absence,
/// on a row whose view left evidence out (it carries `next.unfiltered`). A
/// failed row judged nothing (its error and the unfiltered lead are the way
/// on). Said once per response, on the first page
/// ([`crate::response::paging::push_notice`]).
fn inspection_hint(row: &mut Value) {
    if row["status"] == "error" || !row["data"]["next"]["unfiltered"].is_object() {
        return;
    }
    crate::response::paging::push_notice(
        &mut row["data"],
        crate::contracts::classification_notice("filtered"),
    );
}

/// Isolated contract failures are not admitted and therefore have no entry in
/// `response_query`. Adapter-level invalid-input failures carry a repair call
/// and remain paired with their admitted query.
fn contract_rejection(row: &Value) -> bool {
    row.get("status") == Some(&Value::String("error".into()))
        && row.pointer("/data/errorCode").and_then(Value::as_str) == Some("invalidInput")
        && row.pointer("/data/next").is_none()
}

/// The scope and unit of each classified tool's receipt.
pub(crate) fn receipt_kind(tool: ToolId) -> Option<(SelectionScope, SelectionUnit)> {
    match tool {
        ToolId::LocalSearch | ToolId::LocalFetch | ToolId::GhGetFileContent => {
            Some((SelectionScope::SelectedView, SelectionUnit::Chunks))
        }
        ToolId::GhSearchCode => Some((SelectionScope::ProviderPage, SelectionUnit::Files)),
        _ => None,
    }
}

/// The receipt of a view nothing judged: all `total` items are unknown.
pub(crate) fn unavailable(
    scope: SelectionScope,
    unit: SelectionUnit,
    total: usize,
    reason: &str,
) -> Selection {
    let total = i64::try_from(total).unwrap_or(i64::MAX);
    Selection {
        status: SelectionStatus::Unavailable,
        scope,
        unit,
        total_items: total,
        selected_items: 0,
        excluded_items: 0,
        unknown_items: total,
        reason: Some(reason.into()),
        failures: Vec::new(),
        usage: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_filtered_view_gets_one_inspection_notice_on_its_first_page() {
        for tool in [
            ToolId::LocalSearch,
            ToolId::LocalFetch,
            ToolId::GhSearchCode,
            ToolId::GhGetFileContent,
        ] {
            let mut structured = json!({"results":[{"index":0,"data":{
                "selection":{"status":"partial","selectedItems":1,"unknownItems":1},
                "next":{"unfiltered":{"tool":tool.as_str(),"query":{"queries":[{"path":"a.rs"}]}}}
            }}]});
            let query =
                json!({"path":"a.rs","clasifyContext":{"classifiers":["States the rule?"]}});
            let original_lead = structured["results"][0]["data"]["next"]["unfiltered"].clone();
            settle(tool, &mut structured, &query);
            let data = &structured["results"][0]["data"];
            assert_eq!(
                data["warnings"],
                json!([crate::contracts::classification_notice("filtered")]),
                "one notice on a successful row: {data}"
            );
            assert_eq!(data["next"]["unfiltered"], original_lead);
            let once = structured.clone();
            settle(tool, &mut structured, &query);
            assert_eq!(structured, once, "no duplicate recovery tips");
            // A continuation page repeats no static reminder.
            let mut later = json!({"results":[{"index":0,"data":{
                "selection":{"status":"partial","selectedItems":1,"unknownItems":1},
                "next":{"unfiltered":{"tool":tool.as_str(),"query":{"queries":[{"path":"a.rs"}]}}}
            }}]});
            let mut page = query.clone();
            page["snapshot"] = json!("s");
            settle(tool, &mut later, &page);
            assert!(
                later["results"][0]["data"].get("warnings").is_none(),
                "{later}"
            );
        }
    }

    #[test]
    fn a_view_that_left_nothing_out_does_not_prompt_more_reading() {
        let mut structured = json!({"results":[{"index":0,"data":{
            "selection":{"status":"complete","selectedItems":1,"totalItems":1,"unknownItems":0}
        }}]});
        let before = structured.clone();
        decorate(
            ToolId::LocalFetch,
            &mut structured,
            &json!({"path":"a.rs","clasifyContext":{"classifiers":["States the rule?"]}}),
        );
        assert_eq!(structured, before);
    }

    /// [`decorate`] and the stage's notice placement.
    fn settle(tool: ToolId, structured: &mut Value, query: &Value) {
        decorate(tool, structured, query);
        crate::response::notices::place(structured, "results");
    }

    /// An empty row gets the reminder after its own tips; a failed row
    /// judged nothing, so it gets none. Two rows say it once.
    #[test]
    fn empty_rows_get_the_reminder_once_and_failed_rows_none() {
        for hints in [
            json!(["Existing tip."]),
            json!({"text":["Existing tip."],"read":{"tool":"localFetch","query":{"queries":[{"path":"a.rs"}]}}}),
        ] {
            let row = |index: usize, status: &str| {
                json!({"index":index,"status":status,"data":{
                    "selection":{"status":"partial","selectedItems":0,"unknownItems":1},
                    "hints":hints.clone(),
                    "next":{"unfiltered":{"tool":"localFetch","query":{"queries":[{"path":"a.rs"}]}}}
                }})
            };
            let mut structured =
                json!({"results":[row(0, "error"), row(1, "empty"), row(2, "empty")]});
            let query = json!({"path":"a.rs"});
            settle(
                ToolId::LocalFetch,
                &mut structured,
                &json!({"queries":[query.clone(), query.clone(), query]}),
            );
            let text = |index: usize| structured["results"][index]["data"]["hints"]["text"].clone();
            assert_eq!(
                structured["results"][0]["data"]["hints"], hints,
                "failed: no reminder"
            );
            assert_eq!(text(1)[0], "Existing tip.", "{structured}");
            assert_eq!(
                text(1)[1],
                crate::contracts::classification_notice("filtered"),
                "{structured}"
            );
            assert_eq!(
                structured["results"][2]["data"]["hints"], hints,
                "said once: {structured}"
            );
            for index in 0..3 {
                let data = &structured["results"][index]["data"];
                assert!(data.get("warnings").is_none() && data.get("notices").is_none());
                if hints.is_object() {
                    assert_eq!(data["hints"]["read"], hints["read"]);
                }
            }
        }
    }

    #[test]
    fn classified_source_failure_gets_reconciled_unavailable_receipt() {
        let mut structured = json!({"results":[{"index":0,"status":"error","data":{
            "path":"missing.rs","errorCode":"pathNotFound","error":"missing"
        }}]});
        decorate(
            ToolId::LocalFetch,
            &mut structured,
            &json!({"path":"missing.rs","clasifyContext":{"classifiers":["Relevant?"]}}),
        );
        let data = &structured["results"][0]["data"];
        assert_eq!(data["error"], "missing");
        assert_eq!(data["selection"]["status"], "unavailable");
        assert_eq!(data["selection"]["scope"], "selectedView");
        assert_eq!(data["selection"]["unit"], "chunks");
        assert_eq!(data["selection"]["totalItems"], 0);
        assert_eq!(data["selection"]["selectedItems"], 0);
        assert_eq!(data["selection"]["excludedItems"], 0);
        assert_eq!(data["selection"]["unknownItems"], 0);
        assert_eq!(data["selection"]["reason"], "sourceUnavailable");
        assert!(data.pointer("/next/unfiltered").is_none());
    }

    #[test]
    fn mixed_batch_pairs_only_admitted_queries_and_preserves_existing_receipts() {
        let existing = json!({
            "status":"partial","scope":"providerPage","unit":"files",
            "totalItems":2,"selectedItems":1,"excludedItems":0,"unknownItems":1
        });
        let mut structured = json!({"results":[
            {"index":0,"status":"error","data":{"errorCode":"invalidInput","error":"rejected"}},
            {"index":1,"status":"error","data":{"errorCode":"server","error":"plain failed"}},
            {"index":2,"status":"error","data":{"errorCode":"server","error":"classified failed"}},
            {"index":3,"status":"error","data":{"errorCode":"server","error":"already shaped","selection":existing}}
        ]});
        decorate(
            ToolId::GhSearchCode,
            &mut structured,
            &json!({"queries":[
                {"owner":"o","keywords":["plain"]},
                {"owner":"o","keywords":["selected"],"clasifyContext":{"classifiers":["Relevant?"]}},
                {"owner":"o","keywords":["existing"],"clasifyContext":{"classifiers":["Relevant?"]}}
            ]}),
        );
        let rows = structured["results"].as_array().unwrap();
        assert!(rows[0]["data"].get("selection").is_none());
        assert!(rows[1]["data"].get("selection").is_none());
        assert_eq!(rows[2]["data"]["selection"]["scope"], "providerPage");
        assert_eq!(rows[2]["data"]["selection"]["unit"], "files");
        assert_eq!(rows[3]["data"]["selection"], existing);
    }

    #[test]
    fn ordinary_and_unrelated_failures_are_unchanged() {
        for (tool, query) in [
            (ToolId::LocalSearch, json!({"path":".","matchString":"x"})),
            (ToolId::GhSearchRepo, json!({"keywords":["x"]})),
        ] {
            let mut structured = json!({"results":[{"index":0,"status":"error","data":{
                "errorCode":"server","error":"failed"
            }}]});
            let original = structured.clone();
            decorate(tool, &mut structured, &query);
            assert_eq!(structured, original);
        }
    }

    #[test]
    fn a_refused_view_gets_no_unknown_receipt_or_inspection_notice() {
        let mut structured = json!({"results":[{"index":0,"status":"error","data":{
            "errorCode":"invalidInput","error":"unsupported view",
            "next":{"unfiltered":{"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x"}]}}}
        }}]});
        let before = structured.clone();
        decorate(
            ToolId::LocalSearch,
            &mut structured,
            &json!({"path":".","matchString":"x","clasifyContext":{"classifiers":["Relevant?"]}}),
        );
        assert_eq!(structured, before);
    }

    #[test]
    fn each_classified_tool_has_one_receipt_kind_and_an_unavailable_receipt_leaves_all_unknown() {
        for tool in [
            ToolId::LocalSearch,
            ToolId::LocalFetch,
            ToolId::GhGetFileContent,
        ] {
            assert_eq!(
                receipt_kind(tool),
                Some((SelectionScope::SelectedView, SelectionUnit::Chunks))
            );
        }
        assert_eq!(
            receipt_kind(ToolId::GhSearchCode),
            Some((SelectionScope::ProviderPage, SelectionUnit::Files))
        );
        assert_eq!(receipt_kind(ToolId::GhSearchRepo), None);
        let receipt = unavailable(
            SelectionScope::ProviderPage,
            SelectionUnit::Files,
            2,
            "sourceUnavailable",
        );
        assert_eq!(receipt.status, SelectionStatus::Unavailable);
        assert_eq!(
            (
                receipt.total_items,
                receipt.selected_items,
                receipt.excluded_items,
                receipt.unknown_items
            ),
            (2, 0, 0, 2)
        );
        assert_eq!(receipt.reason.as_deref(), Some("sourceUnavailable"));
    }
}
