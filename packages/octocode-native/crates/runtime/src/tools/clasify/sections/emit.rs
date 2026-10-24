//! A batch's output: completed sections emitted once, coverage, and the
//! terminal outcome that settles every remaining cell.
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
    Outcome { output, usage }
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
            metadata["coverage"] = json!({"totalSections":resource.units.len(),"completedSections":completed,"remainingSections":resource.units.len()-completed,"answered":counts[0],"error":counts[1],"unsupported":counts[2],"unassessed":counts[3]});
            if let Some(reason) = terminal {
                metadata["coverage"]["terminalReason"] = json!(reason);
            }
        }
        metadata["sections"] = json!(sections);
        resources.insert(resource.key.clone(), metadata);
    }
    let mut output = json!({"operation":"sections","questions":session.questions,"resources":resources,"usage":usage.output()});
    if let Some(model) = session.resolved_model.as_ref().or(session.model.as_ref()) {
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
