//! Bounded assessment over a frozen resource/section/question plan.
use super::Row;
use crate::execution::ExecutionContext;
use crate::tools::judgment::ClassificationError;
use crate::tools::judgment::stats::ClassificationUsage;
use crate::tools::source_capture::SourceReader;
use octocode_contract_types::ErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod assess;
mod budget;
mod capture;
mod emit;
mod store;

use assess::{assess_and_save, pending_cost, settle_unsupported};
pub(super) use budget::Budget;
use emit::terminal;
pub(crate) use store::Sessions;

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
}

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    context: Option<String>,
    questions: Value,
    keys: Vec<String>,
    model: Option<String>,
    resolved_model: Option<String>,
    long_context: bool,
    resources: Vec<Resource>,
    github_identity: Option<String>,
    batches: Vec<Value>,
    active: Vec<[usize; 2]>,
}

pub(super) struct Outcome {
    pub output: Value,
    pub usage: ClassificationUsage,
}

fn unavailable(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextUnavailable,
        message,
        "Use persistent storage across CLI calls or a long-lived MCP runtime; start a fresh sections request explicitly if the saved plan expired.",
    )
}

/// A continuation token that names no next batch: the caller changed it.
fn bad_token(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextUnavailable,
        message,
        "Copy nextSectionPage unchanged, or start a fresh sections request.",
    )
}

fn token(id: &str, batch: usize) -> String {
    format!("sections-v1:{id}:{batch:x}")
}

fn decode(value: &str) -> Option<(&str, usize)> {
    let (id, batch) = value.strip_prefix("sections-v1:")?.split_once(':')?;
    if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((id, usize::from_str_radix(batch, 16).ok()?))
}

/// An error as a section or resource reports it: its hints, then the failed
/// read's own leads by name, as the hint lines a failed row publishes.
fn error_value(error: &ClassificationError) -> Value {
    let mut value = json!({"errorCode": error.code, "error": error.message});
    if !error.hints.is_empty() {
        value["hints"] = json!(error.hints);
    }
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
    let (mut session, ordinal) = if let Some(assessment) = input["assessment"].as_str() {
        let (id, ordinal) = decode(assessment).ok_or_else(|| {
            bad_token("The assessment token is malformed; no source or provider was called.")
        })?;
        let key = runtime.sessions.key(&runtime.access, id);
        let ticket = runtime.sessions.flights.join(key.resource.clone());
        let wait_budget = evaluator.budget(context.deadline, context.cancellation.clone());
        let _turn = dispatcher
            .handle()
            .block_on(wait_budget.wait(ticket.turn()))
            .map_err(|_| unavailable("The call stopped while waiting for the assessment transaction; nothing was judged."))?;
        let _process_lock = runtime.sessions.transaction_lock(&key, context)?;
        let mut session = runtime.sessions.current(&key, context)?.ok_or_else(|| unavailable("The frozen assessment plan is missing, expired, or belongs to another access context; no source or provider was called."))?;
        dispatcher
            .validate_retained_github_identity(session.github_identity.as_deref(), context)?;
        if let Some(saved) = session.batches.get(ordinal) {
            let mut output = saved.clone();
            output["usage"] = serde_json::to_value(ClassificationUsage::default().output())
                .unwrap_or(Value::Null);
            output["replayed"] = json!(true);
            return Ok(Outcome {
                output,
                usage: ClassificationUsage::default(),
            });
        }
        if ordinal != session.batches.len() {
            return Err(bad_token(
                "The assessment token does not name the next frozen batch; nothing was judged.",
            ));
        }
        if !session.active.is_empty() {
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
            let outcome = terminal(
                &mut session,
                "An interrupted assessment has unknown provider outcomes; remaining work was not attempted.",
                ClassificationUsage::default(),
            );
            session.batches.push(outcome.output.clone());
            if !runtime.sessions.save(key.clone(), session, context) {
                runtime.sessions.remove(&key, context);
            }
            return Ok(outcome);
        }
        validate_sources(&session, dispatcher, context)?;
        return assess_and_save(&mut session, ordinal, dispatcher, context, runtime, shared);
    } else {
        (
            capture::create(
                row,
                input,
                dispatcher,
                context,
                runtime.sessions.max_bytes,
                &shared.captures,
            )?,
            0,
        )
    };
    session.github_identity = dispatcher.retained_github_identity(context)?;
    settle_unsupported(&mut session, shared.limits);
    let pending = pending_cost(&session);
    if !runtime.can_resume
        && (pending.0 > shared.limits.max_candidates || pending.1 > shared.limits.max_chars)
    {
        let reason = "The complete plan requires more than one bounded assessment batch. No inference ran because this runtime ends after the call. Use persistent storage or a long-lived MCP runtime.";
        let mut outcome = terminal(&mut session, reason, ClassificationUsage::default());
        let error = unavailable(reason);
        outcome.output["errorCode"] = json!(error.code);
        outcome.output["error"] = json!(error.message);
        outcome.output["hints"] = json!(error.hints);
        return Ok(outcome);
    }
    // The batch saves the whole frozen plan before any provider request
    // (and after it), so a fresh plan needs no save of its own.
    let key = runtime.sessions.key(&runtime.access, &session.id);
    let _process_lock = runtime.sessions.transaction_lock(&key, context)?;
    assess_and_save(&mut session, ordinal, dispatcher, context, runtime, shared)
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
