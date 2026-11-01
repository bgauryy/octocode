//! The last step of every row before validation: the shared public shape of
//! paging. Tools build page facts as they cut pages and read them back while
//! shaping; a caller sees one `pagination` object with one meaning per field,
//! and none at all when one page holds the complete result.
use serde_json::{Map, Value};

/// Keys a list or window block states about a complete page and nothing else.
const COMPLETE_KEYS: [&str; 8] = [
    "page",
    "totalPages",
    "totalItems",
    "hasMore",
    "unit",
    "offset",
    "length",
    "total",
];
/// A block on a row or nested row: `pagination`, or one file's patch window.
const BLOCKS: [&str; 2] = ["pagination", "patchPagination"];
/// A map of named blocks: one collection's page each.
const BLOCK_MAP: &str = "contentPagination";
/// What shaping stages read back from a block; `next.*` carries the cursor.
const SHAPING_ONLY: [&str; 2] = ["pageSize", "nextOffset"];
/// Row keys that carry calls, not evidence: their inputs are never rewritten.
const CALLS: [&str; 2] = ["next", "related"];

pub(crate) fn publish_rows(structured: &mut Value) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let Some(data) = row.get_mut("data") else {
            continue;
        };
        publish(data);
        if let Some(data) = data.as_object_mut() {
            drop_zero_disclosures(data);
        }
    }
}

fn publish(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(publish),
        Value::Object(fields) => publish_fields(fields),
        _ => {}
    }
}

fn publish_fields(fields: &mut Map<String, Value>) {
    for key in BLOCKS {
        if let Some(Value::Object(block)) = fields.get_mut(key)
            && published_block_is_complete(block)
        {
            fields.remove(key);
        }
    }
    if let Some(Value::Object(blocks)) = fields.get_mut(BLOCK_MAP) {
        blocks.retain(|_, block| {
            block
                .as_object_mut()
                .is_none_or(|block| !published_block_is_complete(block))
        });
        if blocks.is_empty() {
            fields.remove(BLOCK_MAP);
        }
    }
    for (key, value) in fields.iter_mut() {
        if !CALLS.contains(&key.as_str()) && key != BLOCK_MAP {
            publish(value);
        }
    }
}

/// Drops the shaping-only keys and says whether what is left is a complete page.
fn published_block_is_complete(block: &mut Map<String, Value>) -> bool {
    for key in SHAPING_ONLY {
        block.remove(key);
    }
    complete(block)
}

/// One first page with nothing after it, stating only its own size.
fn complete(block: &Map<String, Value>) -> bool {
    block.get("hasMore") == Some(&Value::Bool(false))
        && block.get("page").is_none_or(|page| page == 1)
        && block.get("offset").is_none_or(|offset| offset == 0)
        && block
            .keys()
            .all(|key| COMPLETE_KEYS.contains(&key.as_str()))
}

/// A fact that says nothing happened is absent; the row states it when it did.
fn drop_zero_disclosures(data: &mut Map<String, Value>) {
    data.retain(|key, value| match key.as_str() {
        "isPartial" => value != &Value::Bool(false),
        "filesSkipped" => value != 0,
        "diagnostics" | "warnings" | "partialReasons" => {
            value.as_array().is_none_or(|items| !items.is_empty())
        }
        _ => true,
    });
}

/// Removes from each row of the first response to a fresh request the fields
/// that repeat that row's query unchanged: `index` already maps the row to its
/// query. A value the tool resolved or changed differs and stays; a page read
/// through a cursor keeps them, because its request names none of them.
pub(crate) fn drop_echoes(structured: &mut Value, queries: &[Value], row_indices: &[usize]) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let Some(index) = row["index"].as_u64().map(crate::num::usize_of) else {
            continue;
        };
        let Some(query) = row_indices
            .iter()
            .position(|original| *original == index)
            .and_then(|position| queries.get(position))
            .and_then(Value::as_object)
        else {
            continue;
        };
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            data.retain(|key, value| query.get(key) != Some(value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_complete_first_page_states_no_block_and_every_other_page_does() {
        let mut output = json!({"results":[{"index":0,"data":{
            "pagination":{"page":1,"totalPages":1,"pageSize":9,"totalItems":3,"hasMore":false},
            "files":[{"path":"a","pagination":{"unit":"lines","offset":0,"length":2,"hasMore":false}}],
        }},{"index":1,"data":{
            "pagination":{"page":2,"totalPages":2,"pageSize":9,"totalItems":12,"hasMore":false},
        }},{"index":2,"data":{
            "pagination":{"page":1,"pageSize":9,"hasMore":true},
        }},{"index":3,"data":{
            "pagination":{"unit":"lines","offset":4,"length":2,"hasMore":false},
        }},{"index":4,"data":{
            "pagination":{"page":1,"hasMore":false,"providerLimit":{"reason":"cap"}},
        }}]});
        publish_rows(&mut output);
        let rows = output["results"].as_array().expect("rows");
        assert_eq!(rows[0]["data"], json!({"files":[{"path":"a"}]}));
        assert_eq!(
            rows[1]["data"]["pagination"],
            json!({"page":2,"totalPages":2,"totalItems":12,"hasMore":false})
        );
        assert_eq!(
            rows[2]["data"]["pagination"],
            json!({"page":1,"hasMore":true})
        );
        assert_eq!(rows[3]["data"]["pagination"]["offset"], 4);
        assert_eq!(
            rows[4]["data"]["pagination"]["providerLimit"]["reason"],
            "cap"
        );
    }

    #[test]
    fn section_maps_keep_only_the_blocks_with_more_to_read() {
        let mut output = json!({"results":[{"index":0,"data":{
            "contentPagination":{
                "files":{"page":1,"pageSize":9,"totalItems":1,"hasMore":false},
                "comments":{"unshown":true,"pageSize":9,"totalItems":4,"hasMore":true},
                "patches":{"unit":"chars","offset":0,"length":3,"total":9,"hasMore":true,"nextOffset":3},
            },
        }},{"index":1,"data":{
            "files":[{"path":"a.ts","patchPagination":{"unit":"chars","offset":0,"length":3,"total":3,"hasMore":false}}],
        }}]});
        publish_rows(&mut output);
        assert_eq!(
            output["results"][0]["data"]["contentPagination"],
            json!({
                "comments":{"unshown":true,"totalItems":4,"hasMore":true},
                "patches":{"unit":"chars","offset":0,"length":3,"total":9,"hasMore":true},
            })
        );
        assert_eq!(
            output["results"][1]["data"],
            json!({"files":[{"path":"a.ts"}]})
        );
    }

    #[test]
    fn zero_disclosures_are_absent_and_nonzero_ones_stay() {
        let mut output = json!({"results":[{"index":0,"data":{
            "isPartial":false,"filesSkipped":0,"diagnostics":[],"warnings":[],"partialReasons":[],
        }},{"index":1,"data":{
            "isPartial":true,"filesSkipped":2,"diagnostics":[{"message":"m"}],
        }}]});
        publish_rows(&mut output);
        assert_eq!(output["results"][0]["data"], json!({}));
        assert_eq!(output["results"][1]["data"]["filesSkipped"], 2);
        assert_eq!(output["results"][1]["data"]["isPartial"], true);
    }

    #[test]
    fn only_a_value_the_request_stated_unchanged_is_an_echo() {
        let queries = [
            json!({"path":"src","operation":"files"}),
            json!({"path":"/abs/src"}),
        ];
        let mut output = json!({"results":[
            {"index":1,"data":{"path":"src","operation":"tree"}},
            {"index":0,"data":{"error":"bad input","path":"src"}},
            {"index":2,"data":{"path":"/abs/src","operation":"files","files":[]}},
        ]});
        // Query positions 0 and 1 answer original rows 0 and 2; row 1 was rejected.
        drop_echoes(&mut output, &queries, &[0, 2]);
        assert_eq!(
            output["results"],
            json!([
                {"index":1,"data":{"path":"src","operation":"tree"}},
                {"index":0,"data":{"error":"bad input"}},
                {"index":2,"data":{"operation":"files","files":[]}},
            ])
        );
    }
}
