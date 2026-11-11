//! A source too large for one provider window, answered in one row: it is
//! judged in sections ([`sections::whole`]), each question on every section,
//! and the sections' answers combine into one answer per question that names
//! the lines of its deciding section. The rule per question type:
//!
//! - `noul`: the highest section (a fact stated anywhere in the source holds
//!   for the source);
//! - `score`: the highest level, for the same reason;
//! - `choice`: the label of the most confident section that did not abstain.
//!   A section abstains when it chose `unknown`, the label the question
//!   offers for "this text does not say": a section that lacks the evidence
//!   says nothing about the source, so many of them never outvote the one
//!   that holds it. Only when every section abstained is the answer
//!   `unknown`. Each label's share of the sections is its probability, so a
//!   disagreement stays visible.
//!
//! Every section's own verdict stays one call away: the row offers the same
//! questions as a `sections` call over the source.
use super::sections::{self, JudgedSection};
use super::{Row, recovery, reply};
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::tools::source_capture::{self, SourceReader};
use serde_json::{Map, Value, json};

/// A `noul` answer at or above this is a yes: its section is worth reading.
const YES: f64 = 0.5;

/// A combined row: its output, and its typed answers in question order.
pub(super) struct Combined {
    pub output: Value,
}

/// Judge `source` in sections and combine them. `refused` is the provider
/// spend of the whole-source attempt that the window refused. When some
/// question gets no answer from any section, the error is the provider
/// requests the sections sent: the refusal stays the row's error and counts
/// them.
pub(super) fn judge(
    row: &Row,
    source: &Value,
    refused: u64,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    can_resume: bool,
) -> Result<Combined, u64> {
    let whole = sections::whole(row, source, dispatcher, context).map_err(|_| 0_u64)?;
    if whole.metadata["status"] != "ready" || whole.sections.is_empty() {
        return Err(whole.usage.calls);
    }
    let mut answers = Map::new();
    for (index, (key, question)) in row.keys.iter().zip(&row.questions).enumerate() {
        let answer = combine(question, &whole.sections, index).ok_or(whole.usage.calls)?;
        answers.insert(key.clone(), answer);
    }
    let mut output = json!({
        "answers": answers,
        "coverage": coverage(&whole.sections),
        "usage": reply::evaluation_usage(
            whole.usage.input_tokens,
            whole.usage.output_tokens,
            whole.usage.calls.saturating_add(refused),
        ),
    });
    if let Some(model) = whole.model {
        output["model"] = json!(model);
    }
    if let Some(read) = deciding_read(source, &whole.metadata, &output["answers"], can_resume) {
        crate::response::result::attach_calls(&mut output, json!({"read":read}));
    }
    if let Some(call) = recovery::sections_call(row, source, can_resume) {
        crate::response::result::attach_related(&mut output, call);
    }
    Ok(Combined { output })
}

/// Question `index`'s answered sections: each one's lines and answer.
fn answered(sections: &[JudgedSection], index: usize) -> Vec<(&Value, &Value)> {
    sections
        .iter()
        .filter_map(|section| {
            let cell = section.cells.get(index)?.as_ref()?;
            (cell["status"] == "answered").then(|| (&section.lines, &cell["answer"]))
        })
        .collect()
}

/// The section whose `field` is highest; the first of equals, in source order.
fn highest<'a>(answers: &[(&'a Value, &'a Value)], field: &str) -> Option<(&'a Value, &'a Value)> {
    answers.iter().copied().reduce(|best, next| {
        if next.1[field].as_f64() > best.1[field].as_f64() {
            next
        } else {
            best
        }
    })
}

/// One answer for `question` from every section's answer to it; `None` when
/// no section answered.
fn combine(question: &Value, sections: &[JudgedSection], index: usize) -> Option<Value> {
    let answers = answered(sections, index);
    let kind = question["type"].as_str()?;
    let combined = match kind {
        "noul" => {
            let (lines, answer) = highest(&answers, "noul")?;
            json!({"noul":answer["noul"],"lines":lines})
        }
        "score" => {
            let (lines, answer) = highest(&answers, "score")?;
            let mut combined = answer.clone();
            combined["lines"] = lines.clone();
            // A section cell carries no legend; the row's answer does.
            combined["legend"] = question["criteria"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(level, criterion)| (level.to_string(), criterion.clone()))
                .collect::<Map<_, _>>()
                .into();
            combined
        }
        "choice" => decide(question, &answers)?,
        _ => return None,
    };
    Some(combined)
}

/// Whether a choice of `label` abstains: `unknown` is the label a choice
/// question offers for evidence the text does not hold (the request schema
/// tells the agent to add it), so a section that chose it has no answer.
fn abstains(label: &str) -> bool {
    label.eq_ignore_ascii_case("unknown")
}

/// The label of the most confident section that did not abstain (the first
/// in source order among equals), or, when every section abstained, the
/// abstention; each label's share of the sections as its probability, and
/// the lines of the deciding section. An answer every section abstained on
/// decides no lines.
fn decide(question: &Value, answers: &[(&Value, &Value)]) -> Option<Value> {
    let labels = question["criteria"].as_object()?;
    let total = answers.len();
    let chosen: Vec<_> = answers
        .iter()
        .copied()
        .filter(|(_, answer)| answer["choice"].as_str().is_some())
        .collect();
    let substantive: Vec<_> = chosen
        .iter()
        .copied()
        .filter(|(_, answer)| {
            answer["choice"]
                .as_str()
                .is_some_and(|label| !abstains(label))
        })
        .collect();
    let deciding = if substantive.is_empty() {
        highest(&chosen, "confidence")?
    } else {
        highest(&substantive, "confidence")?
    };
    let count = |label: &str| {
        answers
            .iter()
            .filter(|(_, answer)| answer["choice"] == label)
            .count()
    };
    // To the provider's own two decimals; a minority never rounds to none.
    let share = |votes: usize| {
        let share = (votes as f64 / total as f64 * 100.0).round() / 100.0;
        if votes > 0 { share.max(0.01) } else { share }
    };
    let probabilities: Map<String, Value> = labels
        .keys()
        .map(|label| (label.clone(), json!(share(count(label)))))
        .collect();
    let (lines, answer) = deciding;
    let mut combined = json!({"choice": answer["choice"], "probabilities": probabilities});
    if let Some(confidence) = answer["confidence"].as_f64() {
        combined["confidence"] = json!(confidence);
    }
    if !substantive.is_empty() {
        combined["lines"] = lines.clone();
    }
    Some(combined)
}

/// The row's coverage, counted as a `sections` resource counts it: outcomes
/// are per section and question.
fn coverage(sections: &[JudgedSection]) -> Value {
    let mut counts = [0usize; 4];
    for cell in sections.iter().flat_map(|section| &section.cells) {
        let slot = match cell.as_ref().and_then(|cell| cell["status"].as_str()) {
            Some("answered") => 0,
            Some("error") => 1,
            Some("unsupported") => 2,
            _ => 3,
        };
        counts[slot] += 1;
    }
    let mut coverage = json!({"totalSections":sections.len(),"answered":counts[0]});
    for (name, count) in [
        ("error", counts[1]),
        ("unsupported", counts[2]),
        ("unassessed", counts[3]),
    ] {
        if count > 0 {
            coverage[name] = json!(count);
        }
    }
    coverage
}

/// One read of the deciding sections' lines (adjacent ones merged): every
/// answer's section except a `noul` no, whose highest section decides
/// nothing. A URL capture has no line read.
fn deciding_read(
    source: &Value,
    metadata: &Value,
    answers: &Value,
    can_resume: bool,
) -> Option<Value> {
    let tool = source["tool"]
        .as_str()
        .and_then(ToolId::from_name)
        .filter(|tool| matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent))?;
    let mut lines: Vec<(u64, u64)> = answers
        .as_object()?
        .values()
        .filter(|answer| answer["noul"].as_f64().is_none_or(|noul| noul >= YES))
        .filter_map(|answer| {
            let lines = &answer["lines"];
            Some((lines["startLine"].as_u64()?, lines["endLine"].as_u64()?))
        })
        .collect();
    lines.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (start, end) in lines {
        match merged.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    if merged.is_empty() {
        return None;
    }
    let mut query = source["query"].clone();
    query["ranges"] = merged
        .into_iter()
        .map(|(start, end)| json!({"startLine":start,"endLine":end}))
        .collect();
    let pin = metadata["snapshot"].as_str();
    Some(if can_resume {
        source_capture::read_lead(tool, &query, pin)
    } else {
        source_capture::fresh_read(tool, &query, pin)
    })
}

#[cfg(test)]
mod tests;
