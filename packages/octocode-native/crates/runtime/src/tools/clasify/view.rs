//! The two views of a row. `details` is the row as it is built: every
//! field. `summary`, the default, is what decides the next step: each
//! answer's value, the lines of the deciding part, one coverage, the token
//! spend, and the ready calls. Everything a summary leaves out is in
//! `details`, so nothing is lost; a summary only removes.
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
//! - quiet units, which stay counted in `coverage.quiet`;
//! - a `sections` lead that is longer than the source it would split.
use crate::id::ToolId;
use octocode_contract_types::ClasifyChannel;
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

/// Cut `row` to its summary, in place.
pub(super) fn summarize(row: &mut Value) {
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
        units(fields);
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

fn answer(answer: &mut Value) {
    if let Some(fields) = answer.as_object_mut() {
        // Sections that disagreed on a choice: the minority's share is the
        // one sign that the majority label is not the whole answer.
        let split = fields.contains_key("lines")
            && fields.contains_key("choice")
            && fields["probabilities"].as_object().is_some_and(|shares| {
                shares
                    .values()
                    .filter(|share| share.as_f64().is_some_and(|share| share > 0.0))
                    .count()
                    > 1
            });
        for field in ANSWER_DETAIL {
            if split && field == "probabilities" {
                continue;
            }
            fields.shift_remove(field);
        }
    }
}

/// One request is the common case: its count says nothing.
fn usage(fields: &mut Map<String, Value>) {
    if let Some(usage) = fields.get_mut("usage").and_then(Value::as_object_mut)
        && usage.get("calls") == Some(&json!(1))
    {
        usage.shift_remove("calls");
    }
}

/// An answered evaluation: `answers` over one source, content or images.
fn evaluation(fields: &mut Map<String, Value>) {
    let mut boxed = false;
    for value in fields
        .get_mut("answers")
        .and_then(Value::as_object_mut)
        .into_iter()
        .flat_map(|answers| answers.values_mut())
    {
        boxed |= value.get("box").is_some();
        answer(value);
    }
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
        answer(&mut inner);
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
/// its usage and nothing else is said: tokens and requests.
fn plan_usage(fields: &mut Map<String, Value>) {
    let Some(usage) = fields.get_mut("usage").and_then(Value::as_object_mut) else {
        return;
    };
    let complete = usage.get("unknownUsageCalls") == Some(&json!(0))
        && usage
            .keys()
            .all(|field| PLAN_USAGE.contains(&field.as_str()));
    if complete {
        usage.retain(|field, _| PLAN_SPEND.contains(&field.as_str()));
    }
}

/// The spend fields both usage forms share.
const PLAN_SPEND: [&str; 3] = ["calls", "inputTokens", "outputTokens"];
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
fn units(fields: &mut Map<String, Value>) {
    if let Some(Value::Object(units)) = fields.get_mut("units") {
        units.retain(|_, cells| !super::units::is_quiet(cells));
        for value in units
            .values_mut()
            .filter_map(Value::as_object_mut)
            .flat_map(|cells| cells.values_mut())
        {
            cell(value);
        }
    }
    usage(fields);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every leaf a summary states is a leaf of the details row: a summary
    /// only removes, or unwraps an answered cell to its answer.
    fn within(summary: &Value, details: &Value) -> bool {
        match (summary, details) {
            (Value::Object(summary), Value::Object(details)) => {
                if details.get("status") == Some(&json!("answered"))
                    && !summary.contains_key("status")
                {
                    return within(&Value::Object(summary.clone()), &details["answer"]);
                }
                summary
                    .iter()
                    .all(|(key, value)| details.get(key).is_some_and(|other| within(value, other)))
            }
            (Value::Array(summary), Value::Array(details)) => summary
                .iter()
                .all(|value| details.iter().any(|other| within(value, other))),
            _ => summary == details,
        }
    }

    fn summary_of(details: &Value) -> Value {
        let mut summary = details.clone();
        summarize(&mut summary);
        assert!(within(&summary, details), "{summary}\n{details}");
        summary
    }

    #[test]
    fn an_evaluation_keeps_each_answers_value_and_its_tokens() {
        let details = json!({
            "model":"jev-1",
            "answers":{
                "restart":{"noul":0.96},
                "kind":{"choice":"ops","confidence":0.81,"probabilities":{"ops":0.93,"code":0.07}},
                "risk":{"score":1.5,"confidence":0.5,"probabilities":{"0":0.0,"1":0.5,"2":0.5},
                    "legend":{"0":"low","1":"medium","2":"high"}},
            },
            "usage":{"calls":1,"inputTokens":572,"outputTokens":30},
        });
        assert_eq!(
            summary_of(&details),
            json!({
                "answers":{"restart":{"noul":0.96},"kind":{"choice":"ops"},"risk":{"score":1.5}},
                "usage":{"inputTokens":572,"outputTokens":30},
            })
        );
    }

    /// A combined answer keeps its lines and the row its coverage; several
    /// requests stay counted.
    #[test]
    fn a_sectioned_evaluation_keeps_lines_coverage_and_its_request_count() {
        let details = json!({
            "model":"jev-1",
            "answers":{"kind":{"choice":"a","confidence":0.75,"probabilities":{"a":0.75,"b":0.25},
                "lines":{"startLine":21,"endLine":30}}},
            "coverage":{"totalSections":4,"answered":4},
            "usage":{"calls":5,"inputTokens":400,"outputTokens":20},
            "next":{"read":{"kind":"related","tool":"localFetch","input":{"queries":[{"path":"a"}]}}},
        });
        let summary = summary_of(&details);
        // The sections disagreed: each label's share stays.
        assert_eq!(
            summary["answers"]["kind"],
            json!({"choice":"a","probabilities":{"a":0.75,"b":0.25},
                "lines":{"startLine":21,"endLine":30}})
        );
        let mut agreed = details.clone();
        agreed["answers"]["kind"]["probabilities"] = json!({"a":1.0,"b":0.0});
        assert_eq!(
            summary_of(&agreed)["answers"]["kind"],
            json!({"choice":"a","lines":{"startLine":21,"endLine":30}})
        );
        assert_eq!(summary["coverage"], details["coverage"]);
        assert_eq!(summary["usage"]["calls"], 5);
        assert_eq!(summary["next"], details["next"]);
    }

    #[test]
    fn image_provenance_stays_only_as_the_size_a_box_maps_to() {
        let asset = json!({
            "source":{"tool":"localFetch","query":{"path":"shot.png"}},
            "sourceDigest":"ab","format":"png","bytes":10,"width":716,"height":1075,
            "sourceWidth":1432,"sourceHeight":2150,"path":"/tmp/x.png","expiresAt":"2026",
        });
        let boxed = json!({
            "answers":{"where":{"box":{"top":0.1,"bottom":0.5,"left":0.1,"right":0.4},"confidence":0.9}},
            "assets":{"shot":asset},
        });
        let summary = summary_of(&boxed);
        assert_eq!(
            summary["assets"]["shot"],
            json!({"width":716,"height":1075,"sourceWidth":1432,"sourceHeight":2150})
        );
        assert!(summary["answers"]["where"].get("confidence").is_none());
        let plain = json!({"answers":{"logo":{"noul":0.9}},"assets":{"shot":asset}});
        assert!(summary_of(&plain).get("assets").is_none());
    }

    #[test]
    fn a_section_is_its_lines_and_answers() {
        let details = json!({
            "model":"jev-1",
            "resources":{"mid":{"status":"ready","sourceDigest":"d4","sections":[{
                "id":"s12","kind":"declarations","name":"f_376 … f_406 (31 declarations)",
                "target":{"bytes":[23784,25751],"lines":{"startLine":1876,"endLine":2027}},
                "context":[{"bytes":[21784,23784],"lines":{"startLine":1717,"endLine":1875}}],
                "answers":{
                    "p":{"status":"answered","answer":{"noul":0.96}},
                    "kind":{"status":"answered","answer":{"choice":"ops","confidence":0.8,"probabilities":{"ops":0.9}}},
                    "late":{"status":"unassessed","reason":"deadline"},
                },
            }],"coverage":{"totalSections":1,"answered":2,"unassessed":1}}},
            "usage":{"calls":1,"inputTokens":5,"knownUsageCalls":1,"models":["jev-1"],"outputTokens":3,"unknownUsageCalls":0},
        });
        let summary = summary_of(&details);
        let section = &summary["resources"]["mid"]["sections"][0];
        assert_eq!(
            section,
            &json!({
                "target":{"lines":{"startLine":1876,"endLine":2027}},
                "answers":{
                    "p":{"noul":0.96},
                    "kind":{"choice":"ops"},
                    "late":{"status":"unassessed","reason":"deadline"},
                },
            })
        );
        // The owner's bound: a summary section is at most 40% of the details one.
        let (short, long) = (
            section.to_string().len(),
            details["resources"]["mid"]["sections"][0].to_string().len(),
        );
        assert!(short * 100 <= long * 40, "{short} of {long}");
        assert_eq!(
            summary["usage"],
            json!({"calls":1,"inputTokens":5,"outputTokens":3})
        );
        assert_eq!(
            summary["resources"]["mid"]["coverage"],
            details["resources"]["mid"]["coverage"]
        );
    }

    /// Usage that says more than its spend (unreported requests, an estimate
    /// of the rest) is kept whole.
    #[test]
    fn a_plans_usage_stays_whole_when_it_says_more_than_its_spend() {
        let details = json!({
            "resources":{"a":{"status":"ready","sections":[]}},
            "usage":{"calls":2,"inputTokens":5,"knownUsageCalls":1,"outputTokens":3,"unknownUsageCalls":1},
        });
        assert_eq!(summary_of(&details)["usage"], details["usage"]);
    }

    #[test]
    fn a_sections_lead_longer_than_its_source_is_dropped() {
        let lead = json!({"kind":"related","tool":"clasify","input":{"queries":[{
            "operation":"sections","questions":{"p":{"type":"noul","instructions":"Is it planted?"}},
            "resources":{"source":{"tool":"localFetch","query":{"path":"tiny.rs"}}}}]}});
        let row = |read_chars: u64| json!({"answers":{"p":{"noul":0.9}},"readChars":read_chars,"related":[lead]});
        assert!(summary_of(&row(43)).get("related").is_none());
        assert_eq!(summary_of(&row(77_296))["related"], json!([lead]));
    }

    #[test]
    fn quiet_units_leave_a_summary_and_stay_counted() {
        let details = json!({
            "units":{
                "a.rs:3":{"auth":{"status":"answered","answer":{"noul":0.91}}},
                "a.rs:9":{"auth":{"status":"answered","answer":{"noul":0.02}}},
                "b.rs:1":{"auth":{"status":"error","error":{"errorCode":"timeout","error":"late"}}},
            },
            "coverage":{"totalUnits":3,"answered":2,"quiet":1,"error":1},
            "usage":{"calls":3,"inputTokens":9,"outputTokens":3},
        });
        let summary = summary_of(&details);
        assert_eq!(
            summary["units"],
            json!({
                "a.rs:3":{"auth":{"noul":0.91}},
                "b.rs:1":{"auth":{"status":"error","error":{"errorCode":"timeout","error":"late"}}},
            })
        );
        assert_eq!(summary["coverage"], details["coverage"]);
    }

    #[test]
    fn a_failed_row_and_a_model_listing_are_left_whole() {
        for row in [
            json!({"errorCode":"classificationProviderError","error":"x","usage":{"calls":1,"inputTokens":0,"outputTokens":0}}),
            json!({"models":[{"name":"jev","description":"d","releaseDate":"r"}]}),
        ] {
            assert_eq!(summary_of(&row), row);
        }
    }

    #[test]
    fn a_details_continuation_stays_in_details() {
        let mut row = json!({"next":{
            "nextPage":{"kind":"continue","tool":"clasify","input":{"queries":[{"operation":"sections","assessment":"t"}]}},
            "read":{"kind":"continue","tool":"localFetch","input":{"queries":[{"path":"a"}]}},
        }});
        carry_details(&mut row);
        assert_eq!(
            row["next"]["nextPage"]["input"]["queries"][0]["view"],
            "details"
        );
        assert!(
            row["next"]["read"]["input"]["queries"][0]
                .get("view")
                .is_none()
        );
    }
}
