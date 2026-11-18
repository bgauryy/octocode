//! The last step of every row before validation: the shared public shape of
//! paging. Tools build page facts as they cut pages and read them back while
//! shaping; a caller sees one `pagination` object with one meaning per field,
//! and none at all when one page holds the complete result.
use serde_json::{Map, Value};

/// Keys a list or window block states about a complete page and nothing else.
const COMPLETE_KEYS: [&str; 6] = ["unit", "offset", "length", "total", "countScope", "hasMore"];
/// Units a page counts in text, not in rows: the reader cannot derive their
/// `length` from the rows listed, so such a block keeps `unit` and `length`.
const TEXT_UNITS: [&str; 4] = ["bytes", "chars", "characters", "lines"];
/// A block on a row or nested row: `pagination`, or one file's patch window.
const BLOCKS: [&str; 2] = ["pagination", "patchPagination"];
/// A map of named blocks: one collection's page each.
const BLOCK_MAP: &str = "sectionPagination";
/// What shaping stages read back from a block; `next.*` carries the cursor.
const SHAPING_ONLY: [&str; 1] = ["nextOffset"];
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
    // A page that remains counts rows: the rows listed are its length, and
    // the operation names the unit.
    if let Some(Value::Object(block)) = fields.get_mut("pagination")
        && block
            .get("unit")
            .and_then(Value::as_str)
            .is_some_and(|unit| !TEXT_UNITS.contains(&unit))
    {
        block.remove("unit");
        block.remove("length");
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

/// One first page with nothing after it, stating only its own size: it
/// starts at 0, holds the whole `total` (when stated), and counts all of it
/// (`countScope` absent or `complete`). A total larger than the page, or a
/// floor (`loaded`, `partial`, `capped`), keeps the block.
fn complete(block: &Map<String, Value>) -> bool {
    block.get("hasMore") == Some(&Value::Bool(false))
        && block.get("offset").is_none_or(|offset| offset == 0)
        && block
            .get("total")
            .is_none_or(|total| Some(total) == block.get("length"))
        && block
            .get("countScope")
            .is_none_or(|scope| scope == "complete")
        && block
            .keys()
            .all(|key| COMPLETE_KEYS.contains(&key.as_str()))
}

/// A row whose page lies past the end of its list or body states it as the
/// row's `status:"outOfRange"`; its `pagination` keeps the size (`total`).
/// The one place a page's `outOfRange` fact becomes the row status, run
/// before notices are placed (an `outOfRange` row states guidance in hints).
pub(crate) fn state_out_of_range(structured: &mut Value) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        // A section past its end leaves the row's other sections standing:
        // its block's offset at its total says so, and the row keeps its
        // status.
        if let Some(Value::Object(sections)) = data.get_mut(BLOCK_MAP) {
            for block in sections.values_mut().filter_map(Value::as_object_mut) {
                block.remove("outOfRange");
            }
        }
        let past_end = data
            .get_mut("pagination")
            .and_then(Value::as_object_mut)
            .and_then(|block| block.remove("outOfRange"))
            == Some(Value::Bool(true));
        if past_end && row.get("status").is_none_or(|status| status == "empty") {
            row["status"] = Value::String("outOfRange".into());
        }
    }
}

/// A fact that says nothing happened is absent; the row states it when it did.
fn drop_zero_disclosures(data: &mut Map<String, Value>) {
    drop_complete_listing_summary(data);
    state_partial_once(data);
    data.retain(|key, value| match key.as_str() {
        "isPartial" => value != &Value::Bool(false),
        "filesSkipped" => value != 0,
        "diagnostics" | "warnings" | "partialReasons" => {
            value.as_array().is_none_or(|items| !items.is_empty())
        }
        _ => true,
    });
}

/// A listing's `summary` (`fileCount`, `dirCount`, `bytes`) of a complete
/// page is its own entries counted: a page with no `pagination` and no
/// partial mark holds the whole listing. A paged or partial page keeps it,
/// the only source of the whole listing's split and size.
fn drop_complete_listing_summary(data: &mut Map<String, Value>) {
    let partial = data.get("isPartial") == Some(&Value::Bool(true))
        || data
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_some_and(|reasons| !reasons.is_empty());
    let counts = data
        .get("summary")
        .and_then(Value::as_object)
        .is_some_and(|summary| {
            summary
                .keys()
                .all(|key| matches!(key.as_str(), "fileCount" | "dirCount" | "bytes"))
        });
    if counts && !partial && !data.contains_key("pagination") {
        data.remove("summary");
    }
}

/// A partial page states it once. `partialReasons` or a `pagination`
/// whose `countScope` is a floor already say so, and then `isPartial` is
/// absent; `coverage.state:"partial"` never stands beside another statement.
fn state_partial_once(data: &mut Map<String, Value>) {
    if data.get("isPartial") != Some(&Value::Bool(true)) {
        return;
    }
    let reasons = data
        .get("partialReasons")
        .and_then(Value::as_array)
        .is_some_and(|reasons| !reasons.is_empty());
    let floor = data
        .get("pagination")
        .and_then(|page| page.get("countScope"))
        .and_then(Value::as_str)
        .is_some_and(|scope| scope != "complete");
    if reasons || floor {
        data.remove("isPartial");
    }
    if let Some(Value::Object(coverage)) = data.get_mut("coverage")
        && coverage.get("state").and_then(Value::as_str) == Some("partial")
    {
        coverage.remove("state");
        if coverage.is_empty() {
            data.remove("coverage");
        }
    }
}

/// Removes from each row the fields that repeat its query unchanged: `index`
/// already maps the row to its query. Only the first response to a fresh
/// request applies it; a page read through a cursor keeps them, because its
/// request names none of them. A value the tool resolved or changed differs
/// and stays (a branch's resolved `commitSha` stays). `pinned_commit_is_echo`
/// says the caller's rows treat a `commitSha` equal to a commit-pinned `ref`
/// as a repeat (a file read); elsewhere `commitSha` is the row's identity,
/// which the output contract requires, so it stays.
pub fn drop_echoes(
    pinned_commit_is_echo: bool,
    structured: &mut Value,
    queries: &[Value],
    row_indices: &[usize],
) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let Some(index) = row["index"]
            .as_u64()
            .map(octocode_engine::num::saturating_usize)
        else {
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
            // A read pinned to a commit states that commit as `ref`; the
            // `commitSha` it resolved to is the same text.
            if pinned_commit_is_echo
                && let Some(pinned) = query.get("ref")
                && data.get("commitSha") == Some(pinned)
            {
                data.remove("commitSha");
            }
            drop_resource_echoes(data, query);
        }
    }
}

/// A resource's `source` repeats the row's `resources.<key>` when unchanged.
fn drop_resource_echoes(
    data: &mut serde_json::Map<String, Value>,
    query: &serde_json::Map<String, Value>,
) {
    let Some(resources) = data.get_mut("resources").and_then(Value::as_object_mut) else {
        return;
    };
    for (key, resource) in resources {
        if let Some(resource) = resource.as_object_mut()
            && query.get("resources").and_then(|asked| asked.get(key)) == resource.get("source")
        {
            resource.remove("source");
        }
    }
}

#[cfg(test)]
mod tests;
