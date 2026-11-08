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
    let mut outcome = terminal(session, reason, usage);
    if runtime.can_resume {
        attach_hit_reads(&mut outcome.output);
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
    // A batch a sibling row's admissions left nothing to add still answers
    // for its first resource: the row reads on with its next batch.
    if resources.is_empty()
        && let Some(resource) = session.resources.first()
        && let Some(shown) = &resource.shown
    {
        let mut metadata = shown.clone();
        metadata["sections"] = json!([]);
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
    if let Some(notice) = usage.long_context_notice() {
        crate::response::result::add_hints(&mut output, [notice]);
    }
    output
}

/// Whether a section's answers say yes to some `noul` question: the sections
/// worth reading to confirm.
fn is_hit(section: &Value) -> bool {
    section["answers"].as_object().is_some_and(|answers| {
        answers.values().any(|cell| {
            cell["status"] == "answered"
                && cell["answer"]["noul"].as_f64().is_some_and(|p| p >= 0.5)
        })
    })
}

/// Adds `next.read`, `next.read2`, … to a batch that a later call can run: the
/// read is a saved continuation, which a command that exits after one call
/// cannot publish.
pub(super) fn attach_hit_reads(output: &mut Value) {
    let reads = output["resources"]
        .as_object()
        .map(hit_reads)
        .unwrap_or_default();
    for (index, read) in reads.into_iter().enumerate() {
        let name = if index == 0 {
            "read".to_owned()
        } else {
            format!("read{}", index + 1)
        };
        crate::response::result::set_next(output, &name, read);
    }
}

/// One read per resource of exactly the lines of its yes-sections in this
/// batch (adjacent or overlapping ones merged), pinned to the judged snapshot:
/// confirming an answer costs the lines it names, not the whole source the
/// resource's own `read` returns. A URL capture has no line read.
fn hit_reads(resources: &serde_json::Map<String, Value>) -> Vec<Value> {
    resources
        .values()
        .filter(|resource| resource["status"] == "ready")
        .filter_map(|resource| {
            let tool = resource["source"]["tool"]
                .as_str()
                .and_then(crate::id::ToolId::from_name)
                .filter(|tool| {
                    matches!(
                        tool,
                        crate::id::ToolId::LocalFetch | crate::id::ToolId::GhGetFileContent
                    )
                })?;
            let all = resource["sections"].as_array()?;
            // Every section of a fully judged resource said yes: its own
            // `read` is already exactly this read.
            if resource["coverage"]["remainingSections"]
                .as_u64()
                .unwrap_or(0)
                == 0
                && all.iter().all(is_hit)
            {
                return None;
            }
            let mut lines = all
                .iter()
                .filter(|section| is_hit(section))
                .filter_map(|section| {
                    let target = &section["target"]["lines"];
                    Some((target["startLine"].as_u64()?, target["endLine"].as_u64()?))
                })
                .collect::<Vec<_>>();
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
            let mut query = resource["source"]["query"].clone();
            query["ranges"] = merged
                .into_iter()
                .map(|(start, end)| json!({"startLine":start,"endLine":end}))
                .collect();
            Some(crate::tools::source_capture::read_lead(
                tool,
                &query,
                resource["snapshot"].as_str(),
            ))
        })
        .collect()
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
