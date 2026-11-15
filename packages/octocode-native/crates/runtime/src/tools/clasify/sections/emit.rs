//! A batch's output: completed sections emitted once, coverage, and the
//! terminal outcome that settles every remaining cell. A resource row is
//! repeated on a later batch only when it gains sections or its coverage or
//! status changed: unchanged rows are duplicate metadata, not evidence.
use super::{Outcome, Resource, Runtime, Session};
use crate::execution::ExecutionContext;
use crate::response::result::{Continuation, insert_next};
use crate::tools::judgment::stats::ClassificationUsage;
use octocode_contract_types::{
    ClasifyChannel, ClasifyQuery, ClasifyQuerySectionsAssessment,
    ClasifyQuerySectionsAssessmentOperation,
};
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
        attach_continuations(&mut outcome.output, session, true);
        let _ = session.push_batch(&outcome.output);
        let key = runtime.sessions.key(&runtime.access, &session.id);
        if !runtime.sessions.save(key.clone(), session.clone(), context) {
            runtime.sessions.remove(&key, context);
            // No plan is left for next.quiet to read: show every section any
            // batch held back, earlier kept batches' too.
            restore_all_quiet(&mut outcome.output, session);
        }
    }
    outcome
}

/// The sections a batch held back as quiet, by resource: what
/// [`Held::restore`] shows again when the plan that `next.quiet` reads is not
/// kept.
#[derive(Default)]
pub(super) struct Held(Vec<(String, Vec<Value>)>);

impl Held {
    pub(super) fn restore(self, output: &mut Value) {
        for (key, quiet) in self.0 {
            let row = &mut output["resources"][&key];
            if let Some(sections) = row["sections"].as_array_mut() {
                sections.extend(quiet);
            }
            if let Some(coverage) = row["coverage"].as_object_mut() {
                coverage.remove("quiet");
            }
        }
        if let Some(next) = output["next"].as_object_mut() {
            next.remove(ClasifyChannel::Quiet.key());
        }
    }
}

/// Every quiet section of the plan, back on `output`, when no kept plan is
/// left for `next.quiet`: held sections only ever reach the caller through
/// it, so none was shown before and each is shown once.
fn restore_all_quiet(output: &mut Value, session: &Session) {
    if let Some(next) = output["next"].as_object_mut() {
        next.remove(ClasifyChannel::Quiet.key());
    }
    for resource in &session.resources {
        let quiet: Vec<Value> = quiet_units(session, resource)
            .map(|unit| section(unit, &session.keys))
            .collect();
        if quiet.is_empty() {
            continue;
        }
        if output["resources"].get(&resource.key).is_none() {
            let Some(mut row) = resource.shown.clone() else {
                continue;
            };
            row["sections"] = json!([]);
            output["resources"][&resource.key] = row;
        }
        let row = &mut output["resources"][&resource.key];
        if let Some(coverage) = row["coverage"].as_object_mut() {
            coverage.remove("quiet");
        }
        if let Some(sections) = row["sections"].as_array_mut() {
            sections.extend(quiet);
        }
    }
}

/// The continuations of a batch a later call can run (the plan is kept):
/// the hits view, then reads of the yes-sections. Quiet sections leave the
/// page, ranked by highest `noul` first, and are counted in
/// `coverage.quiet`; the `last` batch offers `next.quiet`, which lists every
/// one, so each section reaches the caller exactly once.
pub(super) fn attach_continuations(output: &mut Value, session: &Session, last: bool) -> Held {
    // Reads first, over every section this batch emitted: `hit_reads` skips a
    // resource whose sections all said yes, which only the full list shows.
    attach_hit_reads(output, session);
    let held = hold_back_quiet(output, session);
    let quiet = session
        .resources
        .iter()
        .any(|resource| quiet_units(session, resource).next().is_some());
    if last && quiet {
        let Ok(assessment) = super::quiet_token(&session.id).try_into() else {
            unreachable!("a quiet token is a valid assessment token");
        };
        let query = ClasifyQuery {
            subtype_0: None,
            subtype_1: None,
            subtype_2: None,
            subtype_3: Some(ClasifyQuerySectionsAssessment {
                assessment,
                main_goal: None,
                operation: ClasifyQuerySectionsAssessmentOperation::Sections,
                reasoning: None,
                view: None,
            }),
        };
        insert_next(
            &mut output["next"],
            ClasifyChannel::Quiet,
            Continuation::continued_query(query),
        );
    }
    held
}

fn hold_back_quiet(output: &mut Value, session: &Session) -> Held {
    let mut held = Held::default();
    for resource in &session.resources {
        let Some(row) = output["resources"].get_mut(&resource.key) else {
            continue;
        };
        let count = quiet_units(session, resource).count();
        if count > 0 && row["coverage"].is_object() {
            row["coverage"]["quiet"] = json!(count);
        }
        let Some(sections) = row["sections"].as_array_mut() else {
            continue;
        };
        let (quiet, mut shown): (Vec<Value>, Vec<Value>) = std::mem::take(sections)
            .into_iter()
            .partition(|section| is_quiet(&session.questions, &section["answers"]));
        // Highest yes first; ties, and sections with no noul answer, keep plan order.
        shown.sort_by(|a, b| top_noul(b).total_cmp(&top_noul(a)));
        *sections = shown;
        if !quiet.is_empty() {
            held.0.push((resource.key.clone(), quiet));
        }
    }
    held
}

/// `next.quiet`: every quiet section emitted so far, from the kept plan, with
/// no provider call.
pub(super) fn quiet_replay(session: &Session) -> Value {
    let mut resources = serde_json::Map::new();
    for resource in &session.resources {
        let sections: Vec<Value> = quiet_units(session, resource)
            .map(|unit| section(unit, &session.keys))
            .collect();
        if sections.is_empty() {
            continue;
        }
        let Some(mut row) = resource.shown.clone() else {
            continue;
        };
        if row["coverage"].is_object() {
            row["coverage"]["quiet"] = json!(sections.len());
        }
        row["sections"] = json!(sections);
        resources.insert(resource.key.clone(), row);
    }
    if resources.is_empty()
        && let Some(resource) = session.resources.first()
        && let Some(shown) = &resource.shown
    {
        let mut row = shown.clone();
        row["sections"] = json!([]);
        resources.insert(resource.key.clone(), row);
    }
    let mut output = json!({
        "questions":session.questions,
        "resources":resources,
        "usage":ClassificationUsage::default().output(),
        "replayed":true,
    });
    if let Some(model) = &session.resolved_model {
        output["model"] = json!(model);
    }
    output
}

/// A resource's emitted sections that are quiet.
fn quiet_units<'a>(
    session: &'a Session,
    resource: &'a super::Resource,
) -> impl Iterator<Item = &'a super::Unit> {
    resource.units.iter().filter(|unit| {
        unit.emitted && is_quiet(&session.questions, &section(unit, &session.keys)["answers"])
    })
}

/// One completed unit as the output shows it: its metadata and every answer.
fn section(unit: &super::Unit, keys: &[String]) -> Value {
    let mut section = unit.metadata.clone();
    section["answers"] = Value::Object(
        keys.iter()
            .cloned()
            .zip(unit.cells.iter().filter_map(Clone::clone))
            .collect(),
    );
    section
}

/// Below this P(yes) a `noul` answer is a no; 0.3-0.7 is unsure, and shown.
const QUIET_BELOW: f64 = 0.3;

/// Whether a section's `answers` hold back: every question is `noul`, every
/// cell answered, every P(yes) below [`QUIET_BELOW`]. A failed, unsupported
/// or unassessed cell, or another question type, is always shown.
fn is_quiet(questions: &Value, answers: &Value) -> bool {
    answers.as_object().is_some_and(|answers| {
        !answers.is_empty()
            && answers.iter().all(|(key, cell)| {
                questions[key]["type"] == "noul"
                    && cell["status"] == "answered"
                    && cell["answer"]["noul"]
                        .as_f64()
                        .is_some_and(|p| p < QUIET_BELOW)
            })
    })
}

/// A shown section's rank: its highest `noul` answer, below every probability
/// when it has none.
fn top_noul(section: &Value) -> f64 {
    section["answers"]
        .as_object()
        .into_iter()
        .flat_map(|answers| answers.values())
        .filter_map(|cell| cell["answer"]["noul"].as_f64())
        .fold(-1.0, f64::max)
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
                sections.push(section(unit, &session.keys));
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
    let mut output = session
        .source_disclosures
        .clone()
        .unwrap_or_else(|| json!({}));
    output["questions"] = session.questions.clone();
    output["resources"] = json!(resources);
    output["usage"] = json!(usage.output());
    if let Some(model) = &session.resolved_model {
        output["model"] = json!(model);
    }
    if let Some(notice) = usage.long_context_notice() {
        crate::response::paging::push_warnings(&mut output, [notice]);
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
fn attach_hit_reads(output: &mut Value, session: &Session) {
    let pin = |key: &str| {
        session
            .resources
            .iter()
            .find(|resource| resource.key == key)
            .and_then(Resource::pin)
    };
    let reads = output["resources"]
        .as_object()
        .map(|resources| hit_reads(resources, pin))
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
/// resource's own `read` returns. A URL capture has no line read. `pin`
/// names the snapshot or commit each resource key was judged at.
fn hit_reads<'a>(
    resources: &serde_json::Map<String, Value>,
    pin: impl Fn(&str) -> Option<&'a str>,
) -> Vec<Value> {
    resources
        .iter()
        .filter(|(_, resource)| resource["status"] == "ready")
        .filter_map(|(key, resource)| {
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
                pin(key),
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
