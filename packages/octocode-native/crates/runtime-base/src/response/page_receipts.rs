//! Response-level receipts are stated once, on the first page of an answer
//! ([`PAGE_ONE_RECEIPTS`]). A tool pages its own answer (`pagination.offset`
//! past 0 is a later page); each later page is a fresh execution that
//! would state the same receipts again, so the stage removes them there.
//! Locators, every `next` call and the page's own `related` leads stay: a
//! later page is cited and continued alone, and its leads name its rows.

use crate::contracts::output_fields::PAGE_ONE_RECEIPTS;
use serde_json::{Map, Value};

/// Whether `data` is a page after the first of its answer.
fn is_later_page(data: &Map<String, Value>) -> bool {
    let Some(pagination) = data.get("pagination") else {
        return false;
    };
    pagination["offset"]
        .as_u64()
        .is_some_and(|offset| offset > 0)
}

/// Removes the receipts of each result row that is a later page of its
/// answer.
pub(crate) fn drop_later_page_receipts(structured: &mut Value) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for data in rows
        .iter_mut()
        .filter_map(|row| row.get_mut("data").and_then(Value::as_object_mut))
    {
        if !is_later_page(data) {
            continue;
        }
        for receipt in &PAGE_ONE_RECEIPTS {
            if data
                .get(receipt.member)
                .is_some_and(|value| (receipt.is_receipt)(value))
            {
                data.shift_remove(receipt.member);
            }
        }
    }
}

#[cfg(test)]
mod tests;
