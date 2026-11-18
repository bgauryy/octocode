use super::*;
use crate::contracts::output_fields::PAGE_ONE_RECEIPTS;
use crate::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::json;

fn walk(pages: &[Value]) -> Vec<Value> {
    pages
        .iter()
        .map(|data| {
            let mut page = json!({"results":[{"index":0,"data":data}]});
            drop_later_page_receipts(&mut page);
            page["results"][0]["data"].clone()
        })
        .collect()
}

fn answer_page(page: u64, files: Value) -> Value {
    json!({"files": files,
        "pagination":{"unit":"files","offset":page - 1,"length":1,"total":3,"hasMore":page<3},
        "coverage":{"state":"complete","indexLimit":{"maxFileBytes":393216}},
        "related":[{"kind":"related","tool":"ghGetFileContent","input":{"queries":[{"path":"a"}]}}],
        "commitSha":"abc","next":{"nextPage":{"tool":"ghSearchCode","input":{"cursor":"c"}}}})
}

/// A tiny-window walk: the first page states the receipts, no later page
/// does, and every page keeps its rows, its locator, its own `related`
/// leads and its `next` call.
#[test]
fn receipts_ride_the_first_page_only() {
    let files = (1..=3)
        .map(|n| json!([{"path": format!("f{n}")}]))
        .collect::<Vec<_>>();
    let pages = (1..=3)
        .map(|page| answer_page(page, files[page as usize - 1].clone()))
        .collect::<Vec<_>>();
    let walked = walk(&pages);
    for (at, data) in walked.iter().enumerate() {
        assert_eq!(data["files"], files[at], "page {}: rows whole", at + 1);
        assert_eq!(data["commitSha"], "abc", "page {}: locator", at + 1);
        assert!(
            data["next"]["nextPage"].is_object(),
            "page {}: next",
            at + 1
        );
        for receipt in &PAGE_ONE_RECEIPTS {
            assert_eq!(
                data.get(receipt.member).is_some(),
                at == 0,
                "page {}: {}",
                at + 1,
                receipt.member
            );
        }
    }
}

/// A read paged by offset is a later page too; a partial coverage is a
/// disclosure and stays on every page.
#[test]
fn a_disclosure_and_an_offset_page_are_judged_by_their_own_state() {
    let mut partial = answer_page(2, json!([]));
    partial["coverage"] = json_fixture(r#"{"state": "partial", "reason": "filesSkipped"}"#);
    let mut unit = json_fixture(
        r#"{
            "content": "x",
            "pagination": {
                "unit": "lines",
                "offset": 40,
                "length": 40,
                "hasMore": true
            },
            "related": [{"kind": "related"}]
        }"#,
    );
    unit["coverage"] = json_fixture(r#"{"state": "complete"}"#);
    let walked = walk(&[partial, unit]);
    assert_eq!(walked[0]["coverage"]["state"], "partial");
    assert!(walked[0]["related"].is_array());
    assert!(walked[1]["related"].is_array() && walked[1].get("coverage").is_none());
}

/// No tool's output schema requires a receipt, so removing it never makes a
/// later page invalid.
#[test]
fn no_output_schema_requires_a_receipt() {
    fn required(schema: &Value, name: &str) -> bool {
        match schema {
            Value::Object(map) => {
                map.get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|names| names.iter().any(|item| item == name))
                    || map.values().any(|member| required(member, name))
            }
            Value::Array(items) => items.iter().any(|item| required(item, name)),
            _ => false,
        }
    }
    for tool in ToolId::ALL {
        let contract = crate::contracts::tool_contract(tool).expect("embedded contract");
        for receipt in &PAGE_ONE_RECEIPTS {
            assert!(
                !required(&contract["outputSchema"], receipt.member),
                "{tool:?} requires {}",
                receipt.member
            );
        }
    }
}
