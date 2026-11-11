//! What astSearch shows of its sources, cut from each source judged whole.
//! A parser reads source unredacted, and a node cut from it (a string
//! literal, a capture, a name) holds nothing of the key that names it a
//! secret or of the syntax that makes it code. Every text of a row is
//! therefore the cut of its position from the judged source
//! ([`crate::security::JudgedText`]): the redaction a read of those lines
//! shows, no more and no less. A position is the one the row was built
//! with, never a search for the row's text: the same letters occur in the
//! values beside a name.
use crate::security::JudgedText;
use crate::tools::judged_rows::{RowSources, number};
use serde_json::Value;

/// The key under which a declaration row whose response states no `column`
/// carries, from where it is built (`symbols::rows`) to here, the 1-based
/// column its name starts at on its `line`, in the engine's unit. Judging a
/// page takes it out of every row ([`judge_symbol`]): no response states it.
pub(super) const NAME_AT: &str = "nameAt";

/// The marker a lean row puts after the header of a multi-line match.
const CLIP: &str = " …";

/// Judge every text of `data`'s rows against the source its row names.
pub(super) fn judge_rows(data: &mut Value, sources: &RowSources<'_>) {
    visit(data, sources);
    sources.warn(data);
}

fn visit(value: &mut Value, sources: &RowSources<'_>) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| visit(item, sources)),
        Value::Object(fields) => {
            let source = fields
                .get("path")
                .and_then(Value::as_str)
                .filter(|_| fields.contains_key("matches") || fields.contains_key("symbols"))
                .and_then(|shown| sources.source(shown));
            if let Some(source) = &source {
                for row in rows(fields.get_mut("matches")) {
                    judge_match(row, source, sources);
                }
            }
            // A declaration row gives up its name's place whether or not
            // its source can be judged.
            for row in rows(fields.get_mut("symbols")) {
                judge_symbol(row, source.as_deref(), sources);
            }
            for (key, field) in fields.iter_mut() {
                if key != "symbols" && (source.is_none() || key != "matches") {
                    visit(field, sources);
                }
            }
        }
        _ => {}
    }
}

fn rows(field: Option<&mut Value>) -> impl Iterator<Item = &mut Value> {
    field.and_then(Value::as_array_mut).into_iter().flatten()
}

/// A row's 1-based `line` and 0-based column (from the 1-based `column`).
fn position(row: &Value, column: &str) -> Option<(usize, Option<usize>)> {
    let line = number(&row["line"])?;
    Some((line, number(&row[column]).and_then(|c| c.checked_sub(1))))
}

/// One match row: its `value` (a header before [`CLIP`] in a lean row), each
/// ranged capture's `text` at its own position, and each unranged capture
/// from within the match's lines.
fn judge_match(row: &mut Value, source: &JudgedText, sources: &RowSources<'_>) {
    let Some(at) = position(row, "column") else {
        return;
    };
    let suffix = if row["clipped"] == true { CLIP } else { "" };
    sources.judge_leaf(&mut row["value"], source, at, suffix);
    let last = number(&row["endLine"]).unwrap_or(at.0);
    let within = source.line_span(at.0, last.saturating_sub(at.0) + 1);
    if let Some(ranges) = row.get_mut("metavarRanges").and_then(Value::as_object_mut) {
        for capture in ranges
            .values_mut()
            .flat_map(|rows| rows.as_array_mut())
            .flatten()
        {
            if let Some(at) = position(capture, "column")
                && capture.get("text").is_some()
            {
                sources.judge_leaf(&mut capture["text"], source, at, "");
            }
        }
    }
    let Some(captures) = row.get_mut("metavars").and_then(Value::as_object_mut) else {
        return;
    };
    for capture in captures.values_mut() {
        let leaves: Vec<&mut Value> = match capture {
            Value::Array(items) => items.iter_mut().collect(),
            leaf => vec![leaf],
        };
        for leaf in leaves {
            let Some(text) = leaf.as_str() else { continue };
            let shown = within
                .as_ref()
                .and_then(|within| source.fragment_within(within, text))
                .map(std::borrow::Cow::into_owned);
            match shown {
                Some(shown) => {
                    let changed = shown != text;
                    sources.put(leaf, source, shown, changed);
                }
                // Not the source's text within the match: shown only when
                // the pass replaced nothing there.
                None if within.as_ref().is_none_or(|within| source.exposes(within)) => {
                    sources.put(
                        leaf,
                        source,
                        crate::security::cut::REDACTED.to_owned(),
                        true,
                    );
                }
                None => {}
            }
        }
    }
}

/// One declaration row and its members: its name at the place the row was
/// built with (its `column`, else [`NAME_AT`], on its `line`), and the row
/// gives [`NAME_AT`] up here. The name a trait impl's row states for its
/// type is the one at that place; its display name, the name of a parent
/// declared by another row and the names it is exported under have no place
/// of their own ([`RowSources::judge_name`]).
fn judge_symbol(row: &mut Value, source: Option<&JudgedText>, sources: &RowSources<'_>) {
    let unstated = row
        .as_object_mut()
        .and_then(|fields| fields.remove(NAME_AT));
    if let Some(source) = source {
        let column = unstated
            .as_ref()
            .or_else(|| row.get("column"))
            .and_then(number)
            .and_then(|column| column.checked_sub(1));
        let at = number(&row["line"]).zip(column);
        for name in ["symbolName", "typeName"] {
            if row.get(name).is_some_and(Value::is_string) {
                sources.judge_name(&mut row[name], source, at);
            }
        }
        if row.get("parent").is_some_and(Value::is_string) {
            sources.judge_name(&mut row["parent"], source, None);
        }
        for public in rows(row.get_mut("exportedAs")) {
            sources.judge_name(public, source, None);
        }
    }
    for member in rows(row.get_mut("members")) {
        judge_symbol(member, source, sources);
    }
}

#[cfg(test)]
mod tests;
