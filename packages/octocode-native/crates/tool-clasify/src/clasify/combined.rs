//! A source too large for one provider window, answered in one row: it is
//! judged in sections ([`sections::whole`]), each question on every section,
//! and the sections' answers combine into one answer per question that names
//! the lines of its deciding section. The rule per question type:
//!
//! - `noul`: the highest section (a fact stated anywhere in the source holds
//!   for the source); a no below 0.5 names its highest section's lines in
//!   `details` only, since no section decided it, and never narrows a read;
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
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::providers::classification::gate::GateLease;
use octocode_tools_shared::judgment::stats::ClassificationUsage;
use octocode_tools_shared::source_capture::{self, SourceReader};
use serde_json::{Map, Value, json};

/// A `noul` answer at or above this is a yes: its section is worth reading.
pub(super) const YES: f64 = 0.5;

/// A combined row: its output, and its typed answers in question order.
pub struct Combined {
    pub output: Value,
}

/// A source no section could answer: what its requests cost, the one
/// non-window reason every question failed for, when there is one (the row
/// then fails as its sections did, with that message), and the count of its
/// sections when they were not admitted (none was sent).
pub(super) struct Failed {
    pub usage: ClassificationUsage,
    pub cause: Option<(ErrorCode, String)>,
    pub unadmitted: Option<u64>,
}

/// Judge `source` in sections and combine them. `refused` is the provider
/// spend of the whole-source attempt that the window refused. A question no
/// section answered becomes an error cell carrying its sections' code, next
/// to the answered ones. Only when no question was answered is the row an
/// error: the refusal stays the row's error, counting the sections'
/// requests, unless every question failed for one other reason. The
/// sections are judged only when `admits` takes their count, each send
/// counted against `cap` ([`sections::whole`]).
#[expect(
    clippy::too_many_arguments,
    reason = "one source's sections: its runtime, its spend so far and its bound"
)]
pub(super) fn judge(
    row: &Row,
    source: &Value,
    refused: u64,
    cap: Option<&octocode_tools_shared::judgment::RequestCap>,
    admits: &dyn Fn(u64) -> bool,
    gate: &GateLease,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    can_resume: bool,
) -> Result<Combined, Box<Failed>> {
    let whole =
        sections::whole(row, source, dispatcher, context, gate, cap, admits).map_err(|failed| {
            Box::new(Failed {
                usage: failed.usage,
                cause: None,
                unadmitted: failed.unadmitted,
            })
        })?;
    if whole.metadata["status"] != "ready" || whole.sections.is_empty() {
        return Err(Box::new(Failed {
            usage: whole.usage,
            cause: None,
            unadmitted: None,
        }));
    }
    let mut answers = Map::new();
    let mut failures = Vec::new();
    for (index, (key, question)) in row.keys.iter().zip(&row.questions).enumerate() {
        match combine(question, &whole.sections, index) {
            Some(answer) => {
                answers.insert(key.clone(), answer);
            }
            None => {
                let (code, message) = unanswered(&whole.sections, index);
                answers.insert(
                    key.clone(),
                    json!({"status":"error","error":{"errorCode":code,"error":message}}),
                );
                failures.push((code, message));
            }
        }
    }
    if failures.len() == row.keys.len() {
        // A spent cap only stops the splits of what the window refused: it is
        // no reason of its own, so the refusal stays the row's.
        let same = failures
            .first()
            .filter(|(code, _)| {
                !matches!(
                    code,
                    ErrorCode::ClassificationStateTooLarge | ErrorCode::ClassificationOverBudget
                ) && failures.iter().all(|(other, _)| other == code)
            })
            .cloned();
        return Err(Box::new(Failed {
            usage: whole.usage,
            cause: same,
            unadmitted: None,
        }));
    }
    let parts = parts(row, &whole.sections);
    let mut coverage = coverage(&whole.sections);
    // The parts a summary holds back, counted as a `sections` row counts them.
    let quiet = parts
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| super::units::is_quiet(&part["answers"]))
        .count();
    if quiet > 0 {
        coverage["quiet"] = json!(quiet);
    }
    let mut output = json!({
        "answers": answers,
        "coverage": coverage,
        "sections": parts,
        "usage": reply::evaluation_usage(
            whole.usage.input_tokens,
            whole.usage.output_tokens,
            whole.usage.calls.saturating_add(refused),
            refused,
        ),
    });
    if let Some(model) = whole.model {
        output["model"] = json!(model);
    }
    let lines = read_lines(
        output["answers"]
            .as_object()
            .into_iter()
            .flat_map(Map::values),
    );
    if let Some(read) = lines_read(
        source,
        whole.pin.as_deref(),
        &lines,
        can_resume,
        &whole.metadata["read"],
    ) {
        octocode_runtime_base::response::result::attach_calls(&mut output, json!({"read":read}));
    }
    if let Some(call) = recovery::sections_call(row, source, can_resume) {
        octocode_runtime_base::response::result::attach_related(&mut output, call);
    }
    Ok(Combined { output })
}

/// Every section as the part of the source it is, in source order: where it
/// sits and its own outcome for every question, as a `sections` row states
/// one. The row's `answers` combine them; the parts keep what each said, so
/// no second, billed `sections` call is needed to learn every place.
fn parts(row: &Row, sections: &[JudgedSection]) -> Value {
    sections
        .iter()
        .map(|section| {
            let answers = row
                .keys
                .iter()
                .enumerate()
                .map(|(index, key)| {
                    let cell = section
                        .cells
                        .get(index)
                        .and_then(Option::as_ref)
                        .cloned()
                        .unwrap_or_else(|| json!({"status":"unassessed"}));
                    (key.clone(), cell)
                })
                .collect::<Map<_, _>>();
            json!({"target": section.target, "answers": answers})
        })
        .collect()
}

/// Why question `index` has no answer: the code and message of the first
/// section error that was not the window's refusal, else the window's, else
/// that no section was assessed.
fn unanswered(sections: &[JudgedSection], index: usize) -> (ErrorCode, String) {
    let errors = sections
        .iter()
        .filter_map(|section| section.cells.get(index)?.as_ref())
        .filter(|cell| cell["status"] == "error")
        .filter_map(|cell| {
            let code =
                serde_json::from_value::<ErrorCode>(cell["error"]["errorCode"].clone()).ok()?;
            let message = cell["error"]["error"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            Some((code, message))
        })
        .collect::<Vec<_>>();
    errors
        .iter()
        .find(|(code, _)| *code != ErrorCode::ClassificationStateTooLarge)
        .or_else(|| errors.first())
        .cloned()
        .unwrap_or_else(|| {
            (
                ErrorCode::ClassificationContextFailed,
                "No section was assessed for this question.".to_owned(),
            )
        })
}

/// Question `index`'s answered sections: each one's lines and answer.
fn answered(sections: &[JudgedSection], index: usize) -> Vec<(&Value, &Value)> {
    sections
        .iter()
        .filter_map(|section| {
            let cell = section.cells.get(index)?.as_ref()?;
            (cell["status"] == "answered").then(|| (section.lines(), &cell["answer"]))
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
            // Below a yes the section is only the strongest weak one, derived
            // rather than picked: `details` states it, a summary does not,
            // and it never narrows the read.
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
pub(super) fn abstains(label: &str) -> bool {
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

/// The deciding lines of `answers`, adjacent spans merged: every answer that
/// names lines but a `noul` no, which decides nothing to read.
fn deciding_lines<'a>(answers: impl IntoIterator<Item = &'a Value>) -> Vec<(u64, u64)> {
    named_lines(
        answers
            .into_iter()
            .filter(|answer| answer["noul"].as_f64().is_none_or(|noul| noul >= YES)),
    )
}

/// The lines a read of `answers` cites: their deciding lines
/// ([`deciding_lines`]); with none, the lines a `noul` no names, the part or
/// section that came nearest to a yes, so the negative is checked where it
/// was closest. Empty when no answer names lines.
pub(super) fn read_lines<'a>(
    answers: impl IntoIterator<Item = &'a Value> + Clone,
) -> Vec<(u64, u64)> {
    let deciding = deciding_lines(answers.clone());
    if deciding.is_empty() {
        named_lines(answers)
    } else {
        deciding
    }
}

/// The lines `answers` name, adjacent spans merged.
fn named_lines<'a>(answers: impl IntoIterator<Item = &'a Value>) -> Vec<(u64, u64)> {
    let mut lines: Vec<(u64, u64)> = answers
        .into_iter()
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
    merged
}

/// One read of `lines` of `source`, pinned to `pin`, the snapshot or commit
/// the source was judged at. A URL capture's lines are its saved file's:
/// `capture`, the read of that whole file, narrowed to them
/// ([`source_capture::narrowed_read`]). No lines has no line read.
pub(super) fn lines_read(
    source: &Value,
    pin: Option<&str>,
    lines: &[(u64, u64)],
    can_resume: bool,
    capture: &Value,
) -> Option<Value> {
    if source.get("url").is_some() {
        return source_capture::narrowed_read(capture, lines);
    }
    let tool = source["tool"]
        .as_str()
        .and_then(ToolId::from_name)
        .filter(|tool| matches!(tool, ToolId::LocalFetch | ToolId::GhGetFileContent))?;
    if lines.is_empty() {
        return None;
    }
    let mut query = source["query"].clone();
    query["ranges"] = lines
        .iter()
        .map(|(start, end)| json!({"startLine":start,"endLine":end}))
        .collect();
    Some(if can_resume {
        source_capture::read_lead(tool, &query, pin)
    } else {
        source_capture::fresh_read(tool, &query, pin)
    })
}

#[cfg(test)]
mod tests;
