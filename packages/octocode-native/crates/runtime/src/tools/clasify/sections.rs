//! Bounded assessment over a frozen resource/section/question plan.
use super::Row;
use crate::execution::ExecutionContext;
use crate::tools::judgment::stats::ClassificationUsage;
use crate::tools::judgment::{ClassificationError, LongContext};
use crate::tools::source_capture::SourceReader;
use octocode_contract_types::ErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod assess;
mod budget;
mod capture;
mod emit;
mod store;

use assess::assess_and_save;
pub(super) use budget::Budget;
use emit::terminal_saved;
#[cfg(test)]
pub(crate) use store::PLAN_RETENTION;
pub(crate) use store::{PLAN_IDLE, Sessions};

#[derive(Clone, Copy)]
pub(crate) struct Runtime<'a> {
    pub(crate) sessions: &'a Sessions,
    pub access: [u8; 32],
    pub(crate) can_resume: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Unit {
    metadata: Value,
    state: Value,
    /// The state's serialized Unicode scalars, counted once at capture.
    chars: usize,
    cells: Vec<Option<Value>>,
    emitted: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Resource {
    key: String,
    metadata: Value,
    units: Vec<Unit>,
    local_check: Option<Value>,
    /// The resource row (without sections) last emitted. A later batch
    /// repeats the resource only when it carries new sections or this row
    /// changed, so continuation pages never resend unchanged metadata.
    #[serde(default)]
    shown: Option<Value>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    context: Option<String>,
    questions: Value,
    keys: Vec<String>,
    model: Option<String>,
    resolved_model: Option<String>,
    /// The row's `longContext` ([`LongContext::from_field`]), so every
    /// batch asks for the long window as the caller chose. A plan saved by
    /// another build is never read: its key's scope holds the build identity.
    long_context: LongContext,
    resources: Vec<Resource>,
    github_identity: Option<String>,
    batches: Vec<Value>,
    active: Vec<[usize; 2]>,
}

pub(super) struct Outcome {
    pub output: Value,
}

fn unavailable(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextUnavailable,
        message,
        "Section plans across CLI calls need storage.mode persistent, or one running MCP server; start a fresh sections request explicitly if the saved plan expired.",
    )
}

/// Refresh each plan an `assessment` token in `outputs` continues: every
/// read of a plan restarts its idle lifetime ([`PLAN_IDLE`]). A plan past it
/// stays expired.
pub(crate) fn refresh_continued<'a>(
    sessions: &Sessions,
    access: &[u8; 32],
    outputs: impl IntoIterator<Item = &'a Value>,
) {
    let mut ids = std::collections::BTreeSet::new();
    for output in outputs {
        continued_plans(output, &mut ids);
    }
    for id in ids {
        sessions.refresh(&sessions.key(access, id));
    }
}

/// The plan id of every `assessment` token in `value`.
fn continued_plans<'a>(value: &'a Value, ids: &mut std::collections::BTreeSet<&'a str>) {
    match value {
        Value::Object(fields) => {
            for (field, value) in fields {
                match value.as_str().and_then(decode) {
                    Some((id, _)) if field == "assessment" => {
                        ids.insert(id);
                    }
                    _ => continued_plans(value, ids),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                continued_plans(item, ids);
            }
        }
        _ => {}
    }
}

/// A continuation token that names no next batch: the caller changed it.
fn bad_token(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextUnavailable,
        message,
        "Copy nextPage unchanged, or start a fresh sections request.",
    )
}

fn token(id: &str, batch: usize) -> String {
    format!("sections-v1:{id}:{batch:x}")
}

/// `next.quiet`: the plan's quiet sections, replayed ([`emit::quiet_replay`]).
fn quiet_token(id: &str) -> String {
    format!("sections-v1:{id}:{QUIET}")
}

const QUIET: &str = "quiet";

/// A token's plan id and its batch ordinal; `None` for the quiet replay.
fn decode(value: &str) -> Option<(&str, Option<usize>)> {
    let (id, batch) = value.strip_prefix("sections-v1:")?.split_once(':')?;
    if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if batch == QUIET {
        return Some((id, None));
    }
    Some((id, Some(usize::from_str_radix(batch, 16).ok()?)))
}

/// An error as a section or resource reports it: its hints, then the failed
/// read's own leads by name, as the hint lines a failed row publishes.
fn error_value(error: &ClassificationError) -> Value {
    let mut value = json!({"errorCode": error.code, "error": error.message});
    crate::response::result::add_hints(&mut value, error.hints.clone());
    crate::response::result::attach_calls(
        &mut value,
        Value::Object(error.leads.iter().cloned().collect()),
    );
    value
}

pub(super) fn run(
    row: &Row,
    input: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    runtime: Runtime<'_>,
    shared: &Budget,
) -> Result<Outcome, ClassificationError> {
    let evaluator = dispatcher.evaluator();
    let (mut session, notices, related) = if let Some(assessment) = input["assessment"].as_str() {
        let (id, ordinal) = decode(assessment).ok_or_else(|| {
            bad_token("The assessment token is malformed; no source or provider was called.")
        })?;
        let key = runtime.sessions.key(&runtime.access, id);
        let ticket = runtime.sessions.flights.join(key.resource.clone());
        let wait_budget = evaluator.budget(
            crate::execution::cancel::deadline_of(context),
            context.cancellation.clone(),
        );
        let _turn = dispatcher
            .handle()
            .block_on(wait_budget.wait(ticket.turn()))
            .map_err(|_| unavailable("The call stopped while waiting for the assessment transaction; nothing was judged."))?;
        let _process_lock = runtime.sessions.transaction_lock(&key, context)?;
        let mut session = runtime.sessions.current(&key, context)?.ok_or_else(|| unavailable("The frozen assessment plan is missing, expired, or belongs to another access context; no source or provider was called."))?;
        dispatcher
            .validate_retained_github_identity(session.github_identity.as_deref(), context)?;
        let Some(ordinal) = ordinal else {
            return Ok(Outcome {
                output: emit::quiet_replay(&session),
            });
        };
        if let Some(saved) = session.batches.get(ordinal) {
            let mut output = saved.clone();
            output["usage"] = serde_json::to_value(ClassificationUsage::default().output())
                .unwrap_or(Value::Null);
            // A replay bills nothing: the batch's retry notice went with
            // its usage.
            if let Some(output) = output.as_object_mut() {
                output.remove("hints");
            }
            output["replayed"] = json!(true);
            return Ok(Outcome { output });
        }
        if ordinal != session.batches.len() {
            return Err(bad_token(
                "The assessment token does not name the next frozen batch; nothing was judged.",
            ));
        }
        if !session.active.is_empty() {
            return Ok(interrupted(&mut session, runtime, context));
        }
        if let Some(withheld) = withheld_saved_source(&session, dispatcher.read_paths()) {
            runtime.sessions.remove(&key, context);
            return Err(withheld);
        }
        validate_sources(&session, dispatcher, context)?;
        return assess_and_save(&mut session, ordinal, dispatcher, context, runtime, shared);
    } else {
        capture::create(
            row,
            input,
            dispatcher,
            context,
            octocode_engine::memory::MEMORY.limit(),
            &shared.captures,
        )?
    };
    session.github_identity = dispatcher.retained_github_identity(context)?;
    // The batch saves the whole frozen plan before any provider request
    // (and after it), so a fresh plan needs no save of its own.
    let key = runtime.sessions.key(&runtime.access, &session.id);
    let _process_lock = runtime.sessions.transaction_lock(&key, context)?;
    let mut outcome = assess_and_save(&mut session, 0, dispatcher, context, runtime, shared)?;
    // What a glob's walk withheld, said on the first batch.
    crate::response::result::add_hints(&mut outcome.output, notices);
    for call in related {
        crate::response::result::attach_related(&mut outcome.output, call);
    }
    Ok(outcome)
}

/// The batch a continuation ends with when the previous process stopped
/// mid-batch: the cells it had in flight have unknown provider outcomes, so
/// they are errors, never retried; the rest is settled like any terminal
/// batch ([`terminal_saved`]), so the sections earlier batches held back
/// stay reachable through `next.quiet`, or are shown here when no plan is
/// kept.
fn interrupted(session: &mut Session, runtime: Runtime<'_>, context: &ExecutionContext) -> Outcome {
    let error = error_value(&unavailable(
        "A previous process stopped after it could have submitted this assessment batch. Its provider outcome is unknown; this continuation did not retry it.",
    ));
    for [resource, unit] in std::mem::take(&mut session.active) {
        if let Some(unit) = session
            .resources
            .get_mut(resource)
            .and_then(|resource| resource.units.get_mut(unit))
        {
            for cell in &mut unit.cells {
                if cell.is_none() {
                    *cell = Some(json!({"status":"error","error":error}));
                }
            }
        }
    }
    terminal_saved(
        session,
        "An interrupted assessment has unknown provider outcomes; remaining work was not attempted.",
        ClassificationUsage::default(),
        runtime,
        context,
    )
}

fn validate_sources(
    session: &Session,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<(), ClassificationError> {
    for resource in &session.resources {
        if let Some(source) = &resource.local_check {
            capture::validate_local(source, dispatcher, context)?;
        }
    }
    Ok(())
}

/// A saved plan outlives the binary that made it, and holds its sections' text:
/// one that holds a file the egress now withholds by name sends nothing more.
/// A resource refused when planned holds no text, so it stops nothing.
fn withheld_saved_source(
    session: &Session,
    reads: &crate::policy::path::PathPolicy,
) -> Option<ClassificationError> {
    session
        .resources
        .iter()
        .filter(|resource| !resource.units.is_empty())
        .map(|resource| &resource.metadata["source"])
        .filter(|source| source["tool"] == crate::id::ToolId::GhGetFileContent.as_str())
        .find_map(|source| {
            let withheld =
                crate::tools::source_capture::withheld_github_source(&source["query"], reads)?;
            Some(ClassificationError {
                message: format!(
                    "The saved plan holds sections of '{}', which classification withholds by its name: no further section was sent, and the plan was dropped.",
                    source["query"]["path"].as_str().unwrap_or_default()
                ),
                ..withheld
            })
        })
}

fn new_id(input: &Value) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = crate::civil_date::unix_nanos();
    crate::digest::json_sha256(
        &json!({"input":input,"time":now.to_string(),"process":std::process::id(),"serial":serial}),
    )
}

#[cfg(test)]
mod tests;
