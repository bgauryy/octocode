//! A batch's output: completed sections emitted once, coverage, and the
//! terminal outcome that settles every remaining cell. A resource row is
//! repeated on a later batch only when it gains sections or its coverage or
//! status changed: unchanged rows are duplicate metadata, not evidence.
use super::{Outcome, Runtime, Session};
use crate::execution::ExecutionContext;
use crate::tools::judgment::stats::ClassificationUsage;
use serde_json::{Value, json};

pub(super) fn terminal_saved(
    session: &mut Session,
    reason: &str,
    usage: ClassificationUsage,
    runtime: Runtime<'_>,
    context: &ExecutionContext,
) -> Outcome {
    session.active.clear();
    let outcome = terminal(session, reason, usage);
    if runtime.can_resume {
        session.batches.push(outcome.output.clone());
        let key = runtime.sessions.key(&runtime.access, &session.id);
        if !runtime.sessions.save(key.clone(), session.clone(), context) {
            runtime.sessions.remove(&key, context);
        }
    }
    outcome
}

pub(super) fn terminal(session: &mut Session, reason: &str, usage: ClassificationUsage) -> Outcome {
    for resource in &mut session.resources {
        for unit in &mut resource.units {
            for cell in &mut unit.cells {
                if cell.is_none() {
                    *cell = Some(json!({"status":"unassessed", "reason":reason}));
                }
            }
        }
    }
    let output = emit(session, &usage, Some(reason));
    Outcome { output }
}

pub(super) fn emit(
    session: &mut Session,
    usage: &ClassificationUsage,
    terminal: Option<&str>,
) -> Value {
    let mut resources = serde_json::Map::new();
    for resource in &mut session.resources {
        let mut metadata = resource.metadata.clone();
        let mut sections = Vec::new();
        let mut completed = 0;
        let mut counts = [0usize; 4];
        for unit in &mut resource.units {
            let complete = unit.cells.iter().all(Option::is_some);
            completed += usize::from(complete);
            for cell in &unit.cells {
                let slot = match cell.as_ref().and_then(|cell| cell["status"].as_str()) {
                    Some("answered") => 0,
                    Some("error") => 1,
                    Some("unsupported") => 2,
                    _ => 3,
                };
                counts[slot] += 1;
            }
            if complete && !unit.emitted {
                let mut section = unit.metadata.clone();
                section["answers"] = Value::Object(
                    session
                        .keys
                        .iter()
                        .cloned()
                        .zip(unit.cells.iter().filter_map(Clone::clone))
                        .collect(),
                );
                sections.push(section);
                unit.emitted = true;
            }
        }
        if metadata["status"] == "ready" {
            // An absent count is 0 (the output contract says so).
            let mut coverage = json!({"totalSections":resource.units.len(),"answered":counts[0]});
            for (name, count) in [
                ("remainingSections", resource.units.len() - completed),
                ("error", counts[1]),
                ("unsupported", counts[2]),
                ("unassessed", counts[3]),
            ] {
                if count > 0 {
                    coverage[name] = json!(count);
                }
            }
            metadata["coverage"] = coverage;
            // The reason explains this resource's unassessed cells; a resource
            // the terminal batch left untouched has none to explain.
            if let Some(reason) = terminal.filter(|_| counts[3] > 0) {
                metadata["coverage"]["terminalReason"] = json!(reason);
            }
        }
        if sections.is_empty() && resource.shown.as_ref() == Some(&metadata) {
            continue;
        }
        resource.shown = Some(metadata.clone());
        metadata["sections"] = json!(sections);
        resources.insert(resource.key.clone(), metadata);
    }
    // Score cells carry no legend, so the shared questions are the only one:
    // every batch carries them, so a page read through a cursor is
    // self-contained. The response layer drops them from the first page of a
    // fresh request, whose caller stated them.
    let mut output =
        json!({"questions":session.questions,"resources":resources,"usage":usage.output()});
    if let Some(model) = &session.resolved_model {
        output["model"] = json!(model);
    }
    output
}

pub(super) fn answered_cell(mut answer: Value) -> Value {
    if let Some(fields) = answer.as_object_mut() {
        if fields.get("type").and_then(Value::as_str) == Some("score") {
            fields.remove("legend");
        }
        fields.remove("type");
    }
    json!({"status":"answered", "answer":answer})
}
