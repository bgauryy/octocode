//! Bounded assessment over a frozen resource/section/question plan.
use super::Row;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::providers::classification::gate::GateLease;
use octocode_tools_shared::judgment::stats::ClassificationUsage;
use octocode_tools_shared::judgment::{ClassificationError, LongContext, RequestCap};
use octocode_tools_shared::source_capture::SourceReader;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod assess;
mod budget;
mod capture;
mod emit;
pub mod store;

use assess::assess_and_save;
pub(super) use budget::Budget;
use emit::terminal_saved;
#[cfg(any(test, feature = "test-support"))]
pub use store::PLAN_RETENTION;
pub use store::{PLAN_IDLE, Sessions};

#[derive(Clone, Copy)]
pub struct Runtime<'a> {
    pub sessions: &'a Sessions,
    pub access: [u8; 32],
    pub can_resume: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Unit {
    /// What the output shows of the section: its id, kind, name, and its
    /// target and context located in the text a read shows of the source.
    metadata: Value,
    /// The target's bytes `[start, end)` in the captured source text, where
    /// a plan is cut: planning state, never shown (a hidden value's length
    /// would show in it).
    #[serde(default)]
    source_target: [usize; 2],
    state: Value,
    /// The state's serialized Unicode scalars, counted once at capture.
    chars: usize,
    cells: Vec<Option<Value>>,
    emitted: bool,
    /// The batch ordinal that emitted the unit: the batch whose `next.quiet`
    /// lists it when it is quiet.
    #[serde(default)]
    emitted_in: usize,
    /// The questions a transient failure left pending once: the next batch
    /// asks them again, and a second such failure is their error.
    #[serde(default)]
    deferred: std::collections::BTreeSet<usize>,
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

impl Resource {
    /// What a read of this resource is pinned to: a local source's snapshot
    /// (kept for the staleness check, never shown), else the commit or
    /// capture digest its row shows.
    fn pin(&self) -> Option<&str> {
        self.local_check
            .as_ref()
            .map_or(
                &self.metadata["sourceDigest"],
                |check| &check["query"]["snapshot"],
            )
            .as_str()
    }
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
    /// The request the plan was made from, without private progress or the
    /// caller's brief ([`octocode_runtime_base::response::result::fresh_query_row`]): the
    /// restart a cursor of the plan offers once its source changed.
    #[serde(default)]
    request: Value,
    /// Source expansion facts retained before inference, then moved into the
    /// first saved batch so an interrupted first assessment cannot lose them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_disclosures: Option<Value>,
    /// The row's `maxRequests`: every batch takes the sections whose first
    /// round fits it, and is held to it ([`assess`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_requests: Option<u64>,
}

impl Session {
    fn push_batch(&mut self, output: &Value) -> Option<Value> {
        let disclosures = self.source_disclosures.take();
        self.batches.push(output.clone());
        disclosures
    }
}

pub struct Outcome {
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
pub fn refresh_continued<'a>(
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

/// `next.quiet` of batch `batch`: the quiet sections it held back, replayed
/// ([`emit::quiet_replay`]).
fn quiet_token(id: &str, batch: usize) -> String {
    format!("sections-v1:{id}:{QUIET}{batch:x}")
}

/// Marks a quiet replay's batch ordinal: not a hex digit.
const QUIET: &str = "q";

/// What a token names: a batch, or a batch's quiet replay.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Named {
    Batch(usize),
    Quiet(usize),
}

/// A token's plan id and what it names.
fn decode(value: &str) -> Option<(&str, Named)> {
    let (id, batch) = value.strip_prefix("sections-v1:")?.split_once(':')?;
    if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let ordinal = |digits: &str| {
        (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| usize::from_str_radix(digits, 16).ok())
            .flatten()
    };
    if let Some(quiet) = batch.strip_prefix(QUIET) {
        return Some((id, Named::Quiet(ordinal(quiet)?)));
    }
    Some((id, Named::Batch(ordinal(batch)?)))
}

/// An error as a section or resource reports it: its hints, then the failed
/// read's own leads by name, as the hint lines a failed row publishes.
fn error_value(error: &ClassificationError) -> Value {
    let mut value = json!({"errorCode": error.code, "error": error.message});
    octocode_runtime_base::response::result::add_hints(&mut value, error.hints.clone());
    octocode_runtime_base::response::result::attach_calls(
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
    gate: &GateLease,
) -> Result<Outcome, ClassificationError> {
    let evaluator = dispatcher.evaluator();
    let (mut session, notices, related) = if let Some(assessment) = input["assessment"].as_str() {
        let (id, ordinal) = decode(assessment).ok_or_else(|| {
            bad_token("The assessment token is malformed; no source or provider was called.")
        })?;
        let key = runtime.sessions.key(&runtime.access, id);
        let ticket = runtime.sessions.flights.join(key.resource.clone());
        let wait_budget = evaluator.budget(
            octocode_runtime_base::execution::cancel::deadline_of(context),
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
        // Every cursor of the plan, a replay included, answers only from the
        // text it judged.
        current_sources(&session, dispatcher, context)?;
        let ordinal = match ordinal {
            Named::Quiet(batch) => {
                return Ok(Outcome {
                    output: emit::quiet_replay(&session, batch),
                });
            }
            Named::Batch(ordinal) => ordinal,
        };
        if let Some(saved) = session.batches.get(ordinal) {
            return Ok(Outcome {
                output: replayed(saved),
            });
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
        return assess_and_save(
            &mut session,
            ordinal,
            dispatcher,
            context,
            runtime,
            shared,
            gate,
        );
    } else {
        capture::create(
            row,
            input,
            dispatcher,
            context,
            octocode_engine::memory::MEMORY.limit(),
            &shared.captures,
            capture::Grain::Plan,
        )?
    };
    let mut disclosures = json!({});
    octocode_runtime_base::response::paging::push_warnings(&mut disclosures, notices);
    for call in related {
        octocode_runtime_base::response::result::attach_related(&mut disclosures, call);
    }
    if disclosures
        .as_object()
        .is_some_and(|fields| !fields.is_empty())
    {
        session.source_disclosures = Some(disclosures);
    }
    // Without maxRequests a plan sends one request per resource it names;
    // sections past that are a price nobody accepted, so nothing is sent.
    let named = input["resources"]
        .as_object()
        .map_or(0, serde_json::Map::len);
    if row.max_requests.is_none() {
        let plan = assess::plan_of(&session, &assess::pending(&session), dispatcher);
        let named = u64::try_from(named).unwrap_or(u64::MAX);
        if let Err(over) = super::admission::admit(None, plan, named, evaluator.retry_room()) {
            return Ok(Outcome {
                output: super::reply::over_budget(row, &over, runtime.can_resume),
            });
        }
    }
    session.max_requests = row.max_requests;
    session.github_identity = dispatcher.retained_github_identity(context)?;
    session.request = octocode_runtime_base::response::result::fresh_query_row(
        octocode_runtime_base::id::ToolId::Clasify,
        input,
    );
    // The batch saves the whole frozen plan before any provider request
    // (and after it), so a fresh plan needs no save of its own.
    let key = runtime.sessions.key(&runtime.access, &session.id);
    let _process_lock = runtime.sessions.transaction_lock(&key, context)?;
    assess_and_save(&mut session, 0, dispatcher, context, runtime, shared, gate)
}

/// Replaying a saved evaluation spends nothing and keeps its evidence,
/// including warnings about the original evaluation and source coverage.
/// Its usage is the page's own, original spend, marked `replayed`: this read
/// billed nothing, and the tokens the page cost are reported once more here,
/// where a `details` reader looks for them.
pub(super) fn replayed(saved: &Value) -> Value {
    let mut output = saved.clone();
    if let Some(usage) = output.get_mut("usage").and_then(Value::as_object_mut) {
        usage.insert("replayed".into(), json!(true));
    }
    if let Some(output) = output.as_object_mut() {
        output.remove("hints");
    }
    output["replayed"] = json!(true);
    output
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

/// The one check before any cursor of a plan answers (its next batch, a
/// saved batch's replay, `next.details`, `next.quiet`): every local source
/// still holds the text the plan judged. One that changed is
/// `staleSnapshot`, whose `next.restart` is the plan's own request.
fn current_sources(
    session: &Session,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<(), ClassificationError> {
    for resource in &session.resources {
        if let Some(source) = &resource.local_check {
            capture::validate_local(source, dispatcher, context).map_err(|mut error| {
                if error.code == ErrorCode::StaleSnapshot && session.request.is_object() {
                    let restart = octocode_runtime_base::response::result::Continuation::continued(
                        octocode_runtime_base::id::ToolId::Clasify,
                        session.request.clone(),
                    );
                    error.leads = vec![("restart".into(), restart.build())];
                }
                error
            })?;
        }
    }
    Ok(())
}

/// A saved plan outlives the binary that made it, and holds its sections' text:
/// one that holds a file the egress now withholds by name sends nothing more.
/// A resource refused when planned holds no text, so it stops nothing.
fn withheld_saved_source(
    session: &Session,
    reads: &octocode_runtime_base::policy::path::PathPolicy,
) -> Option<ClassificationError> {
    session
        .resources
        .iter()
        .filter(|resource| !resource.units.is_empty())
        .map(|resource| &resource.metadata["source"])
        .filter(|source| source["tool"] == octocode_runtime_base::id::ToolId::GhGetFileContent.as_str())
        .find_map(|source| {
            let withheld =
                octocode_tools_shared::source_capture::withheld_github_source(&source["query"], reads)?;
            Some(ClassificationError {
                message: format!(
                    "The saved plan holds sections of '{}', which classification withholds by its name: no further section was sent, and the plan was dropped.",
                    source["query"]["path"].as_str().unwrap_or_default()
                ),
                ..withheld
            })
        })
}

/// One judged section of a source judged whole: its target lines and each
/// question's outcome, in question order (`None`: not assessed).
pub(super) struct JudgedSection {
    /// Where the section sits in the source: its lines and bytes.
    pub target: Value,
    pub cells: Vec<Option<Value>>,
}

impl JudgedSection {
    /// The section's target lines.
    pub(super) fn lines(&self) -> &Value {
        &self.target["lines"]
    }
}

/// A source judged whole in sections within one call.
pub(super) struct Whole {
    /// The resource row a `sections` call shows for it: status, sourceDigest.
    pub metadata: Value,
    /// What a read of the judged source is pinned to ([`Resource::pin`]).
    pub pin: Option<String>,
    pub sections: Vec<JudgedSection>,
    pub usage: ClassificationUsage,
    pub model: Option<String>,
}

/// A source judged whole that failed after some of its requests were
/// billed: what those requests cost, and, when its sections were not
/// admitted, how many it planned (none of them was sent).
pub(super) struct WholeFailed {
    pub usage: ClassificationUsage,
    pub unadmitted: Option<u64>,
}

/// Judge one `source` whole in sections: each section alone, with every
/// question, fills one request of the provider's window
/// ([`octocode_tools_shared::judgment::Evaluator::request_fit`]), and every section is
/// judged concurrently under the call's gate and deadline. While no answer
/// here has reported what a token costs, the first section is judged alone
/// at one byte per token and the rest are planned at what it taught. A
/// section the window still refuses is planned again at most half its size,
/// until the window takes it or the questions alone would fill it. Nothing
/// is kept: the caller combines the sections into one answer per question.
/// The sections are the row's own expansion: they are judged only when
/// `admits` takes their count, every send counted against `cap`; otherwise
/// none is sent, and the failure names their count.
pub(super) fn whole(
    row: &Row,
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
    cap: Option<&RequestCap>,
    admits: &dyn Fn(u64) -> bool,
) -> Result<Whole, Box<WholeFailed>> {
    let mut usage = ClassificationUsage::default();
    let mut unadmitted = None;
    let bounds = Bounds {
        cap,
        admits,
        unadmitted: &mut unadmitted,
    };
    whole_billed(row, source, dispatcher, context, gate, bounds, &mut usage).map_err(|_| {
        Box::new(WholeFailed {
            usage: std::mem::take(&mut usage),
            unadmitted,
        })
    })
}

/// What bounds a source's sections ([`whole`]).
struct Bounds<'a> {
    cap: Option<&'a RequestCap>,
    admits: &'a dyn Fn(u64) -> bool,
    unadmitted: &'a mut Option<u64>,
}

/// [`whole`], adding every request it bills to `usage` even when it fails.
fn whole_billed(
    row: &Row,
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
    bounds: Bounds<'_>,
    usage: &mut ClassificationUsage,
) -> Result<Whole, ClassificationError> {
    let questions = row
        .keys
        .iter()
        .cloned()
        .zip(row.questions.iter().cloned())
        .collect::<serde_json::Map<_, _>>();
    let input = json!({"resources":{"source":source},"questions":questions});
    let long = LongContext::from_field(row.long_context) == LongContext::Always;
    let fit = || {
        dispatcher
            .evaluator()
            .request_fit(row.model.as_deref(), long)
    };
    let pool = capture::Pool::default();
    let plan = |bytes, span| {
        capture::create(
            row,
            &input,
            dispatcher,
            context,
            octocode_engine::memory::MEMORY.limit(),
            &pool,
            capture::Grain::Request { bytes, span },
        )
        .map(|(session, _, _)| session)
    };
    let first = fit();
    let mut session = plan(first.bytes, 0..usize::MAX)?;
    let Some((metadata, pin)) = session
        .resources
        .first()
        .map(|resource| (resource.metadata.clone(), resource.pin().map(str::to_owned)))
    else {
        return Err(unavailable("The source named no resource to judge."));
    };
    let sections = u64::try_from(session.resources[0].units.len()).unwrap_or(u64::MAX);
    if !(bounds.admits)(sections) {
        *bounds.unadmitted = Some(sections);
        return Err(unavailable(
            "The source's sections were not admitted; none was sent.",
        ));
    }
    let cap = bounds.cap;
    let mut judged = Vec::new();
    let mut planned_at = first.bytes;
    if !first.learned && session.resources[0].units.len() > 1 {
        judge_pending(&mut session, 1, dispatcher, context, gate, cap, usage);
        let next = fit();
        if next.bytes > first.bytes {
            let probe = session.resources[0].units.remove(0);
            let rest = target_bytes(&probe).end..usize::MAX;
            judged.push(probe);
            let resolved = session.resolved_model.take();
            session = plan(next.bytes, rest)?;
            session.resolved_model = resolved;
            planned_at = next.bytes;
        }
    }
    loop {
        judge_pending(
            &mut session,
            usize::MAX,
            dispatcher,
            context,
            gate,
            cap,
            usage,
        );
        let units = session
            .resources
            .first_mut()
            .map(|resource| std::mem::take(&mut resource.units))
            .unwrap_or_default();
        let (refused, kept): (Vec<Unit>, Vec<Unit>) = units.into_iter().partition(refused);
        judged.extend(kept);
        // A refused section was denser than what the fit learned: it is
        // split at most half the size it was sent at, so the refusals end.
        let next = fit().bytes.min(planned_at / 2);
        if refused.is_empty() {
            break;
        }
        let mut again = Vec::new();
        for unit in refused {
            match plan(next, target_bytes(&unit)) {
                Ok(replanned) => again.extend(
                    replanned
                        .resources
                        .into_iter()
                        .flat_map(|resource| resource.units),
                ),
                // Too small to hold the questions: the refusal stands.
                Err(_) => judged.push(unit),
            }
        }
        if again.is_empty() {
            break;
        }
        if let Some(resource) = session.resources.first_mut() {
            resource.units = again;
        }
        planned_at = next;
    }
    judged.sort_by_key(|unit| target_bytes(unit).start);
    Ok(Whole {
        metadata,
        pin,
        sections: judged
            .into_iter()
            .map(|unit| JudgedSection {
                target: unit.metadata["target"].clone(),
                cells: unit.cells,
            })
            .collect(),
        usage: std::mem::take(usage),
        model: session.resolved_model,
    })
}

/// The source bytes a unit's target covers.
fn target_bytes(unit: &Unit) -> std::ops::Range<usize> {
    let [start, end] = unit.source_target;
    start..end
}

/// Whether the provider's window refused a question of this unit.
fn refused(unit: &Unit) -> bool {
    unit.cells
        .iter()
        .flatten()
        .any(|cell| cell["error"]["errorCode"] == json!(ErrorCode::ClassificationStateTooLarge))
}

/// Judge up to `take` of the plan's units with a question still pending,
/// in plan order, concurrently, every send counted against `cap`.
fn judge_pending(
    session: &mut Session,
    take: usize,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
    cap: Option<&RequestCap>,
    usage: &mut ClassificationUsage,
) {
    let admitted: Vec<[usize; 2]> = session
        .resources
        .iter()
        .enumerate()
        .flat_map(|(resource, planned)| {
            planned
                .units
                .iter()
                .enumerate()
                .filter(|(_, unit)| unit.cells.iter().any(Option::is_none))
                .map(move |(unit, _)| [resource, unit])
        })
        .take(take)
        .collect();
    let pending: Vec<Vec<usize>> = admitted
        .iter()
        .map(|&[resource, unit]| {
            session.resources[resource].units[unit]
                .cells
                .iter()
                .enumerate()
                .filter_map(|(index, cell)| cell.is_none().then_some(index))
                .collect()
        })
        .collect();
    let inputs = assess::inputs(session, &admitted, &pending);
    let results = assess::judge_units(session, &inputs, dispatcher, context, gate, cap);
    assess::apply_results(
        session,
        &admitted,
        pending,
        results.into_iter(),
        usage,
        false,
    );
}

fn new_id(input: &Value) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = octocode_runtime_base::civil_date::unix_nanos();
    octocode_runtime_base::digest::json_sha256(
        &json!({"input":input,"time":now.to_string(),"process":std::process::id(),"serial":serial}),
    )
}

#[cfg(test)]
mod tests;
