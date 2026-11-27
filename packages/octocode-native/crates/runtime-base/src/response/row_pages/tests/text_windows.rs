//! String windows exist to fit a row onto pages: a later part repeats its
//! row's context beside each window, so when that context alone fills the
//! page, cutting the string fits nothing and only multiplies the context.
//! Such a row stays whole and its page discloses it as oversized.
use super::nested_continuation::{Join, declared_collections, merge};
use super::*;
use crate::id::ToolId;
use octocode_io::cancellation::Cancellation;
use octocode_test_support::json_fixture;

/// One line of a GitHub file read, as `ghGetFileContent` shapes its row.
fn line_row() -> Value {
    json_fixture(
        r#"{
            "owner": "octo",
            "repo": "demo-docs",
            "path": "guide.txt",
            "content": "Line 1: the greeter guide repeats this sentence to fill a long document.\n",
            "totalLines": 1500,
            "fileType": "text",
            "commitSha": "9ae2c1f0b5d4e3a2918f7c6b5a4d3e2f1a0b9c8d",
            "pagination": {
                "unit": "lines",
                "offset": 0,
                "length": 1,
                "total": 3,
                "hasMore": true
            }
        }"#,
    )
}

/// Every part of `data` at `budget`, in order.
fn parts(tool: ToolId, data: &Value, budget: usize) -> Vec<Value> {
    let plan = PreparedRows::new(vec![json!({"index":0,"data":data.clone()})], tool)
        .plan(budget, &Cancellation::new())
        .expect("valid plan");
    (0..plan.total_pages)
        .flat_map(|page| {
            plan.build_page(page, &[None], &Cancellation::new())
                .expect("valid page")
        })
        .collect()
}

/// The size of the row with every string emptied: a floor under the context
/// any later part repeats beside a window.
fn context_floor(data: &Value) -> usize {
    fn empty(value: &mut Value) {
        match value {
            Value::String(text) => text.clear(),
            Value::Array(items) => items.iter_mut().for_each(empty),
            Value::Object(map) => map.values_mut().for_each(empty),
            _ => {}
        }
    }
    let mut skeleton = json!({"index":0,"data":data.clone()});
    empty(&mut skeleton);
    octocode_engine::json_size::utf16_units(&skeleton).expect("sized")
}

#[test]
fn a_row_whose_context_fills_the_page_stays_whole() {
    let data = line_row();
    let floor = context_floor(&data);
    for budget in 1..floor {
        let parts = parts(ToolId::GhGetFileContent, &data, budget);
        assert_eq!(
            parts.len(),
            1,
            "budget {budget} under the {floor}-char context: the row is cut into {} parts",
            parts.len()
        );
        assert_eq!(parts[0]["data"], data, "budget {budget}: the whole row");
    }
}

#[test]
fn string_windows_join_back_to_the_row_at_every_budget() {
    let data = line_row();
    let collections = declared_collections(ToolId::GhGetFileContent, &data);
    for budget in 1..=600 {
        let mut joined: Option<Value> = None;
        for part in parts(ToolId::GhGetFileContent, &data, budget) {
            match joined.as_mut() {
                None => joined = Some(part["data"].clone()),
                Some(joined) => merge(
                    joined,
                    part["data"].clone(),
                    "/data",
                    &Join {
                        continued: &[],
                        text: part["rowPart"]["text"].as_str(),
                        collections: &collections,
                    },
                ),
            }
        }
        assert_eq!(
            joined.as_ref(),
            Some(&data),
            "budget {budget}: parts join to the row"
        );
    }
}
