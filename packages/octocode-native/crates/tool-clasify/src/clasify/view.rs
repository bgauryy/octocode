//! The two views of a row. `details` is the row as it is built: every
//! field. `summary`, the default, is what decides the next step: each
//! answer's value, the lines of the deciding part, one coverage, the token
//! spend, and the ready calls. Everything a summary leaves out is in
//! `details`, so nothing is lost; a summary only removes.
//!
//! A row states the `lines` that decide an answer and a `coverage` where its
//! source can: a text judged whole names lines through its companion choice
//! (`locate`), a sectioned read or a units row states a `coverage`. The
//! exceptions are a URL capture, a text of one line, a `box` question, a
//! request the provider's window split into single questions, and a read
//! whose text holds fewer lines than its `ranges` name: those answers state
//! none.
//!
//! What a summary removes, by rule:
//!
//! - the distribution behind a value (`probabilities`, `confidence`) and the
//!   `legend`, which restates the request's criteria; a choice whose sections
//!   disagreed keeps each label's share;
//! - the answering `model`, and a request count of one;
//! - image provenance, except the pixel sizes a `box` maps to;
//! - a section's `id`, `kind`, `name`, `context` and byte range, and the
//!   `status` wrapper of an answered cell;
//! - quiet units, which stay counted in `coverage.quiet` and named in a
//!   warning, so each can be asked about by its key;
//! - a `sections` lead that is longer than the source it would split.
use octocode_contract_types::ClasifyChannel;
use octocode_runtime_base::id::ToolId;
use serde_json::{Map, Value, json};

/// The fields of an answer that say how its value was reached.
const ANSWER_DETAIL: [&str; 3] = ["probabilities", "confidence", "legend"];
/// The fields of a section that locate and name it beyond its lines.
const SECTION_DETAIL: [&str; 4] = ["id", "kind", "name", "context"];
/// The image fields a `box` maps to: the judged size and the original's.
const BOX_FRAME: [&str; 4] = ["width", "height", "sourceWidth", "sourceHeight"];

/// A continuation of a `details` row stays in `details`: the next page of a
/// plan, or of a tool's rows, reads as the page before it did.
pub(super) fn carry_details(row: &mut Value) {
    // The row is the details already: no call repeats it.
    if let Some(next) = row.get_mut("next").and_then(Value::as_object_mut) {
        next.shift_remove(ClasifyChannel::Details.key());
        if next.is_empty()
            && let Some(fields) = row.as_object_mut()
        {
            fields.shift_remove("next");
        }
    }
    let Some(next) = row.get_mut("next").and_then(Value::as_object_mut) else {
        return;
    };
    for call in next.values_mut() {
        if call["tool"] != ToolId::Clasify.as_str() {
            continue;
        }
        for query in call["input"]["queries"]
            .as_array_mut()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object_mut)
        {
            query.insert("view".into(), json!("details"));
        }
    }
}

/// Cut `row` to its summary, in place. `shared`: a sibling row of the call
/// states `calls: 0` because this one paid for its request, so every row
/// states its `calls`, a single request included.
pub(super) fn summarize(row: &mut Value, shared: bool) {
    let Some(fields) = row.as_object_mut() else {
        return;
    };
    if fields.contains_key("errorCode") && !fields.contains_key("resources") {
        return;
    }
    if fields.contains_key("models") {
        return;
    }
    fields.shift_remove("model");
    if fields.contains_key("units") {
        units(fields, shared);
    } else if fields.contains_key("answers") {
        evaluation(fields, shared);
    } else if let Some(Value::Object(resources)) = fields.get_mut("resources") {
        let sectioned = resources
            .values()
            .any(|resource| resource.get("sections").is_some());
        for resource in resources.values_mut().filter_map(Value::as_object_mut) {
            if sectioned {
                resource_sections(resource);
            } else if resource.contains_key("answers") {
                resource.shift_remove("model");
                evaluation(resource, shared);
            }
        }
        if sectioned {
            plan_usage(fields);
        }
    }
}

/// One answer's summary. `sectioned`: the row combined sections, so a
/// choice's probabilities are the sections' shares, not the provider's.
fn answer(answer: &mut Value, sectioned: bool) {
    if let Some(fields) = answer.as_object_mut() {
        // Sections that disagreed on a choice: the minority's share is the
        // one sign that the majority label is not the whole answer.
        let split = sectioned
            && fields.contains_key("lines")
            && fields.contains_key("choice")
            && fields["probabilities"].as_object().is_some_and(|shares| {
                shares
                    .values()
                    .filter(|share| share.as_f64().is_some_and(|share| share > 0.0))
                    .count()
                    > 1
            });
        // A sectioned no names its highest section only in `details`.
        if sectioned
            && fields
                .get("noul")
                .and_then(Value::as_f64)
                .is_some_and(|noul| noul < 0.5)
        {
            fields.shift_remove("lines");
        }
        for field in ANSWER_DETAIL {
            if split && field == "probabilities" {
                continue;
            }
            fields.shift_remove(field);
        }
    }
}

/// A summary states the billing facts that change a decision: `calls` when
/// more than one request billed or a sibling row shared it, and `refused`. The token counts are the
/// `details` view's, and one request is the common case: its count says
/// nothing.
fn usage(fields: &mut Map<String, Value>, shared: bool) {
    let Some(usage) = fields.get_mut("usage").and_then(Value::as_object_mut) else {
        return;
    };
    usage.shift_remove("inputTokens");
    usage.shift_remove("outputTokens");
    if !shared && usage.get("calls") == Some(&json!(1)) {
        usage.shift_remove("calls");
    }
    if usage.is_empty() {
        fields.shift_remove("usage");
    }
}

/// An answered evaluation: `answers` over one source, content or images.
fn evaluation(fields: &mut Map<String, Value>, shared: bool) {
    let sectioned = fields.contains_key("coverage");
    let mut boxed = false;
    for value in fields
        .get_mut("answers")
        .and_then(Value::as_object_mut)
        .into_iter()
        .flat_map(|answers| answers.values_mut())
    {
        boxed |= value.get("box").is_some();
        answer(value, sectioned);
    }
    usage(fields, shared);
    // A box is a fraction of an image: its pixels need the image's size.
    if boxed {
        for asset in fields
            .get_mut("assets")
            .and_then(Value::as_object_mut)
            .into_iter()
            .flat_map(|assets| assets.values_mut())
            .filter_map(Value::as_object_mut)
        {
            asset.retain(|field, _| BOX_FRAME.contains(&field.as_str()));
        }
    } else {
        fields.shift_remove("assets");
    }
    short_source_leads(fields);
}

/// Drops a related `sections` call that is longer than the source it would
/// split: reading that source (`next.read`) costs less than the lead.
fn short_source_leads(fields: &mut Map<String, Value>) {
    let Some(read_chars) = fields.get("readChars").and_then(Value::as_u64) else {
        return;
    };
    let Some(Value::Array(related)) = fields.get_mut("related") else {
        return;
    };
    related.retain(|call| {
        let sections = call["tool"] == ToolId::Clasify.as_str()
            && call["input"]["queries"][0]["operation"] == "sections";
        !sections || (call.to_string().chars().count() as u64) < read_chars
    });
    if related.is_empty() {
        fields.shift_remove("related");
    }
}

/// An answered cell is its answer; any other keeps its status and reason.
fn cell(cell: &mut Value) {
    if cell["status"] == "answered" {
        let mut inner = cell["answer"].take();
        answer(&mut inner, false);
        *cell = inner;
    }
}

/// One resource of a `sections` row: each section is its lines and answers.
fn resource_sections(resource: &mut Map<String, Value>) {
    for section in resource
        .get_mut("sections")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        for field in SECTION_DETAIL {
            section.shift_remove(field);
        }
        if let Some(target) = section.get_mut("target").and_then(Value::as_object_mut) {
            target.shift_remove("bytes");
        }
        for value in section
            .get_mut("answers")
            .and_then(Value::as_object_mut)
            .into_iter()
            .flat_map(|answers| answers.values_mut())
        {
            cell(value);
        }
    }
}

/// A plan's spend as an evaluation states it, when every request reported
/// its usage and nothing else is said: the requests; tokens are `details`.
fn plan_usage(fields: &mut Map<String, Value>) {
    let Some(usage) = fields.get_mut("usage").and_then(Value::as_object_mut) else {
        return;
    };
    let complete = usage.get("unknownUsageCalls") == Some(&json!(0))
        && usage
            .keys()
            .all(|field| PLAN_USAGE.contains(&field.as_str()));
    if complete {
        usage.retain(|field, _| field == "calls");
    }
}

/// What a plan's usage holds when it says no more than its spend.
const PLAN_USAGE: [&str; 6] = [
    "calls",
    "inputTokens",
    "outputTokens",
    "knownUsageCalls",
    "unknownUsageCalls",
    "models",
];

/// A units row: quiet units leave (they stay counted), cells unwrap.
fn units(fields: &mut Map<String, Value>, shared: bool) {
    let mut left_out = Vec::new();
    if let Some(Value::Object(units)) = fields.get_mut("units") {
        units.retain(|key, cells| {
            let quiet = super::units::is_quiet(cells);
            if quiet {
                left_out.push(key.clone());
            }
            !quiet
        });
        for value in units
            .values_mut()
            .filter_map(Value::as_object_mut)
            .flat_map(|cells| cells.values_mut())
        {
            cell(value);
        }
    }
    if !left_out.is_empty() {
        octocode_runtime_base::response::paging::push_warnings(
            fields,
            [format!(
                "{} quiet {} left out of this summary (every answer below the quiet threshold): {}; view:\"details\" repeats the call and lists them.",
                left_out.len(),
                if left_out.len() == 1 { "unit" } else { "units" },
                left_out.join(", "),
            )],
        );
    }
    usage(fields, shared);
}

#[cfg(test)]
mod tests;
