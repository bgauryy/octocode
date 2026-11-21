//! The one hoist stage: what a response repeats is stated once, as the
//! tool's output contract plans it ([`plan`]). It runs on the sanitized
//! envelope, before validation and paging (`stage::settle`), so it only
//! moves or drops values the output gate already sealed, the validator
//! checks the hoisted shape, and every saved page replays it. In order:
//! 1. omitted values (F3, [`omit`]): a field holding the value the contract
//!    derives (a constant, another field, a template over one);
//! 2. holders (F1, [`holder`]): a value every item of a holder's collections
//!    shares, stated on the holder, innermost first, out to `data`; then the
//!    declaration lists the plan names as outline strings ([`outline`], Q5),
//!    joined here from the sealed leaves and never by a tool;
//! 3. the batch (F4, [`batch`]): warnings, keyed facts, shared row fields,
//!    hints and merged related calls, stated once in the envelope.
//!
//! One reading rule recovers every value: an item lacking a field reads it
//! from its nearest holder, then the row's `data`, then the envelope's
//! `shared.data`, then the request; an omitted value is the one its rule
//! derives. The stage never enters an executable call (`tool` beside
//! `input`) or a row's `next` and `related` channels, so every emitted call
//! runs unchanged; `path` and `line` are never planned. Tools that cut their
//! own pages price a row as the stage states it ([`apply_on`]).
mod batch;
mod holder;
mod omit;
mod outline;
mod pattern;
mod plan;

pub(crate) use pattern::CALL_CHANNELS;

use crate::contracts::output_fields::page_context_fields;
use crate::id::ToolId;
use serde_json::{Map, Value};

/// States once what `tool`'s plan lets the result rows of `structured`
/// share. A row's statement depends on its `data` alone ([`state_row`]), and
/// never reads or moves the calls a row offers (`next`, `related`), so a row
/// whose `data` is an earlier row's apart from those calls (a batch that asks
/// the same query twice: each row continues under its own handle) takes
/// that row's stated `data` with its own calls, instead of stating it again.
pub(crate) fn apply(tool: ToolId, structured: &mut Value) {
    let plan = plan::of(tool);
    if let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) {
        let same = same_data(rows, !plan.names_calls());
        for at in 0..rows.len() {
            let stated = same[at].and_then(|earlier| rows[earlier].get("data").cloned());
            let Some(data) = rows[at].get_mut("data").filter(|data| data.is_object()) else {
                continue;
            };
            match stated {
                Some(Value::Object(mut stated)) => {
                    if let Value::Object(own) = data {
                        for channel in pattern::CALL_CHANNELS {
                            if let (Some(call), Some(slot)) =
                                (own.get_mut(channel), stated.get_mut(channel))
                            {
                                *slot = std::mem::take(call);
                            }
                        }
                    }
                    *data = Value::Object(stated);
                }
                _ => state_row(tool, plan, data),
            }
        }
    }
    batch::apply(&plan.batch, structured);
}

/// For each row, the first earlier row whose object `data` is its own as
/// text (member order included), compared before either is stated; with
/// `calls_apart`, apart from the values of its top-level call channels.
fn same_data(rows: &[Value], calls_apart: bool) -> Vec<Option<usize>> {
    fn data(row: &Value) -> Option<&Map<String, Value>> {
        row.get("data").and_then(Value::as_object)
    }
    let mut same: Vec<Option<usize>> = Vec::with_capacity(rows.len());
    for (at, row) in rows.iter().enumerate() {
        let earlier = data(row).and_then(|own| {
            (0..at).find(|&earlier| {
                same[earlier].is_none()
                    && data(&rows[earlier]).is_some_and(|other| {
                        other.len() == own.len()
                            && other
                                .iter()
                                .zip(own)
                                .all(|((key, value), (own_key, own_value))| {
                                    key == own_key
                                        && ((calls_apart
                                            && pattern::CALL_CHANNELS.contains(&key.as_str()))
                                            || same_text(value, own_value))
                                })
                    })
            })
        });
        same.push(earlier);
    }
    same
}

/// Whether `a` and `b` serialize alike: equal, with object members in the
/// same order (`==` on objects ignores their order).
fn same_text(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|((key, value), (other_key, other))| {
                    key == other_key && same_text(value, other)
                })
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(value, other)| same_text(value, other))
        }
        _ => a == b,
    }
}

/// [`apply`] for one result row's `data`, as the stage states it before the
/// batch step: a tool that sizes its rows before the response stage
/// measures them as they are published.
pub fn apply_on(tool: ToolId, data: &mut Map<String, Value>) {
    let mut row = Value::Object(std::mem::take(data));
    state_row(tool, plan::of(tool), &mut row);
    if let Value::Object(stated) = row {
        *data = stated;
    }
}

/// The omitted values of [`apply_on`] for one `item` at row pattern `at`
/// (`/data/files/*/symbols/*`): what a tool that sizes items one by one
/// prices. A rule whose source lies outside the item keeps its value, so the
/// item is never priced shorter than it is published.
pub fn apply_at(tool: ToolId, at: &str, item: &mut Value) {
    let prefix = pattern::Pattern::parse(at.strip_prefix("/data").unwrap_or(at));
    let plan = plan::of(tool);
    let entries = plan
        .omit
        .iter()
        .filter_map(|entry| entry.within(&prefix))
        .collect::<Vec<_>>();
    omit::apply(&entries, item);
    for reference in plan
        .refs
        .iter()
        .filter(|reference| reference.target == prefix)
    {
        omit::nested(&reference.tail, &entries, item);
    }
}

fn state_row(tool: ToolId, plan: &plan::Plan, data: &mut Value) {
    omit::apply(&plan.omit, data);
    omit::through(&plan.refs, &plan.omit, data);
    let context = page_context_fields(tool);
    holder::apply(&plan.holders, data, &|pointer| context.at(pointer));
    outline::apply(&plan.outline, data);
}

#[cfg(test)]
mod tests;
