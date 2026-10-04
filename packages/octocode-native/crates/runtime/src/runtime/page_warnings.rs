//! Unfinished-result disclosure for every tool.
//!
//! A row whose `next` holds pages with a known remaining count opens its
//! `warnings` with one entry that names each count and the page reaching it,
//! e.g. `N more changed files: follow next.nextChangedFilesPage`. The counts
//! come from the row's own pagination facts; nothing is removed or moved out
//! of the row. A page without a known count, or one a tool warning already
//! names, adds nothing. The entry follows the tool's own warnings, and
//! `warnings` moves to the front of the row so an agent reading a long page
//! meets it before the data; `next` keeps its place.

use serde_json::{Map, Value};

use super::channels::PAGES_KEY;

const WARNINGS: &str = "warnings";

/// Adds the remaining-pages warning to every result row. Idempotent: the
/// entry names its pages, so a second pass finds them named.
pub(super) fn disclose_remaining_pages(structured: &mut Value) {
    for row in structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            disclose_row(data);
        }
    }
}

fn disclose_row(data: &mut Map<String, Value>) {
    let Some(Value::Object(pages)) = data.get(PAGES_KEY) else {
        return;
    };
    let named = |name: &str| {
        let reference = format!("next.{name}");
        data.get(WARNINGS)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|warning| warning.contains(&reference))
    };
    let parts = pages
        .keys()
        .filter(|name| !named(name))
        .filter_map(|name| remaining(data, name).map(|left| format!("{left}: follow next.{name}")))
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return;
    }
    let entry = Value::String(parts.join("; "));
    let warnings = match data.shift_remove(WARNINGS) {
        Some(Value::Array(mut warnings)) => {
            warnings.push(entry);
            warnings
        }
        _ => vec![entry],
    };
    data.shift_insert(0, WARNINGS.to_owned(), Value::Array(warnings));
}

/// The content axis (`contentPagination.<axis>` of an item read) a page
/// continues.
fn content_axis(page: &str) -> Option<&'static str> {
    Some(match page {
        "continueBody" => "body",
        "continueReviewBody" => "reviewBody",
        "continueCommentBody" => "commentBody",
        "continuePatch" => "patches",
        "nextChangedFilesPage" => "changedFiles",
        "nextCommentsPage" | "nextCommentPage" => "comments",
        "nextReviewsPage" => "reviews",
        "nextCommitsPage" => "commits",
        _ => return None,
    })
}

/// What page `name` has left to show, from the row's pagination facts.
fn remaining(data: &Map<String, Value>, name: &str) -> Option<String> {
    if let Some(axis) = content_axis(name) {
        return axis_left(axis, content_entry(data, axis)?);
    }
    match name {
        "nextPage" | "nextMatchPage" | "nextFilePage" | "nextFilePathsPage" | "continue" => {
            let query = data.get(PAGES_KEY)?.get(name)?.get("query");
            row_left(&with_page_facts(data.get("pagination")?, query), data)
        }
        _ => None,
    }
}

/// `contentPagination.<axis>` of the row, or of the first item that carries
/// one (a read returns its item in a one-element list).
fn content_entry<'a>(data: &'a Map<String, Value>, axis: &str) -> Option<&'a Value> {
    let entry = |value: &'a Value| value.get("contentPagination")?.get(axis);
    data.get("contentPagination")
        .and_then(|pages| pages.get(axis))
        .or_else(|| {
            data.values()
                .filter_map(Value::as_array)
                .flatten()
                .find_map(entry)
        })
}

fn has_more(entry: &Value) -> bool {
    entry.get("hasMore").and_then(Value::as_bool) == Some(true)
}

fn number(entry: &Value, keys: &[&str]) -> Option<(u64, usize)> {
    keys.iter()
        .enumerate()
        .find_map(|(index, key)| entry.get(*key)?.as_u64().map(|value| (value, index)))
}

fn counted(count: u64, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

fn more(count: u64, singular: &str, plural: &str) -> String {
    format!(
        "{count} more {}",
        if count == 1 { singular } else { plural }
    )
}

/// Items left after the current page: `total - currentPage * perPage`.
fn items_left(entry: &Value, totals: &[&str]) -> Option<(u64, usize)> {
    items_left_per(
        entry,
        totals,
        &["itemsPerPage", "perPage", "pageSize", "entriesPerPage"],
    )
}

fn items_left_per(entry: &Value, totals: &[&str], pers: &[&str]) -> Option<(u64, usize)> {
    let (total, which) = number(entry, totals)?;
    let (per, _) = number(entry, pers)?;
    let (current, _) = number(entry, &["currentPage"])?;
    let left = total.checked_sub(current.checked_mul(per)?)?;
    (left > 0).then_some((left, which))
}

/// Chars left after the shown window: `totalChars - nextCharOffset`.
fn chars_left(entry: &Value) -> Option<u64> {
    let (total, _) = number(entry, &["totalChars"])?;
    let (offset, _) = number(entry, &["nextCharOffset"])?;
    total.checked_sub(offset).filter(|left| *left > 0)
}

fn axis_left(axis: &str, entry: &Value) -> Option<String> {
    if !has_more(entry) {
        return None;
    }
    match axis {
        "patches" => {
            if let Some((files, _)) = number(entry, &["unfinishedFiles"]).filter(|(n, _)| *n > 0) {
                return Some(counted(files, "unfinished patch", "unfinished patches"));
            }
            chars_left(entry).map(|left| format!("{left} more patch chars"))
        }
        "body" | "reviewBody" | "commentBody" => {
            let label = match axis {
                "body" => "body",
                "reviewBody" => "review body",
                _ => "comment body",
            };
            chars_left(entry).map(|left| format!("{left} more {label} chars"))
        }
        _ => {
            let (singular, plural) = match axis {
                "changedFiles" => ("changed file", "changed files"),
                "comments" => ("comment", "comments"),
                "reviews" => ("review", "reviews"),
                _ => ("commit", "commits"),
            };
            items_left(
                entry,
                &[
                    "totalItems",
                    "totalComments",
                    "totalReviews",
                    "totalCommits",
                ],
            )
            .map(|(left, _)| more(left, singular, plural))
        }
    }
}

/// A row `pagination` with the facts a tool left only in its page query
/// (deduplicated): the shown page is the queried page minus one, and the
/// page size is the queried one.
fn with_page_facts(pagination: &Value, query: Option<&Value>) -> Value {
    let mut facts = pagination.clone();
    let (Some(object), Some(query)) = (facts.as_object_mut(), query) else {
        return facts;
    };
    if !object.contains_key("currentPage")
        && let Some(page) = query.get("page").and_then(Value::as_u64)
    {
        object.insert("currentPage".into(), Value::from(page.saturating_sub(1)));
    }
    const PER: [&str; 4] = ["itemsPerPage", "perPage", "pageSize", "entriesPerPage"];
    if !PER.iter().any(|key| object.contains_key(*key))
        && let Some(size) = query.get("pageSize").and_then(Value::as_u64)
    {
        object.insert("pageSize".into(), Value::from(size));
    }
    facts
}

/// A row-level `pagination`: lines or chars left in a read (its total from
/// the pagination, else from the row), items left in a listing, or else whole
/// pages left.
fn row_left(pagination: &Value, data: &Map<String, Value>) -> Option<String> {
    if !has_more(pagination) {
        return None;
    }
    if let Some((offset, _)) = number(pagination, &["nextOffset"]) {
        let lines = pagination.get("chunkType").and_then(Value::as_str) != Some("chars");
        let key = if lines { "totalLines" } else { "totalChars" };
        let total = pagination
            .get(key)
            .or_else(|| data.get(key))
            .and_then(Value::as_u64)?;
        let left = total.checked_sub(offset).filter(|left| *left > 0)?;
        return Some(if lines {
            more(left, "line", "lines")
        } else {
            format!("{left} more chars")
        });
    }
    if let Some((left, _)) = items_left_per(pagination, &["totalFiles"], &["filesPerPage"]) {
        return Some(more(left, "file", "files"));
    }
    const TOTALS: [&str; 4] = ["totalMatches", "totalItems", "totalFiles", "totalEntries"];
    const NOUNS: [(&str, &str); 4] = [
        ("match", "matches"),
        ("item", "items"),
        ("file", "files"),
        ("entry", "entries"),
    ];
    if let Some((left, which)) = items_left(pagination, &TOTALS) {
        let (singular, plural) = NOUNS[which];
        return Some(more(left, singular, plural));
    }
    let (pages, _) = number(pagination, &["totalPages"])?;
    let (current, _) = number(pagination, &["currentPage"])?;
    let left = pages.checked_sub(current).filter(|left| *left > 0)?;
    Some(more(left, "page", "pages"))
}

#[cfg(test)]
mod tests {
    use super::disclose_remaining_pages;
    use serde_json::{Value, json};

    fn page(tool: &str) -> Value {
        json!({"tool": tool, "query": {"page": 2}})
    }

    fn disclosed(data: Value) -> Value {
        let mut structured = json!({"results": [{"index": 0, "data": data}]});
        disclose_remaining_pages(&mut structured);
        let once = structured.clone();
        disclose_remaining_pages(&mut structured);
        assert_eq!(structured, once, "idempotent");
        structured["results"][0]["data"].take()
    }

    #[test]
    fn a_pull_request_page_names_every_remaining_count_first() {
        let data = disclosed(json!({
            "type": "pullRequests",
            "pullRequests": [{"number": 1, "contentPagination": {
                "patches": {"hasMore": true, "unfinishedFiles": 12},
                "changedFiles": {"currentPage": 1, "itemsPerPage": 30, "totalItems": 37,
                    "hasMore": true, "nextPage": 2}
            }}],
            "isPartial": true,
            "next": {
                "continuePatch": page("ghGetHistoryItem"),
                "nextChangedFilesPage": page("ghGetHistoryItem")
            }
        }));
        assert_eq!(
            data["warnings"],
            json!([
                "12 unfinished patches: follow next.continuePatch; 7 more changed files: follow next.nextChangedFilesPage"
            ]),
            "{data}"
        );
        assert_eq!(
            data.as_object()
                .and_then(|object| object.keys().next())
                .map(String::as_str),
            Some("warnings"),
            "the disclosure leads the row: {data}"
        );
        assert!(
            data["next"]["continuePatch"].is_object(),
            "pages stay: {data}"
        );
    }

    #[test]
    fn listings_reads_and_bodies_count_what_is_left() {
        let listing = disclosed(json!({
            "files": [{"path": "a"}],
            "pagination": {"currentPage": 1, "totalPages": 2, "totalFiles": 2,
                "totalMatches": 62, "hasMore": true},
            "warnings": ["Regex trap."],
            "next": {"nextPage": page("localSearch")}
        }));
        assert_eq!(
            listing["warnings"],
            json!(["Regex trap.", "1 more page: follow next.nextPage"]),
            "{listing}"
        );
        let files = disclosed(json!({
            "pagination": {"currentPage": 1, "totalPages": 7, "filesPerPage": 3,
                "totalFiles": 20, "totalMatches": 78, "hasMore": true},
            "next": {"nextPage": page("localSearch")}
        }));
        assert_eq!(
            files["warnings"],
            json!(["17 more files: follow next.nextPage"])
        );
        let deduplicated = disclosed(json!({
            "pagination": {"totalMatches": 99, "totalPages": 20, "hasMore": true},
            "next": {"nextPage": {"tool": "ghSearchHistory",
                "query": {"keywords": ["miri"], "page": 2, "pageSize": 5}}}
        }));
        assert_eq!(
            deduplicated["warnings"],
            json!(["94 more matches: follow next.nextPage"])
        );
        assert!(deduplicated["pagination"].get("currentPage").is_none());
        let search = disclosed(json!({
            "pagination": {"currentPage": 1, "perPage": 10, "totalMatches": 25, "hasMore": true},
            "next": {"nextPage": page("ghSearchHistory")}
        }));
        assert_eq!(
            search["warnings"],
            json!(["15 more matches: follow next.nextPage"])
        );
        let read = disclosed(json!({
            "content": "1\tx",
            "pagination": {"chunkType": "lines", "offset": 0, "chunkSize": 381,
                "totalLines": 386, "hasMore": true, "nextOffset": 381},
            "next": {"continue": page("localFetch")}
        }));
        assert_eq!(
            read["warnings"],
            json!(["5 more lines: follow next.continue"])
        );
        let whole = disclosed(json!({
            "path": "a.py",
            "totalLines": 3137,
            "next": {"continue": page("localFetch")},
            "pagination": {"chunkType": "lines", "offset": 0, "length": 471,
                "chunkSize": 2000, "hasMore": true, "nextOffset": 471},
            "isPartial": true
        }));
        assert_eq!(
            whole["warnings"],
            json!(["2666 more lines: follow next.continue"])
        );
        let issue = disclosed(json!({
            "issues": [{"number": 3, "contentPagination": {"body": {"charOffset": 0,
                "charLength": 12000, "totalChars": 14366, "hasMore": true,
                "nextCharOffset": 12000}}}],
            "next": {"continueBody": page("ghGetHistoryItem")}
        }));
        assert_eq!(
            issue["warnings"],
            json!(["2366 more body chars: follow next.continueBody"])
        );
    }

    #[test]
    fn pages_without_a_count_or_already_named_add_nothing() {
        let named = json!({
            "warnings": ["binarySkipped: 5 binary files not searched. next.binarySkipped lists them."],
            "pagination": {"currentPage": 1, "totalPages": 1, "hasMore": false},
            "next": {"binarySkipped": page("structureSearch"), "restart": page("localSearch")}
        });
        assert_eq!(disclosed(named.clone()), named);
        let finished = json!({
            "pagination": {"currentPage": 2, "totalPages": 2, "hasMore": false},
            "next": {"nextPage": page("localSearch")}
        });
        assert_eq!(disclosed(finished.clone()), finished);
        let bare = json!({"files": []});
        assert_eq!(disclosed(bare.clone()), bare);
    }
}
