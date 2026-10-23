use super::{WARNINGS, is_complete};
use crate::id::ToolId;
use crate::id::continuation_channels::PAGES_KEY;
use crate::response::result::Remaining;
use serde_json::{Map, Value};

/// The row key that carries the engine's [`Remaining`] notes until the
/// stage renders them; never public.
pub(super) const REMAINING: &str = "remaining";

/// Notes on `row` every page of its data that `tool` leaves remaining with
/// a counted rest: the tool's `typed` note of a page (a count only the tool
/// knows), else the count the page's public pagination facts state. Runs
/// before extras pruning, while the pagination facts are still on the row.
pub(crate) fn note_remaining(row: &mut Value, tool: ToolId, typed: Vec<Remaining>) {
    let notes = remaining_notes(row, tool, typed);
    if !notes.is_empty()
        && let Some(object) = row.as_object_mut()
    {
        object.insert(
            REMAINING.to_owned(),
            serde_json::to_value(notes).unwrap_or_default(),
        );
    }
}

fn remaining_notes(row: &Value, tool: ToolId, typed: Vec<Remaining>) -> Vec<Remaining> {
    match row.get("data").and_then(Value::as_object) {
        Some(data) if !is_complete(&row["data"]) => {
            remaining_pages(data, typed, |name| crate::id::is_remaining(tool, name))
        }
        _ => Vec::new(),
    }
}

/// Every page of `data` that `remains` and counts its rest, in `next`
/// order: a `typed` note stands for its page; a typed note of a page the
/// row does not offer is dropped.
fn remaining_pages(
    data: &Map<String, Value>,
    mut typed: Vec<Remaining>,
    remains: impl Fn(&str) -> bool,
) -> Vec<Remaining> {
    let Some(Value::Object(pages)) = data.get(PAGES_KEY) else {
        return Vec::new();
    };
    pages
        .iter()
        .filter(|(name, _)| remains(name))
        .filter_map(|(name, call)| {
            if let Some(at) = typed.iter().position(|note| note.page == *name) {
                return Some(typed.swap_remove(at));
            }
            match remaining(data, name, call) {
                Left::Counted(left) => Some(Remaining {
                    page: name.clone(),
                    left,
                }),
                Left::Done | Left::Unknown => None,
            }
        })
        .collect()
}

/// Takes every result row's notes off the rows, returned by row position
/// for the pager. A whole row renders nothing: its visible `pagination` and
/// `next` already carry what is left. Only a split row's moved pages are
/// worded ([`redisclose`]).
pub(crate) fn take_remaining_notes(structured: &mut Value) -> Vec<Vec<Remaining>> {
    structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .map(|row| {
            row.as_object_mut()
                .and_then(|object| object.remove(REMAINING))
                .and_then(|notes| serde_json::from_value(notes).ok())
                .unwrap_or_default()
        })
        .collect()
}

/// The one `warnings` entry for the pages a split moved to the response
/// pager: `17 more files (next.nextPage on the row's last part)`. A moved continuation is
/// the one thing the part's own `pagination` and `next` cannot say. Lines
/// that read the same appear once.
fn render_moved(notes: &[Remaining], moved: &[String]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for note in notes.iter().filter(|note| moved.contains(&note.page)) {
        let part = format!(
            "{} ({PAGES_KEY}.{} on the row's last part)",
            note.left, note.page
        );
        if !parts.contains(&part) {
            parts.push(part);
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// Appends `entry` to the row's warnings and moves them to the front.
fn lead_with(data: &mut Map<String, Value>, entry: String) {
    let warnings = match data.shift_remove(WARNINGS) {
        Some(Value::Array(mut warnings)) => {
            warnings.push(Value::String(entry));
            warnings
        }
        _ => vec![Value::String(entry)],
    };
    data.shift_insert(0, WARNINGS.to_owned(), Value::Array(warnings));
}

/// One part of a split row: names the response page for every page the
/// split `moved` to another part, so a part never points at a continuation
/// it does not carry.
pub(crate) fn redisclose(data: &mut Map<String, Value>, notes: &[Remaining], moved: &[String]) {
    if let Some(entry) = render_moved(notes, moved) {
        lead_with(data, entry);
    }
}

/// What a page has left to show.
pub(super) enum Left {
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
/// public pagination facts.
pub(super) fn remaining(data: &Map<String, Value>, name: &str, call: &Value) -> Left {
    match name {
        "nextMatchPage" => matches_left(data).map_or(Left::Unknown, |left| {
            Left::Counted(more(left, "match", "matches"))
        }),
        "nextPage" | "continueWindow" => {
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

/// The first `limit` of `names` joined with `, `, then the count of the
/// rest: `a, b, +3 more`.
pub(crate) fn first_named(names: impl IntoIterator<Item = String>, limit: usize) -> String {
    let mut names = names.into_iter();
    let mut shown = names.by_ref().take(limit).collect::<Vec<_>>();
    let rest = names.count();
    if rest > 0 {
        shown.push(format!("+{rest} more"));
    }
    shown.join(", ")
}

pub(crate) fn more(count: u64, singular: &str, plural: &str) -> String {
    format!(
        "{count} more {}",
        if count == 1 { singular } else { plural }
    )
}

/// Items left after the current page of a count-cut `entry`:
/// `totalItems - currentPage * pageSize`.
pub(crate) fn items_left(entry: &Value) -> Option<u64> {
    let (total, _) = number(entry, &["totalItems"])?;
    let (per, _) = number(entry, &["pageSize"])?;
    let (current, _) = number(entry, &["currentPage"])?;
    let left = total.checked_sub(current.checked_mul(per)?)?;
    (left > 0).then_some(left)
}

/// A row `pagination` with the facts a tool left only in its page call
/// (deduplicated): the shown page is the queried page minus one, and the
/// page size is the queried one, else the contract default the call omits.
fn with_page_facts(pagination: &Value, call: &Value) -> Value {
    let mut facts = pagination.clone();
    let (Some(object), Some(query)) = (
        facts.as_object_mut(),
        crate::response::result::continuation_row(call),
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
const NOUNS: [(&str, (&str, &str)); 8] = [
    ("files", ("file", "files")),
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
        // A window states its view's `total` in its unit; a line window omits
        // it when the view is the whole file, which `data.totalLines` sizes.
        let (fallback, singular, plural) = match pagination.get("unit").and_then(Value::as_str) {
            Some("bytes") => (None, "byte", "bytes"),
            Some("chars") => (None, "char", "chars"),
            // A symbols view pages its outline, not the source.
            _ if data.get("contentView").and_then(Value::as_str) == Some("symbols") => {
                (Some("totalLines"), "outline line", "outline lines")
            }
            _ => (Some("totalLines"), "line", "lines"),
        };
        let total = pagination
            .get("total")
            .or_else(|| data.get(fallback?))
            .and_then(Value::as_u64)?;
        let left = total.checked_sub(offset).filter(|left| *left > 0)?;
        return Some(more(left, singular, plural));
    }
    if let Some(left) = items_left(pagination) {
        // The continued operation, else the row's list, names what a page
        // holds.
        let operation = crate::response::result::continuation_row(call)
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
