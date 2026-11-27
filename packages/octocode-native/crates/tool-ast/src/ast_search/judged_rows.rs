//! What astSearch shows of its sources, cut from each source judged whole.
//! A parser reads the text a read shows of a source (its placeholders
//! written over in place, [`super::source::admitted_for_match`]), so every
//! position counts that text, as localFetch and localSearch do. A node cut
//! from it alone (a string literal, a capture, a name) holds nothing of the
//! key that names it a secret or of the syntax that makes it code: every
//! text of a row is therefore the cut of its position from the judged
//! source as shown ([`octocode_runtime_base::security::JudgedText::as_shown`]), the redaction
//! a read of those lines shows, no more and no less. A position is the one
//! the row was built with, never a search for the row's text: the same
//! letters occur in the values beside a name.
use octocode_runtime_base::security::JudgedText;
use octocode_tools_shared::judged_rows::{RowSources, number};
use serde_json::Value;

/// Where each declaration row of a symbols page was named: its 1-based
/// `line` and the 1-based column its name starts at there, in the engine's
/// unit, in the order the page lists its rows (a row, then its members).
/// A row states that column only when it tells two rows apart, so the page
/// hands every row's here, beside its rows, to the judging of their names
/// ([`judge_symbol`]): no row carries it.
#[derive(Clone, Default)]
pub(crate) struct NamePlaces(std::collections::VecDeque<(usize, usize)>);

impl NamePlaces {
    /// The places of `rows`, each named at its `line` and at the column of
    /// `columns` beside it.
    pub(super) fn of(rows: &[Value], columns: &[u32]) -> Self {
        Self(
            rows.iter()
                .zip(columns)
                .filter_map(|(row, column)| Some((number(&row["line"])?, *column as usize)))
                .collect(),
        )
    }

    /// The 0-based column the next row was named at, when that row is the
    /// one on `line`. A row the page does not list next has no place here.
    fn next(&mut self, line: Option<usize>) -> Option<usize> {
        let &(placed, column) = self.0.front()?;
        if Some(placed) != line {
            return None;
        }
        self.0.pop_front();
        column.checked_sub(1)
    }
}

/// The marker a lean row puts after the header of a multi-line match.
const CLIP: &str = " …";

/// Judge every text of `data`'s rows against the source its row names;
/// `places` names where its declaration rows were named.
#[cfg(test)]
pub(super) fn judge_rows(data: &mut Value, sources: &RowSources<'_>, places: NamePlaces) {
    judge_rows_with(data, sources, places, &Known::default());
}

/// [`judge_rows`], each judged source taken from `known` when it holds the
/// row's file, else read and judged now and kept in `known`.
pub(super) fn judge_rows_with(
    data: &mut Value,
    sources: &RowSources<'_>,
    mut places: NamePlaces,
    known: &Known,
) {
    visit(data, &Judging { sources, known }, &mut places);
    sources.warn(data);
}

/// The judged sources one page's rows were cut from, by the path each row
/// shows: what another request handed the same page judges its rows by
/// ([`super::shared_rows`]), instead of reading and judging each file again.
/// A file the read policy refused, or that could not be read, is kept as
/// `None`, as [`RowSources::source`] answers it.
#[derive(Clone, Default)]
pub(super) struct Known(
    std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, Option<std::sync::Arc<JudgedText>>>>,
    >,
);

impl Known {
    /// `source` held as what the rows named `shown` are cut from: inline
    /// source, which no file holds ([`super::source::INLINE_NAME`]).
    pub(super) fn holding(shown: &str, source: JudgedText) -> Self {
        let known = Self::default();
        known
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(shown.to_owned(), Some(std::sync::Arc::new(source)));
        known
    }

    /// The judged source of the file a row shows as `shown`.
    fn source(&self, shown: &str, sources: &RowSources<'_>) -> Option<std::sync::Arc<JudgedText>> {
        let held = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(shown)
            .cloned();
        if let Some(held) = held {
            return held;
        }
        let judged = sources.source(shown);
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(shown.to_owned(), judged.clone());
        judged
    }
}

/// A page's judging: its request's sources, and the judged sources known
/// already.
struct Judging<'a, 'b> {
    sources: &'a RowSources<'b>,
    known: &'a Known,
}

/// Lean rows retain a start column when multiple matches start on their line.
/// Expanded rows keep their ranges. The header text already marks clipping.
pub(super) fn drop_judging_positions(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(drop_judging_positions),
        Value::Object(fields) => {
            for row in fields
                .get_mut("matches")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object_mut)
            {
                let shares_line = octocode_tools_shared::same_line::take_mark(row);
                if !row.contains_key("endColumn") {
                    if !shares_line {
                        row.remove("column");
                    }
                    row.remove("clipped");
                }
            }
            fields.values_mut().for_each(drop_judging_positions);
        }
        _ => {}
    }
}

fn visit(value: &mut Value, judging: &Judging<'_, '_>, places: &mut NamePlaces) {
    let sources = judging.sources;
    match value {
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| visit(item, judging, places)),
        Value::Object(fields) => {
            let source = fields
                .get("path")
                .and_then(Value::as_str)
                .filter(|_| fields.contains_key("matches") || fields.contains_key("symbols"))
                .and_then(|shown| judging.known.source(shown, sources));
            if let Some(source) = &source {
                for row in rows(fields.get_mut("matches")) {
                    judge_match(row, source, sources);
                }
            }
            // A declaration row takes its place whether or not its source
            // can be judged: the rows after it keep theirs.
            for row in rows(fields.get_mut("symbols")) {
                judge_symbol(row, source.as_deref(), sources, places);
            }
            for (key, field) in fields.iter_mut() {
                if key != "symbols" && (source.is_none() || key != "matches") {
                    visit(field, judging, places);
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
                        octocode_runtime_base::security::cut::REDACTED.to_owned(),
                        true,
                    );
                }
                None => {}
            }
        }
    }
}

/// One declaration row and its members: its name at the place the row was
/// built with ([`NamePlaces`], on its `line`). The name a trait impl's row states for its
/// type is the one at that place; its display name, the name of a parent
/// declared by another row and the names it is exported under have no place
/// of their own ([`RowSources::judge_name`]).
fn judge_symbol(
    row: &mut Value,
    source: Option<&JudgedText>,
    sources: &RowSources<'_>,
    places: &mut NamePlaces,
) {
    let line = number(&row["line"]);
    let column = places.next(line);
    if let Some(source) = source {
        let at = line.zip(column);
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
        judge_symbol(member, source, sources, places);
    }
}

#[cfg(test)]
mod tests;
