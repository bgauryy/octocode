//! The page contract every tool shares: one pagination block shape, the
//! unfinished-result disclosure, the stale-snapshot restart, and the facts
//! that tell whether a row has more to read.
//!
//! A row whose `next` holds pages that leave more to read opens its
//! `warnings` with one entry that names each page and what it has left,
//! e.g. `N more changed files: follow next.nextFilePage`, or
//! `more: follow next.continue` when the count is unknown. The counts come
//! from the row's own pagination facts (a page size the continuation leaves
//! at its default is the contract default); nothing is removed or moved out
//! of the row. Only a page a tool warning already names, or one whose
//! pagination says nothing is left, adds nothing; `partialReasons` say why a
//! row is partial, not how much is left, so they never replace the entry.
//! The entry follows the tool's own warnings, and `warnings` moves to the
//! front of the row so an agent reading a long page meets it before the
//! data; `next` keeps its place.

use serde_json::{Map, Value, json};

use super::channels::PAGES_KEY;
use crate::tools::id::ToolId;

const WARNINGS: &str = "warnings";

/// Adds the remaining-pages warning to every result row. Idempotent: the
/// entry names its pages, so a second pass finds them named.
pub(super) fn disclose_remaining_pages(structured: &mut Value, tool: ToolId) {
    for row in structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut)
            && data.get("complete") != Some(&Value::Bool(true))
        {
            disclose_pages(data, |name| crate::tools::id::is_remaining(tool, name));
        }
    }
}

/// Envelope `warnings`, first: a batch whose rows have pages left says so
/// before any row, e.g. `incomplete — 2 of 3 rows partial (index 1, 2);
/// follow next.continue on each`. Single-row responses already open
/// their row with the remaining-pages warning.
pub(super) fn disclose_incomplete_rows(structured: &mut Value) {
    let Some(rows) = structured.get("results").and_then(Value::as_array) else {
        return;
    };
    if rows.len() < 2 {
        return;
    }
    let mut partial = Vec::new();
    let mut pages = Vec::new();
    for row in rows {
        let mut names = Vec::new();
        if let Some(data) = row.get("data").filter(|data| !is_complete(data)) {
            page_names(data, &mut names);
        }
        if names.is_empty() {
            continue;
        }
        partial.push(row.get("index").map_or_else(String::new, Value::to_string));
        for name in names {
            if !pages.contains(&name) {
                pages.push(name);
            }
        }
    }
    if partial.is_empty() {
        return;
    }
    let follow = pages
        .iter()
        .map(|name| format!("next.{name}"))
        .collect::<Vec<_>>()
        .join("/");
    add_envelope_warning(
        structured,
        format!(
            "incomplete — {} of {} rows partial (index {}); follow {follow} on each",
            partial.len(),
            rows.len(),
            partial.join(", ")
        ),
    );
}

/// A response page that cut a row keeps its `warnings` on the first part
/// while the row's pages ride the last part: every warning naming one of
/// `moved` (`next.<name>`) names `responsePagination.next` instead, so a
/// part never points at a continuation it does not carry.
pub(crate) fn retarget_moved_pages(data: &mut Map<String, Value>, moved: &[String]) {
    if moved.is_empty() {
        return;
    }
    let Some(Value::Array(warnings)) = data.get_mut(WARNINGS) else {
        return;
    };
    for warning in warnings.iter_mut() {
        let Value::String(text) = warning else {
            continue;
        };
        let mut changed = false;
        for name in moved {
            let named = format!("next.{name}");
            if text.contains(&named) {
                *text = text.replace(&named, "responsePagination.next");
                changed = true;
            }
        }
        // Count-less lines of several pages now read the same: keep one.
        if changed {
            let mut parts: Vec<&str> = Vec::new();
            for part in text.split("; ") {
                if !parts.contains(&part) {
                    parts.push(part);
                }
            }
            *text = parts.join("; ");
        }
    }
}

/// Page names under every `next` map in a row's data (a read's file list
/// keeps them per file). Continuation calls are never entered.
fn page_names(value: &Value, names: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        if let Some(items) = value.as_array() {
            items.iter().for_each(|item| page_names(item, names));
        }
        return;
    };
    if object.get("query").is_some() && object.get("tool").is_some_and(Value::is_string) {
        return;
    }
    if let Some(Value::Object(pages)) = object.get(PAGES_KEY) {
        names.extend(pages.keys().cloned());
    }
    for (key, child) in object {
        if key != PAGES_KEY && key != super::channels::HINTS_KEY {
            page_names(child, names);
        }
    }
}

/// Appends `warning` to the envelope's `warnings`, which leads the envelope.
fn add_envelope_warning(structured: &mut Value, warning: String) {
    let Some(envelope) = structured.as_object_mut() else {
        return;
    };
    let mut warnings = match envelope.shift_remove(WARNINGS) {
        Some(Value::Array(warnings)) => warnings,
        _ => Vec::new(),
    };
    if !warnings
        .iter()
        .any(|known| known.as_str() == Some(warning.as_str()))
    {
        warnings.push(Value::String(warning));
    }
    envelope.shift_insert(0, WARNINGS.to_owned(), Value::Array(warnings));
}

/// A split row's last part carries the row's pages (each resumes after the
/// evidence shown), so it names them too, from the facts the part holds.
pub(crate) fn disclose_carried_pages(data: &mut Map<String, Value>) {
    disclose_pages(data, |_| true);
}

/// Adds the line for every page of `data` that `remains` and no warning
/// names yet.
fn disclose_pages(data: &mut Map<String, Value>, remains: impl Fn(&str) -> bool) {
    let Some(Value::Object(pages)) = data.get(PAGES_KEY) else {
        return;
    };
    // A warning naming `next.expandValues` also names its numbered
    // siblings (`expandValues2`, ...).
    let named = |name: &str| {
        let references = [name, name.trim_end_matches(|c: char| c.is_ascii_digit())]
            .map(|page| format!("next.{page}"));
        data.get(WARNINGS)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|warning| {
                references
                    .iter()
                    .any(|reference| warning.contains(reference))
            })
    };
    let parts = pages
        .iter()
        .filter(|(name, _)| remains(name) && !named(name))
        .filter_map(|(name, call)| {
            let left = match remaining(data, name, call) {
                Left::Done => return None,
                Left::Unknown => "more".to_owned(),
                Left::Counted(left) => left,
            };
            Some(format!("{left}: follow next.{name}"))
        })
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
/// continues. On an item read `nextFilePage` pages the changed files.
fn content_axis(page: &str, data: &Map<String, Value>) -> Option<&'static str> {
    Some(match page {
        "continueBody" => "body",
        "continueReviewBody" => "reviewBody",
        "continueCommentBody" => "commentBody",
        "continuePatch" => "patches",
        "nextFilePage" if content_entry(data, "files").is_some() => "files",
        "nextCommentPage" => "comments",
        "nextReviewPage" => "reviews",
        "nextCommitPage" => "commits",
        _ => return None,
    })
}

/// What a page has left to show.
enum Left {
    /// Its pagination counts what is left.
    Counted(String),
    /// More is left, by an unknown amount.
    Unknown,
    /// Its pagination says nothing is left.
    Done,
}

impl Left {
    fn of(entry: &Value, count: Option<String>) -> Self {
        match count {
            Some(count) => Self::Counted(count),
            None if has_more(entry) => Self::Unknown,
            None => Self::Done,
        }
    }
}

/// What page `name` (continuation `call`) has left to show, from the row's
/// pagination facts.
fn remaining(data: &Map<String, Value>, name: &str, call: &Value) -> Left {
    if let Some(axis) = content_axis(name, data) {
        let Some(entry) = content_entry(data, axis) else {
            return Left::Unknown;
        };
        if !has_more(entry) {
            return Left::Done;
        }
        let count = if axis == "patches" {
            patches_left(entry)
        } else {
            axis_left(axis, entry)
        };
        return Left::of(entry, count);
    }
    match name {
        "nextMatchPage" => matches_left(data).map_or(Left::Unknown, |left| {
            Left::Counted(more(left, "match", "matches"))
        }),
        "nextPage" | "nextFilePage" | "continue" => {
            let Some(pagination) = data.get("pagination") else {
                return Left::Unknown;
            };
            let facts = with_page_facts(pagination, call);
            Left::of(pagination, row_left(&facts, data, call))
        }
        _ => Left::Unknown,
    }
}

/// Matches left after the shown match page of every file on the page, from
/// each file's own `pagination`.
fn matches_left(data: &Map<String, Value>) -> Option<u64> {
    let left: u64 = data
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|item| item.get("pagination"))
        .filter(|pagination| has_more(pagination))
        .filter_map(items_left)
        .sum();
    (left > 0).then_some(left)
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

pub(crate) fn counted(count: u64, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

pub(crate) fn more(count: u64, singular: &str, plural: &str) -> String {
    format!(
        "{count} more {}",
        if count == 1 { singular } else { plural }
    )
}

/// Items left after the current page: the page's own `remainingItems`
/// (a page that does not tile `currentPage × pageSize`), else
/// `totalItems - currentPage * pageSize`.
fn items_left(entry: &Value) -> Option<u64> {
    if let Some((left, _)) = number(entry, &["remainingItems"]) {
        return (left > 0).then_some(left);
    }
    let (total, _) = number(entry, &["totalItems"])?;
    let (per, _) = number(entry, &["pageSize"])?;
    let (current, _) = number(entry, &["currentPage"])?;
    let left = total.checked_sub(current.checked_mul(per)?)?;
    (left > 0).then_some(left)
}

/// Chars left after the shown window: the entry's own `remainingChars`
/// (one window over several bodies), else `totalChars - nextOffset`.
fn chars_left(entry: &Value) -> Option<u64> {
    if let Some((left, _)) = number(entry, &["remainingChars"]) {
        return (left > 0).then_some(left);
    }
    let (total, _) = number(entry, &["totalChars"])?;
    let (offset, _) = number(entry, &["nextOffset"])?;
    total.checked_sub(offset).filter(|left| *left > 0)
}

/// A pull request's unfinished patches: `continuePatch` reads every
/// unfinished file of the page (the cut one first), so it counts them all.
fn patches_left(entry: &Value) -> Option<String> {
    if !has_more(entry) {
        return None;
    }
    match number(entry, &["unfinishedFiles"]).filter(|(n, _)| *n > 0) {
        Some((files, _)) => Some(counted(files, "unfinished patch", "unfinished patches")),
        None => chars_left(entry).map(|left| format!("{left} more patch chars")),
    }
}

fn axis_left(axis: &str, entry: &Value) -> Option<String> {
    if !has_more(entry) {
        return None;
    }
    match axis {
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
                "files" => ("changed file", "changed files"),
                "comments" => ("comment", "comments"),
                "reviews" => ("review", "reviews"),
                _ => ("commit", "commits"),
            };
            items_left(entry).map(|left| more(left, singular, plural))
        }
    }
}

/// A row `pagination` with the facts a tool left only in its page call
/// (deduplicated): the shown page is the queried page minus one, and the
/// page size is the queried one, else the contract default the call omits.
fn with_page_facts(pagination: &Value, call: &Value) -> Value {
    let mut facts = pagination.clone();
    let (Some(object), Some(query)) = (
        facts.as_object_mut(),
        crate::tools::result::continuation_row(call),
    ) else {
        return facts;
    };
    if !object.contains_key("currentPage")
        && let Some(page) = query.get("page").and_then(Value::as_u64)
    {
        object.insert("currentPage".into(), Value::from(page.saturating_sub(1)));
    }
    if !object.contains_key("pageSize")
        && let Some(size) = query
            .get("pageSize")
            .and_then(Value::as_u64)
            .or_else(|| default_page_size(call, query))
    {
        object.insert("pageSize".into(), Value::from(size));
    }
    facts
}

/// What one item of a listing is, by its list key.
const NOUNS: [(&str, (&str, &str)); 9] = [
    ("files", ("file", "files")),
    ("entries", ("entry", "entries")),
    ("matches", ("match", "matches")),
    ("symbols", ("symbol", "symbols")),
    ("repositories", ("repository", "repositories")),
    ("pullRequests", ("pull request", "pull requests")),
    ("issues", ("issue", "issues")),
    ("commits", ("commit", "commits")),
    ("artifacts", ("artifact", "artifacts")),
];

/// The contract's default `pageSize` for the continued tool and operation.
fn default_page_size(call: &Value, query: &Value) -> Option<u64> {
    let tool = ToolId::from_name(call.get("tool")?.as_str()?)?;
    let operation = query.get("operation").and_then(Value::as_str);
    crate::contracts::query_schema_number(tool, operation, "pageSize", "default")
}

/// A row-level `pagination`: lines or chars left in a read (its total from
/// the pagination, else from the row), items left in a listing, or else whole
/// pages left.
fn row_left(pagination: &Value, data: &Map<String, Value>, call: &Value) -> Option<String> {
    if !has_more(pagination) {
        return None;
    }
    if let Some((offset, _)) = number(pagination, &["nextOffset"]) {
        let (key, singular, plural) = match pagination.get("unit").and_then(Value::as_str) {
            Some("bytes") => ("totalBytes", "byte", "bytes"),
            Some("chars") => ("totalChars", "char", "chars"),
            // A symbols view pages its outline, not the source (LF3).
            _ if data.get("contentView").and_then(Value::as_str) == Some("symbols") => {
                ("totalLines", "outline line", "outline lines")
            }
            _ => ("totalLines", "line", "lines"),
        };
        let total = pagination
            .get(key)
            .or_else(|| data.get(key))
            .and_then(Value::as_u64)?;
        let left = total.checked_sub(offset).filter(|left| *left > 0)?;
        return Some(more(left, singular, plural));
    }
    if let Some(left) = items_left(pagination) {
        // The continued operation, else the row's list, names what a page
        // holds.
        let operation = crate::tools::result::continuation_row(call)
            .and_then(|query| query.get("operation"))
            .and_then(Value::as_str);
        let (singular, plural) = NOUNS
            .into_iter()
            .find(|(key, _)| operation == Some(*key))
            .or_else(|| NOUNS.into_iter().find(|(key, _)| data.contains_key(*key)))
            .map_or(("item", "items"), |(_, nouns)| nouns);
        return Some(more(left, singular, plural));
    }
    let (pages, _) = number(pagination, &["totalPages"])?;
    let (current, _) = number(pagination, &["currentPage"])?;
    let left = pages.checked_sub(current).filter(|left| *left > 0)?;
    Some(more(left, "page", "pages"))
}

/// The one text of a stale snapshot, whichever tool's cursor went stale.
pub const STALE_SNAPSHOT_ERROR: &str =
    "The source or the query changed since the earlier pages; follow next.restart to start over.";

/// A stale-snapshot row: the shared text, no `isPartial` (nothing of the
/// shown result remains to read), and `next.restart` when the tool built
/// none: page 1 of the same query, without its snapshot and page cursors.
pub(super) fn restart_stale(tool: ToolId, query: &Value, data: &mut Value) {
    let Some(object) = data.as_object_mut() else {
        return;
    };
    object.insert("error".into(), json!(STALE_SNAPSHOT_ERROR));
    object.remove("isPartial");
    if object
        .get("next")
        .and_then(Value::as_object)
        .is_some_and(|next| next.keys().any(|name| crate::tools::id::is_restart(name)))
    {
        return;
    }
    let Some(row) = query.as_object() else {
        return;
    };
    let is_cursor = |key: &str| {
        matches!(key, "snapshot" | "page" | "offset")
            || ["Snapshot", "Page", "Offset"]
                .iter()
                .any(|suffix| key.ends_with(suffix))
    };
    let row: Map<String, Value> = row
        .iter()
        .filter(|(key, _)| !is_cursor(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let restart = crate::tools::result::Continuation::new(tool, Value::Object(row)).build();
    match object.entry("next").or_insert_with(|| json!({})) {
        Value::Object(next) => {
            next.insert("restart".into(), restart);
        }
        other => *other = json!({ "restart": restart }),
    }
}

/// One page's facts. Every tool builds its `pagination` block from these
/// (never a hand-written JSON block), so the key set and order are the same
/// on every tool and every page: `currentPage`, `totalPages` and
/// `totalItems` when the total is known, `pageSize` when pages are cut by
/// count, `hasMore`, and `outOfRange` on a page past the end. `totalItems`
/// counts the paged unit only (the rows the pages split), never another
/// count of the same result. Tool-specific facts (`countScope`, cursors the
/// stage moves to `next`) follow on the block the caller extends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageFacts {
    page: usize,
    page_size: Option<usize>,
    total_items: Option<usize>,
    total_pages: Option<usize>,
    has_more: bool,
    out_of_range: bool,
}

impl PageFacts {
    /// A count-cut page of a known total: the page count and `hasMore`
    /// follow from it.
    pub(crate) fn counted(page: usize, page_size: usize, total_items: usize) -> Self {
        let page = page.max(1);
        let size = page_size.max(1);
        Self {
            page,
            page_size: Some(size),
            total_items: Some(total_items),
            total_pages: Some(total_items.div_ceil(size).max(1)),
            has_more: page.saturating_mul(size) < total_items,
            out_of_range: false,
        }
    }

    /// A page whose total is unknown (a provider that does not count, or a
    /// walk that stops early): only the cursor facts.
    pub(crate) fn open(page: usize, page_size: Option<usize>, has_more: bool) -> Self {
        Self {
            page: page.max(1),
            page_size,
            total_items: None,
            total_pages: None,
            has_more,
            out_of_range: false,
        }
    }

    /// A page cut by size (bytes, a response budget), not by count: the
    /// pager states the page count and the total of the paged unit.
    pub(crate) fn sized(
        page: usize,
        total_pages: usize,
        total_items: usize,
        has_more: bool,
    ) -> Self {
        Self {
            page: page.max(1),
            page_size: None,
            total_items: Some(total_items),
            total_pages: Some(total_pages.max(1)),
            has_more,
            out_of_range: false,
        }
    }

    /// The same page with a known total of the paged unit.
    pub(crate) fn with_total(mut self, total_items: usize) -> Self {
        self.total_items = Some(total_items);
        if let Some(size) = self.page_size {
            self.total_pages = Some(total_items.div_ceil(size.max(1)).max(1));
        }
        self
    }

    /// The same page with a partial count of the paged unit (items loaded
    /// so far, a provider's capped total): no page count follows from it.
    pub(crate) fn with_items(mut self, total_items: usize) -> Self {
        self.total_items = Some(total_items);
        self
    }

    /// The same size-cut page with the caller's requested page size.
    pub(crate) fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = Some(page_size);
        self
    }

    /// The same page, flagged as requested past the end (`outOfRange`).
    pub(crate) fn out_of_range(mut self, past_end: bool) -> Self {
        self.out_of_range = past_end;
        self
    }

    /// The `pagination` block.
    pub(crate) fn to_value(self) -> Value {
        let mut block = Map::new();
        block.insert("currentPage".into(), json!(self.page));
        if let Some(pages) = self.total_pages {
            block.insert("totalPages".into(), json!(pages));
        }
        if let Some(size) = self.page_size {
            block.insert("pageSize".into(), json!(size));
        }
        if let Some(total) = self.total_items {
            block.insert("totalItems".into(), json!(total));
        }
        block.insert("hasMore".into(), json!(self.has_more));
        if self.out_of_range {
            block.insert("outOfRange".into(), json!(true));
        }
        Value::Object(block)
    }
}

/// One count-cut page of an in-memory list. A page past the end is empty,
/// terminal and flagged `outOfRange`, never clamped to the last page, which
/// would repeat rows the caller already has.
pub(crate) fn slice_page<T: Clone>(
    items: &[T],
    page: usize,
    page_size: usize,
) -> (Vec<T>, PageFacts) {
    let page = page.max(1);
    let size = page_size.max(1);
    let facts = PageFacts::counted(page, size, items.len());
    let past_end = facts.total_pages.is_some_and(|pages| page > pages);
    let start = (page - 1).saturating_mul(size);
    let rows = items.iter().skip(start).take(size).cloned().collect();
    (rows, facts.out_of_range(past_end))
}

/// One pagination block, one shape for every tool and page: where the page
/// stands, its size, what is known of the total and whether more exists.
/// The cursor (`nextPage`, `snapshot`, `resultId`) rides `next`; a false
/// cap flag asserts nothing, and a true one repeats `providerLimit`.
pub(super) fn slim_pagination(block: &mut Map<String, Value>, limited: bool) {
    for key in ["nextPage", "snapshot", "resultId"] {
        block.remove(key);
    }
    match block.get("totalItemsCapped").and_then(Value::as_bool) {
        Some(false) => {
            block.remove("totalItemsCapped");
        }
        Some(true) if limited => {
            block.remove("totalItemsCapped");
        }
        _ => {}
    }
}

pub(super) fn is_pagination_key(key: &str) -> bool {
    key == "pagination" || key.ends_with("Pagination")
}

/// A row that states `complete:true` lists everything in its scope: any
/// `next.*` it carries is a drill-down, not unread rest.
#[must_use]
pub fn is_complete(data: &Value) -> bool {
    data.get("complete") == Some(&Value::Bool(true))
}

pub fn is_partial(data: &Value) -> bool {
    tree_some(data, &|map| {
        map.get("isPartial") == Some(&Value::Bool(true))
            || map.get("hasMore") == Some(&Value::Bool(true))
            || bounded(map)
    })
}

/// Whether `value` offers, under any `next` (never inside a call), a page
/// that leaves more of this result to read ([`crate::tools::id::is_remaining`]).
#[must_use]
pub fn has_remaining_page(tool: ToolId, value: &Value) -> bool {
    has_call(value, &|name| crate::tools::id::is_remaining(tool, name))
}

/// Whether `value` holds, at any depth, an executable `{tool, query}` call
/// under a key `name_matches` accepts.
fn has_call(value: &Value, name_matches: &impl Fn(&str) -> bool) -> bool {
    match value {
        Value::Object(map) => {
            if super::rows::has_executable_call(value) {
                return false;
            }
            map.iter().any(|(key, child)| {
                (name_matches(key) && super::rows::has_executable_call(child))
                    || has_call(child, name_matches)
            })
        }
        Value::Array(array) => array.iter().any(|v| has_call(v, name_matches)),
        _ => false,
    }
}

/// Codes of a partial row: a terminal limit, or a continuation missing.
pub(super) fn pagination_codes(data: &Value) -> Vec<String> {
    if tree_some(data, &|m| {
        m.get("terminalLimit") == Some(&Value::Bool(true))
    }) {
        return vec!["terminalLimitReached".into()];
    }
    let pageable = tree_some(data, &|m| m.get("hasMore") == Some(&Value::Bool(true)));
    let partial = tree_some(data, &|m| m.get("isPartial") == Some(&Value::Bool(true)));
    let page = has_call(data, &crate::tools::id::resumes_after_shown);
    let expansion = has_call(data, &|name| !crate::tools::id::resumes_after_shown(name));
    if (pageable && !page)
        || ((tree_some(data, &bounded) || partial) && !page && !expansion)
        || clipped_value_unreached(data)
    {
        vec!["continuationMissing".into()]
    } else {
        vec![]
    }
}

/// Whether a value clipped inside a file row (`truncated:true` under a row
/// with a `path`) has no continuation reaching it: neither a read or
/// expansion of that file nor one that widens values (`matchContentLength`).
/// Page continuations (`next*`, `continue*`) list more rows and never widen a
/// shown one.
fn clipped_value_unreached(data: &Value) -> bool {
    fn clipped_paths<'a>(value: &'a Value, path: Option<&'a str>, out: &mut Vec<&'a str>) {
        match value {
            Value::Object(map) => {
                let path = map.get("path").and_then(Value::as_str).or(path);
                if map.get("truncated") == Some(&Value::Bool(true))
                    && let Some(path) = path
                {
                    out.push(path);
                }
                for (key, child) in map {
                    if key != "next" {
                        clipped_paths(child, path, out);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| clipped_paths(item, path, out)),
            _ => {}
        }
    }
    fn reaching<'a>(value: &'a Value, paths: &mut Vec<&'a str>, widened: &mut bool) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if let Some(query) =
                        crate::tools::result::continuation_row(child).and_then(Value::as_object)
                        && child.get("tool").is_some_and(Value::is_string)
                        && !crate::tools::id::resumes_after_shown(key)
                    {
                        *widened |= query.contains_key("matchContentLength");
                        if let Some(path) = query.get("path").and_then(Value::as_str) {
                            paths.push(path);
                        }
                    }
                    reaching(child, paths, widened);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| reaching(item, paths, widened)),
            _ => {}
        }
    }
    let mut clipped = Vec::new();
    clipped_paths(data, None, &mut clipped);
    if clipped.is_empty() {
        return false;
    }
    let (mut targets, mut widened) = (Vec::new(), false);
    reaching(data, &mut targets, &mut widened);
    // Rows name files absolutely here; a continuation keeps the caller's
    // (workspace-relative) spelling of the same file.
    let same_file = |row: &str, target: &str| {
        let target = target.trim_start_matches("./");
        row == target
            || row
                .strip_suffix(target)
                .is_some_and(|prefix| prefix.ends_with('/'))
    };
    !widened
        && clipped
            .iter()
            .any(|path| !targets.iter().any(|target| same_file(path, target)))
}

fn tree_some(value: &Value, predicate: &impl Fn(&Map<String, Value>) -> bool) -> bool {
    match value {
        Value::Object(map) => predicate(map) || map.values().any(|v| tree_some(v, predicate)),
        Value::Array(array) => array.iter().any(|v| tree_some(v, predicate)),
        _ => false,
    }
}

fn bounded(record: &Map<String, Value>) -> bool {
    [
        "truncated",
        "capReached",
        "capped",
        "capturesTruncated",
        "totalItemsCapped",
        "incompleteTree",
        "possiblyTruncated",
        "truncatedByDepth",
        "truncatedByBudget",
    ]
    .iter()
    .any(|key| record.get(*key) == Some(&Value::Bool(true)))
        || record
            .get("partialTreeFailures")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{PageFacts, disclose_remaining_pages};
    use crate::tools::id::ToolId;
    use serde_json::{Value, json};

    fn page(tool: &str) -> Value {
        json!({"tool": tool, "query": {"queries":[{"page": 2}]}})
    }

    fn disclosed(data: Value) -> Value {
        let mut structured = json!({"results": [{"index": 0, "data": data}]});
        disclose_remaining_pages(&mut structured, ToolId::LocalSearch);
        let once = structured.clone();
        disclose_remaining_pages(&mut structured, ToolId::LocalSearch);
        assert_eq!(structured, once, "idempotent");
        structured["results"][0]["data"].take()
    }

    #[test]
    fn a_pull_request_page_names_every_remaining_count_first() {
        let data = disclosed(json!({
            "pullRequests": [{"number": 1, "contentPagination": {
                "patches": {"hasMore": true, "unfinishedFiles": 12},
                "files": {"currentPage": 1, "pageSize": 30, "totalItems": 37,
                    "hasMore": true, "nextPage": 2}
            }}],
            "isPartial": true,
            "next": {
                "continuePatch": page("ghGetHistoryItem"),
                "nextFilePage": page("ghGetHistoryItem")
            }
        }));
        assert_eq!(
            data["warnings"],
            json!([
                "12 unfinished patches: follow next.continuePatch; 7 more changed files: follow next.nextFilePage"
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
    fn one_patch_page_counts_every_unfinished_file() {
        let warnings = disclosed(json!({
            "pullRequests": [{"number": 1, "contentPagination": {
                "patches": {"hasMore": true, "unfinishedFiles": 1}}}],
            "next": {"continuePatch": page("ghGetHistoryItem")}
        }))["warnings"]
            .clone();
        assert_eq!(
            warnings,
            json!(["1 unfinished patch: follow next.continuePatch"])
        );
    }

    #[test]
    fn listings_reads_and_bodies_count_what_is_left() {
        let listing = disclosed(json!({
            "files": [{"path": "a"}],
            "pagination": {"currentPage": 1, "totalPages": 2, "totalItems": 2, "hasMore": true},
            "warnings": ["Regex trap."],
            "next": {"nextPage": page("localSearch")}
        }));
        assert_eq!(
            listing["warnings"],
            json!(["Regex trap.", "1 more page: follow next.nextPage"]),
            "{listing}"
        );
        let files = disclosed(json!({
            "files": [{"path": "a"}],
            "pagination": {"currentPage": 1, "totalPages": 7, "pageSize": 3,
                "totalItems": 20, "hasMore": true},
            "next": {"nextPage": page("localSearch")}
        }));
        assert_eq!(
            files["warnings"],
            json!(["17 more files: follow next.nextPage"])
        );
        let deduplicated = disclosed(json!({
            "commits": [{"sha": "a"}],
            "pagination": {"totalItems": 99, "totalPages": 20, "hasMore": true},
            "next": {"nextPage": {"tool": "ghSearchHistory",
                "query": {"queries":[{"keywords": ["miri"], "page": 2, "pageSize": 5}]}}}
        }));
        assert_eq!(
            deduplicated["warnings"],
            json!(["94 more commits: follow next.nextPage"])
        );
        assert!(deduplicated["pagination"].get("currentPage").is_none());
        let search = disclosed(json!({
            "pullRequests": [{"number": 1}],
            "pagination": {"currentPage": 1, "pageSize": 10, "totalItems": 25, "hasMore": true},
            "next": {"nextPage": page("ghSearchHistory")}
        }));
        assert_eq!(
            search["warnings"],
            json!(["15 more pull requests: follow next.nextPage"])
        );
        let read = disclosed(json!({
            "content": "1\tx",
            "pagination": {"unit": "lines", "offset": 0, "length": 381,
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
            "pagination": {"unit": "lines", "offset": 0, "length": 471,
                "hasMore": true, "nextOffset": 471},
            "isPartial": true
        }));
        assert_eq!(
            whole["warnings"],
            json!(["2666 more lines: follow next.continue"])
        );
        // LF3: a symbols view pages outline lines, not source lines.
        let outline = disclosed(json!({
            "path": "a.rs",
            "contentView": "symbols",
            "content": "1\tfn a()",
            "pagination": {"unit": "lines", "offset": 0, "length": 40,
                "totalLines": 52, "hasMore": true, "nextOffset": 40},
            "next": {"continue": page("localFetch")}
        }));
        assert_eq!(
            outline["warnings"],
            json!(["12 more outline lines: follow next.continue"])
        );
        let bytes = disclosed(json!({
            "content": "x",
            "pagination": {"unit": "bytes", "offset": 0, "length": 100,
                "totalBytes": 250, "hasMore": true, "nextOffset": 100},
            "next": {"continue": page("localFetch")}
        }));
        assert_eq!(
            bytes["warnings"],
            json!(["150 more bytes: follow next.continue"])
        );
        let issue = disclosed(json!({
            "issues": [{"number": 3, "contentPagination": {"body": {"offset": 0,
                "length": 12000, "totalChars": 14366, "hasMore": true,
                "nextOffset": 12000}}}],
            "next": {"continueBody": page("ghGetHistoryItem")}
        }));
        assert_eq!(
            issue["warnings"],
            json!(["2366 more body chars: follow next.continueBody"])
        );
        // QA2: one offset continues every cut comment of the page; the
        // count is all of their remaining chars.
        let comments = disclosed(json!({
            "pullRequests": [{"number": 3, "contentPagination": {"commentBody": {"offset": 0,
                "length": 12000, "totalChars": 26574, "hasMore": true,
                "nextOffset": 12000, "remainingChars": 101000}}}],
            "next": {"continueCommentBody": page("ghGetHistoryItem")}
        }));
        assert_eq!(
            comments["warnings"],
            json!(["101000 more comment body chars: follow next.continueCommentBody"])
        );
    }

    #[test]
    fn pages_without_a_count_or_already_named_add_nothing() {
        let named = json!({
            "warnings": ["Snapshot changed. next.restart reruns the search."],
            "pagination": {"currentPage": 1, "totalPages": 1, "hasMore": false},
            "next": {"restart": page("localSearch")}
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

    /// A page whose count is unknown still gets its line, without a number;
    /// a restart page starts over and gets none.
    #[test]
    fn every_remaining_page_gets_a_line_even_without_a_count() {
        let commits = disclosed(json!({
            "commits": [{"sha": "a"}],
            "pagination": {"hasMore": true},
            "next": {"nextPage": page("ghSearchHistory"),
                "continueMaterialize": page("ghStructure")}
        }));
        assert_eq!(
            commits["warnings"],
            json!(["more: follow next.nextPage; more: follow next.continueMaterialize"])
        );
        let bytes = disclosed(json!({
            "content": "x",
            "pagination": {"unit": "bytes", "offset": 0, "length": 100,
                "hasMore": true, "nextOffset": 100},
            "next": {"continue": page("localFetch")}
        }));
        assert_eq!(bytes["warnings"], json!(["more: follow next.continue"]));
        let restart = json!({"next": {"restart": page("localSearch")}});
        assert_eq!(disclosed(restart.clone()), restart);
    }

    /// A page size the continuation leaves at its contract default still
    /// counts: the default is the page size.
    #[test]
    fn a_default_page_size_comes_from_the_contract() {
        let repos = disclosed(json!({
            "repositories": [{"repo": "a"}],
            "pagination": {"totalItems": 38, "hasMore": true},
            "next": {"nextPage": {"tool": "ghSearchRepo",
                "query": {"queries": [{"keywords": ["mcp"], "page": 2}]}}}
        }));
        assert_eq!(
            repos["warnings"],
            json!(["18 more repositories: follow next.nextPage"])
        );
        // A directory outline groups its symbols by file: the continued
        // operation names what a page holds.
        let symbols = disclosed(json!({
            "files": [{"path": "a.ts", "symbols": ["fn a"]}],
            "pagination": {"currentPage": 1, "totalPages": 15, "totalItems": 7194, "hasMore": true},
            "next": {"nextPage": {"tool": "astSearch",
                "query": {"queries": [{"operation": "symbols", "path": "src", "page": 2}]}}}
        }));
        assert_eq!(
            symbols["warnings"],
            json!(["6694 more symbols: follow next.nextPage"])
        );
    }

    /// `nextMatchPage` continues the matches of the page's files, counted by
    /// each file's own pagination, never the row's file pagination.
    #[test]
    fn match_pages_count_the_matches_left_in_the_page_files() {
        let data = disclosed(json!({
            "files": [
                {"path": "a", "matches": ["x"], "pagination": {"currentPage": 1,
                    "pageSize": 20, "totalItems": 45, "hasMore": true}},
                {"path": "b", "matches": ["y"]},
                {"path": "c", "matches": ["z"], "pagination": {"currentPage": 1,
                    "pageSize": 20, "totalItems": 21, "hasMore": true}}
            ],
            "pagination": {"currentPage": 1, "pageSize": 5, "totalItems": 79, "hasMore": true},
            "next": {"nextPage": {"tool": "astSearch",
                    "query": {"queries": [{"operation": "match", "page": 2, "pageSize": 5}]}},
                "nextMatchPage": {"tool": "astSearch",
                    "query": {"queries": [{"operation": "match", "matchPage": 2, "pageSize": 5}]}}}
        }));
        assert_eq!(
            data["warnings"],
            json!([
                "74 more files: follow next.nextPage; 26 more matches: follow next.nextMatchPage"
            ])
        );
    }

    /// Pages moved to a later response part read as one response page: the
    /// retargeted line names it once.
    #[test]
    fn retargeted_lines_name_the_response_page_once() {
        let mut data = json!({
            "warnings": ["more: follow next.nextPage; more: follow next.nextFilePage; 3 unfinished patches: follow next.continuePatch"]
        });
        let moved = ["nextPage", "nextFilePage", "continuePatch"].map(String::from);
        super::retarget_moved_pages(data.as_object_mut().expect("object"), &moved);
        assert_eq!(
            data["warnings"],
            json!([
                "more: follow responsePagination.next; 3 unfinished patches: follow responsePagination.next"
            ])
        );
    }

    /// A tool warning naming `next.expandValues` covers its numbered
    /// siblings `expandValues2`, `expandValues3`.
    #[test]
    fn a_named_page_covers_its_numbered_siblings() {
        let named = json!({
            "warnings": ["Some match values were truncated. next.expandValues reads them whole."],
            "next": {"expandValues": page("localSearch"), "expandValues2": page("localSearch"),
                "expandValues3": page("localSearch")}
        });
        assert_eq!(disclosed(named.clone()), named);
    }

    /// `partialReasons` name why a row is partial (a provider cap, a size
    /// limit, content paging), not how much a page has left: the count still
    /// leads the row.
    #[test]
    fn partial_reasons_do_not_hide_the_remaining_count() {
        let repos = disclosed(json!({
            "repositories": [{"repo": "a"}],
            "pagination": {"totalItems": 1000, "hasMore": true},
            "next": {"nextPage": {"tool": "ghSearchRepo",
                "query": {"queries":[{"keywords": ["mcp"], "page": 2, "pageSize": 3}]}}},
            "providerLimit": {"maxResults": 1000},
            "isPartial": true,
            "partialReasons": ["providerResultCap"]
        }));
        assert_eq!(
            repos["warnings"],
            json!(["997 more repositories: follow next.nextPage"])
        );
        let read = disclosed(json!({
            "path": "a.rs",
            "totalLines": 2270,
            "pagination": {"unit": "lines", "offset": 0, "length": 490,
                "hasMore": true, "nextOffset": 490},
            "isPartial": true,
            "partialReasons": ["full-content-size-limit"],
            "next": {"continue": page("ghGetFileContent")}
        }));
        assert_eq!(
            read["warnings"],
            json!(["1780 more lines: follow next.continue"])
        );
        let patches = disclosed(json!({
            "pullRequests": [{"number": 1, "contentPagination": {
                "patches": {"hasMore": true, "unfinishedFiles": 3}}}],
            "isPartial": true,
            "partialReasons": ["contentPagination"],
            "next": {"continuePatch": page("ghGetHistoryItem")}
        }));
        assert_eq!(
            patches["warnings"],
            json!(["3 unfinished patches: follow next.continuePatch"])
        );
    }

    /// LS6(b): a stale snapshot names both causes: the source or the query.
    #[test]
    fn stale_snapshot_names_source_or_query() {
        assert!(
            crate::response::pages::STALE_SNAPSHOT_ERROR.contains("source or the query changed")
        );
        assert!(crate::response::pages::STALE_SNAPSHOT_ERROR.contains("next.restart"));
    }

    #[test]
    fn page_facts_count_only_the_paged_unit() {
        // One key order on every page; the total is the paged unit's.
        assert_eq!(
            PageFacts::counted(2, 10, 25).to_value().to_string(),
            r#"{"currentPage":2,"totalPages":3,"pageSize":10,"totalItems":25,"hasMore":true}"#
        );
        assert_eq!(
            PageFacts::counted(3, 10, 25).to_value()["hasMore"],
            json!(false)
        );
        // A page past the end is a page, not more.
        assert_eq!(
            PageFacts::counted(4, 10, 25).to_value()["hasMore"],
            json!(false)
        );
        // Unknown totals state only the cursor facts, in the same order.
        assert_eq!(
            PageFacts::open(1, Some(30), true).to_value().to_string(),
            r#"{"currentPage":1,"pageSize":30,"hasMore":true}"#
        );
        assert_eq!(
            PageFacts::open(1, Some(30), true).with_total(31).to_value(),
            json!({"currentPage":1,"totalPages":2,"pageSize":30,"totalItems":31,"hasMore":true})
        );
        // Size-cut pages have no page size.
        assert_eq!(
            PageFacts::sized(1, 4, 90, true).to_value().to_string(),
            r#"{"currentPage":1,"totalPages":4,"totalItems":90,"hasMore":true}"#
        );
    }
}
