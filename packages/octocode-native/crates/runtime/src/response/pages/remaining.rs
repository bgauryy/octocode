use super::WARNINGS;
use crate::id::ToolId;
use crate::response::result::PAGES_KEY;
use crate::response::result::Remaining;
use serde_json::{Map, Value};

/// The row key that carries the engine's [`Remaining`] notes until the
/// stage renders them; never public.
pub(super) const REMAINING: &str = "remaining";

/// Retain producer-supplied counts only for required evidence calls the row actually offers.
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

fn remaining_notes(row: &Value, _tool: ToolId, typed: Vec<Remaining>) -> Vec<Remaining> {
    let Some(pages) = row.pointer("/data/next").and_then(Value::as_object) else {
        return Vec::new();
    };
    typed
        .into_iter()
        .filter(|note| {
            pages
                .get(&note.page)
                .is_some_and(|call| call["kind"] == "continue")
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
/// `totalItems - page * pageSize`.
pub(crate) fn items_left(entry: &Value) -> Option<u64> {
    let total = entry.get("totalItems")?.as_u64()?;
    let per = entry.get("pageSize")?.as_u64()?;
    // An `unshown` section has shown none of its items.
    let current = if entry["unshown"] == true {
        0
    } else {
        entry.get("page")?.as_u64()?
    };
    let left = total.checked_sub(current.checked_mul(per)?)?;
    (left > 0).then_some(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn remaining_counts_require_an_explicit_required_call() {
        let note = |page: &str| Remaining {
            page: page.into(),
            left: "3 more files".into(),
        };
        let mut row = json!({"index":0,"data":{"pagination":{"page":1,"pageSize":2,"totalItems":20},"next":{"page":{"kind":"continue"},"explore":{"kind":"related"}}}});
        note_remaining(
            &mut row,
            ToolId::LocalSearch,
            vec![note("page"), note("missing"), note("explore")],
        );
        let mut output = json!({"results":[row]});
        let notes = take_remaining_notes(&mut output);
        assert_eq!(notes, vec![vec![note("page")]]);
        assert!(output["results"][0].get(REMAINING).is_none());
        let mut data = Map::new();
        redisclose(&mut data, &notes[0], &["page".into()]);
        assert!(
            data["warnings"][0]
                .as_str()
                .unwrap()
                .contains("3 more files")
        );
        assert!(data["warnings"][0].as_str().unwrap().contains("next.page"));
    }

    #[test]
    fn item_counts_do_not_guess_missing_totals_or_overflow() {
        assert_eq!(
            items_left(&json!({"page":2,"pageSize":10,"totalItems":25})),
            Some(5)
        );
        assert_eq!(
            items_left(&json!({"unshown":true,"pageSize":10,"totalItems":25})),
            Some(25)
        );
        assert_eq!(
            items_left(&json!({"page":3,"pageSize":10,"totalItems":25})),
            None
        );
        assert_eq!(items_left(&json!({"page":1,"pageSize":10})), None);
        assert_eq!(
            items_left(&json!({"page":u64::MAX,"pageSize":2,"totalItems":u64::MAX})),
            None
        );
    }
}
