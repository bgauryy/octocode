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
//! - the answering `model`;
//! - image provenance, except the pixel sizes a `box` maps to;
//! - a section's `id`, `kind`, `name`, `context` and byte range, and the
//!   `status` wrapper of an answered cell;
//! - quiet units in rows without predicates, which stay counted in
//!   `coverage.quiet` and named in a warning. A predicate summary keeps its
//!   scores so using them costs no further requests;
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

/// Cut `row` to its summary, in place. `details_saved`: the page offers its
/// saved details (`next.details`), so the warning naming what was left out
/// points there instead of a billed repeat.
pub(super) fn summarize(row: &mut Value, keep_quiet: bool, details_saved: bool) {
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
    if fields.contains_key("units") || fields.contains_key("files") {
        units(fields, keep_quiet, details_saved);
    } else if fields.contains_key("answers") {
        evaluation(fields);
    } else if let Some(Value::Object(resources)) = fields.get_mut("resources") {
        let sectioned = resources
            .values()
            .any(|resource| resource.get("sections").is_some());
        for resource in resources.values_mut().filter_map(Value::as_object_mut) {
            if sectioned {
                resource_sections(resource);
            } else if resource.contains_key("answers") {
                resource.shift_remove("model");
                evaluation(resource);
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
        // A choice keeps its chosen label's probability, what acting on it
        // is weighed by; the other labels' stay in details.
        let chosen = fields
            .get("choice")
            .and_then(Value::as_str)
            .map(str::to_owned);
        for field in ANSWER_DETAIL {
            if field == "probabilities" {
                if split {
                    continue;
                }
                if let (Some(label), Some(Value::Object(shares))) =
                    (chosen.as_deref(), fields.get_mut(field))
                    && shares.contains_key(label)
                {
                    shares.retain(|key, _| key == label);
                    continue;
                }
            }
            fields.shift_remove(field);
        }
    }
}

/// A summary states the billing facts that change a decision: the `calls`
/// the row billed (beside its ceiling, what it spent of it; 0 when a twin
/// row paid), the tokens they cost (what offloading a read is weighed by),
/// `refused`, the ceiling, and the plan when it differs from what was sent.
fn usage(fields: &mut Map<String, Value>) {
    let Some(usage) = fields.get_mut("usage").and_then(Value::as_object_mut) else {
        return;
    };
    // The plan that came true is the request count itself.
    if usage.get("requestsPlanned").is_some() && usage.get("requestsPlanned") == usage.get("calls")
    {
        usage.shift_remove("requestsPlanned");
    }
    if usage.is_empty() {
        fields.shift_remove("usage");
    }
}

/// An answered evaluation: `answers` over one source, content or images.
fn evaluation(fields: &mut Map<String, Value>) {
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
    parts(fields);
    usage(fields);
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

/// The parts of a source an evaluation judged in sections: quiet parts leave
/// (the row's `coverage.quiet` counts them, `details` lists them), and each
/// part left is its lines and answers.
fn parts(fields: &mut Map<String, Value>) {
    let Some(Value::Array(sections)) = fields.get_mut("sections") else {
        return;
    };
    sections.retain(|section| !super::units::is_quiet(&section["answers"]));
    resource_sections(fields);
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
/// its usage and nothing else is said: the requests and the page's ceiling,
/// their tokens, and its first round only when it differs from them.
fn plan_usage(fields: &mut Map<String, Value>) {
    let Some(usage) = fields.get_mut("usage").and_then(Value::as_object_mut) else {
        return;
    };
    let complete = usage.get("unknownUsageCalls") == Some(&json!(0))
        && usage
            .keys()
            .all(|field| PLAN_USAGE.contains(&field.as_str()));
    if complete {
        let planned_is_calls = usage.get("requestsPlanned") == usage.get("calls");
        usage.retain(|field, _| {
            matches!(
                field.as_str(),
                "calls" | "inputTokens" | "outputTokens" | "requestsCeiling"
            ) || (field == "requestsPlanned" && !planned_is_calls)
        });
    }
}

/// What a plan's usage holds when it says no more than its spend and price.
const PLAN_USAGE: [&str; 8] = [
    "calls",
    "inputTokens",
    "outputTokens",
    "knownUsageCalls",
    "unknownUsageCalls",
    "models",
    "requestsPlanned",
    "requestsCeiling",
];

/// A units row: cells unwrap, and a row without predicates hides quiet units.
fn units(fields: &mut Map<String, Value>, keep_quiet: bool, details_saved: bool) {
    let mut left_out = Vec::new();
    if let Some(Value::Object(files)) = fields.get_mut("files") {
        for (path, lines) in files.iter_mut() {
            if let Some(lines) = lines.as_object_mut() {
                shown_units(lines, keep_quiet, &mut left_out, |line| {
                    format!("{path}:{line}")
                });
            }
        }
        files.retain(|_, lines| lines.as_object().is_some_and(|lines| !lines.is_empty()));
        if files.is_empty() {
            fields.shift_remove("files");
        }
    }
    if let Some(Value::Object(units)) = fields.get_mut("units") {
        shown_units(units, keep_quiet, &mut left_out, str::to_owned);
    }
    if !left_out.is_empty() {
        octocode_runtime_base::response::paging::push_warnings(
            fields,
            [format!(
                "{} quiet {} left out of this summary (every answer below the quiet threshold): {}; {}.",
                left_out.len(),
                if left_out.len() == 1 { "unit" } else { "units" },
                named(&left_out),
                if details_saved {
                    "next.details lists them, unbilled"
                } else {
                    "view:\"details\" repeats the call, billed again, and lists them"
                },
            )],
        );
    }
    usage(fields);
}

/// Keeps the units of `units` a summary shows, naming each quiet one it
/// leaves out (by `key`) in `left_out`, and cuts each cell to its summary.
fn shown_units(
    units: &mut Map<String, Value>,
    keep_quiet: bool,
    left_out: &mut Vec<String>,
    key: impl Fn(&str) -> String,
) {
    if !keep_quiet {
        units.retain(|name, cells| {
            let quiet = super::units::is_quiet(cells);
            if quiet {
                left_out.push(key(name));
            }
            !quiet
        });
    }
    for value in units
        .values_mut()
        .filter_map(Value::as_object_mut)
        .flat_map(|cells| cells.values_mut())
    {
        if value.get("status").is_none() {
            answer(value, false);
        } else {
            cell(value);
        }
    }
}

/// Unit keys as a list that names each file once: the `<path>:<line>` keys
/// of one path as `<path>:{3,7,12}` (lines ascending), every other key as it
/// is, in the order each path or key first appears.
fn named(keys: &[String]) -> String {
    /// `<path>:<line>` or `<path>:<line>#<n>`: the path and the line part.
    fn split(key: &str) -> Option<(&str, &str)> {
        let (path, line) = key.rsplit_once(':')?;
        let digits = line.split_once('#').map_or(line, |(digits, _)| digits);
        (!path.is_empty() && !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .then_some((path, line))
    }
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for key in keys {
        let (path, line) =
            split(key).map_or((key.as_str(), None), |(path, line)| (path, Some(line)));
        match (
            line,
            groups
                .iter_mut()
                .find(|(known, lines)| *known == path && !lines.is_empty()),
        ) {
            (Some(line), Some((_, lines))) => lines.push(line),
            (Some(line), None) => groups.push((path, vec![line])),
            (None, _) => groups.push((path, Vec::new())),
        }
    }
    groups
        .into_iter()
        .map(|(path, mut lines)| {
            let order = |line: &&str| {
                let (digits, ordinal) = line.split_once('#').unwrap_or((line, "0"));
                (
                    digits.parse::<u64>().unwrap_or(u64::MAX),
                    ordinal.parse::<u64>().unwrap_or(0),
                )
            };
            lines.sort_by_key(order);
            match lines.as_slice() {
                [] => path.to_owned(),
                [line] => format!("{path}:{line}"),
                _ => format!("{path}:{{{}}}", lines.join(",")),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
