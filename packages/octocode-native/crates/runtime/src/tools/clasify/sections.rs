//! Bounded assessment over a frozen resource/section/question plan.
use super::{ClassificationError, EvaluationInput, Row};
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::execution::{ExecutionContext, SelectionLimits};
use crate::id::ToolId;
use crate::providers::classification::gate::call_cap;
use crate::response::result::Continuation;
use crate::tools::judgment::stats::ClassificationUsage;
use crate::tools::source_capture::SourceReader;
use futures_util::{StreamExt as _, stream};
use octocode_io::flight::KeyedFlight;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

mod capture;

/// Runtime-owned retention; policy/access identities are supplied by the same
/// owner as display snapshots, never reconstructed from environment here.
pub(crate) struct Sessions {
    store: Store<Session>,
    flights: KeyedFlight<String, ()>,
    policy: String,
    max_bytes: usize,
    disk: Option<PathBuf>,
}

impl Sessions {
    pub(crate) fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        let max_bytes = config.max_bytes;
        Self {
            store: Store::new(config, disk.clone()),
            flights: KeyedFlight::transient(),
            policy,
            max_bytes,
            disk,
        }
    }

    pub(crate) fn clear_memory(&self) {
        self.store.clear_memory();
    }

    fn process_lock(
        &self,
        name: &str,
        context: &ExecutionContext,
    ) -> Result<Option<octocode_io::lock::LockedFile>, ClassificationError> {
        let Some(disk) = &self.disk else {
            return Ok(None);
        };
        std::fs::create_dir_all(disk)
            .map_err(|_| unavailable("The assessment storage directory is unavailable."))?;
        loop {
            let mut options = std::fs::OpenOptions::new();
            options.create(true).truncate(false).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            let file = options
                .open(disk.join(name))
                .map_err(|_| unavailable("The assessment transaction lock is unavailable."))?;
            let wait = context
                .deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            match octocode_io::lock::lock_within(file, wait, Duration::from_millis(10)) {
                Ok(lock) => return Ok(Some(lock)),
                Err(octocode_io::lock::LockError::Busy) => {
                    context.check().map_err(|_| unavailable("The request stopped before the assessment transaction lock was acquired."))?;
                }
                Err(octocode_io::lock::LockError::Io(_)) => {
                    return Err(unavailable(
                        "The assessment transaction lock could not be acquired.",
                    ));
                }
            }
        }
    }

    fn key(&self, access: &[u8; 32], id: &str) -> CacheKey {
        CacheKey {
            namespace: "section-assessment-v2".into(),
            scope: format!("{}:{}", self.policy, hex::encode(access)),
            resource: id.into(),
        }
    }

    fn transaction_lock(
        &self,
        key: &CacheKey,
        context: &ExecutionContext,
    ) -> Result<Option<octocode_io::lock::LockedFile>, ClassificationError> {
        let digest = crate::digest::json_sha256(&json!([key.namespace, key.scope, key.resource]));
        // Stable lock files remain after expiry: unlinking one can split waiting
        // processes across different inodes and let both mutate the same plan.
        self.process_lock(&format!("assessment-{digest}.lock"), context)
    }

    fn current(
        &self,
        key: &CacheKey,
        context: &ExecutionContext,
    ) -> Result<Option<Session>, ClassificationError> {
        let _storage = self.process_lock("assessment.lock", context)?;
        Ok(self
            .store
            .get_current(key)
            .map(|saved| saved.value.as_ref().clone()))
    }

    fn remove(&self, key: &CacheKey, context: &ExecutionContext) {
        if let Ok(_storage) = self.process_lock("assessment.lock", context) {
            self.store.remove(key);
        }
    }

    fn save(&self, key: CacheKey, session: Session, context: &ExecutionContext) -> bool {
        let Ok(_storage) = self.process_lock("assessment.lock", context) else {
            return false;
        };
        let bytes = octocode_engine::json_size::bytes(&session).unwrap_or(usize::MAX);
        self.store
            .put_confirmed(
                key,
                session,
                bytes,
                CacheClass::Volatile {
                    ttl: crate::tools::judgment::retention(),
                },
            )
            .1
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Runtime<'a> {
    pub(crate) sessions: &'a Sessions,
    pub access: [u8; 32],
    pub(crate) can_resume: bool,
}

pub(super) struct Budget {
    limits: SelectionLimits,
    admission: Mutex<Admission>,
    captures: capture::Pool,
}

#[derive(Default)]
struct Admission {
    units: usize,
    chars: usize,
    deadline: Option<Instant>,
}

impl Budget {
    pub(super) fn new(limits: SelectionLimits) -> Self {
        Self {
            limits,
            admission: Mutex::new(Admission::default()),
            captures: capture::Pool::default(),
        }
    }

    fn deadline(&self, outer: Instant) -> Instant {
        let mut admission = self
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *admission
            .deadline
            .get_or_insert_with(|| outer.min(Instant::now() + self.limits.timeout))
    }

    fn has_admissions(&self) -> bool {
        self.admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .units
            > 0
    }

    fn admit(&self, chars: usize) -> bool {
        let mut admission = self
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if admission.units >= self.limits.max_candidates
            || admission.chars.saturating_add(chars) > self.limits.max_chars
            || admission
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return false;
        }
        admission.units += 1;
        admission.chars += chars;
        true
    }
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
    query_id: String,
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
        "classificationContextUnavailable",
        message,
        "Use persistent storage across CLI calls or a long-lived MCP runtime; start a fresh sections request explicitly if the saved plan expired.",
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

fn error_value(error: &ClassificationError) -> Value {
    let mut value = json!({"errorCode": error.code, "error": error.message});
    if !error.hints.is_empty() {
        value["hints"] = json!(error.hints);
    }
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
            unavailable("The assessment token is malformed; no source or provider was called.")
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
        session.query_id.clone_from(&row.id);
        dispatcher
            .validate_retained_github_identity(session.github_identity.as_deref(), context)?;
        if let Some(saved) = session.batches.get(ordinal) {
            let mut output = saved.clone();
            output["id"] = json!(row.id);
            output["usage"] = serde_json::to_value(ClassificationUsage::default().output())
                .unwrap_or(Value::Null);
            output["replayed"] = json!(true);
            return Ok(Outcome {
                output,
                usage: ClassificationUsage::default(),
            });
        }
        if ordinal != session.batches.len() {
            return Err(unavailable(
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
        outcome.output["error"] = error_value(&unavailable(reason));
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

fn pending_cost(session: &Session) -> (usize, usize) {
    session
        .resources
        .iter()
        .flat_map(|resource| &resource.units)
        .filter(|unit| unit.cells.iter().any(Option::is_none))
        .fold((0usize, 0usize), |(units, chars), unit| {
            (units + 1, chars.saturating_add(unit.chars))
        })
}

fn settle_unsupported(session: &mut Session, limits: SelectionLimits) -> usize {
    let mut settled = 0;
    for resource in &mut session.resources {
        for unit in &mut resource.units {
            if unit.chars > super::max_state_chars(session.long_context)
                || unit.chars > limits.max_chars
            {
                let error = error_value(&ClassificationError::new(
                    "classificationContextTooLarge",
                    "This exact target and its supporting context exceed the serialized state limit.",
                    "Use the pinned raw reader to inspect the complete target.",
                ));
                for cell in &mut unit.cells {
                    if cell.is_none() {
                        *cell = Some(json!({"status":"unsupported", "error":error}));
                        settled += 1;
                    }
                }
            }
        }
    }
    settled
}

fn assess_and_save(
    session: &mut Session,
    ordinal: usize,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    runtime: Runtime<'_>,
    shared: &Budget,
) -> Result<Outcome, ClassificationError> {
    let evaluator = dispatcher.evaluator();
    let budget = evaluator.budget(
        shared.deadline(context.deadline),
        context.cancellation.clone(),
    );
    let gate = evaluator.gate();
    let questions: Vec<_> = session
        .keys
        .iter()
        .map(|key| session.questions[key].clone())
        .collect();
    let mut usage = ClassificationUsage::default();
    let mut settled = settle_unsupported(session, shared.limits);
    let mut admitted = Vec::new();
    let mut deferred_by_sibling = false;
    'resources: for (resource_index, resource) in session.resources.iter_mut().enumerate() {
        for (unit_index, unit) in resource.units.iter_mut().enumerate() {
            if unit.cells.iter().all(Option::is_some) {
                continue;
            }
            if !shared.admit(unit.chars) {
                deferred_by_sibling = admitted.is_empty() && shared.has_admissions();
                break 'resources;
            }
            admitted.push([resource_index, unit_index]);
        }
    }
    session.active = admitted.clone();
    if runtime.can_resume
        && !admitted.is_empty()
        && !runtime.sessions.save(
            runtime.sessions.key(&runtime.access, &session.id),
            session.clone(),
            context,
        )
    {
        session.active.clear();
        return Ok(terminal_saved(
            session,
            "The assessment transaction could not be retained before inference; no requests were submitted for this batch.",
            usage,
            runtime,
            context,
        ));
    }
    // Every admitted unit is judged concurrently under the call's budget and
    // gate; results apply in admitted order, so the pinned model and every
    // mismatch decision match a serial run whatever order calls finish in.
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
    let mut results = {
        let (session, budget, gate, questions) = (&*session, &budget, &gate, &questions);
        dispatcher.handle().block_on(
            stream::iter(admitted.iter().zip(&pending).enumerate())
                .map(|(position, (&[resource, unit], pending))| async move {
                    let remaining: Vec<_> = pending
                        .iter()
                        .map(|&index| questions[index].clone())
                        .collect();
                    let result = evaluator
                        .evaluate_questions(
                            EvaluationInput {
                                state: &session.resources[resource].units[unit].state,
                                questions: &remaining,
                                assets: None,
                                long_context: session.long_context,
                            },
                            session.model.as_deref(),
                            budget,
                            gate,
                        )
                        .await;
                    (position, result)
                })
                .buffer_unordered(call_cap(evaluator.max_concurrency))
                .collect::<Vec<_>>(),
        )
    };
    results.sort_unstable_by_key(|(position, _)| *position);
    for ((&[resource, unit], pending), (_, result)) in admitted.iter().zip(pending).zip(results) {
        let unit = &mut session.resources[resource].units[unit];
        usage.add(&result.complete);
        usage.models.extend(result.models.iter().cloned());
        let model_mismatch = result.models.len() > 1
            || session
                .resolved_model
                .as_ref()
                .is_some_and(|model| result.models.iter().any(|actual| actual != model));
        if !model_mismatch && session.resolved_model.is_none() {
            session.resolved_model = result.models.iter().next().cloned();
        }
        for (index, answer) in pending.into_iter().zip(result.answers) {
            let answer = if model_mismatch && answer.is_ok() {
                Err(ClassificationError::invalid_response(
                    "The provider resolved this frozen assessment plan to different model identities; this answer was not accepted.",
                ))
            } else {
                answer
            };
            match answer {
                Ok(answer) => {
                    unit.cells[index] = Some(answered_cell(answer));
                    settled += 1;
                }
                Err(error)
                    if matches!(error.code.as_str(), "timeout" | "cancelled")
                        && error.provider_calls == 0 => {}
                Err(error) => {
                    unit.cells[index] =
                        Some(json!({"status":"error", "error":error_value(&error)}));
                    settled += 1;
                }
            }
        }
    }
    session.active.clear();
    let remaining = pending_cost(session).0;
    if remaining > 0 && (!runtime.can_resume || (settled == 0 && !deferred_by_sibling)) {
        return Ok(terminal_saved(
            session,
            if runtime.can_resume {
                "This assessment batch made no progress before its execution budget ended; remaining cells were not assessed."
            } else {
                "The execution budget ended and this runtime cannot retain a later assessment batch; remaining cells were not assessed."
            },
            usage,
            runtime,
            context,
        ));
    }
    let mut output = emit(session, &usage, None);
    if remaining > 0 {
        output["next"] = json!({"nextSectionPage":Continuation::new(ToolId::Clasify, json!({"operation":"sections","assessment":token(&session.id, ordinal + 1)})).build()});
    }
    session.batches.push(output.clone());
    if runtime.can_resume
        && !runtime.sessions.save(
            runtime.sessions.key(&runtime.access, &session.id),
            session.clone(),
            context,
        )
    {
        session.batches.pop();
        // Include this batch again in the full terminal result because its
        // provisional output has not yet reached the caller.
        for resource in &mut session.resources {
            if let Some(shown) = output["resources"][&resource.key]["sections"].as_array() {
                for unit in &mut resource.units {
                    if shown.iter().any(|item| item["id"] == unit.metadata["id"]) {
                        unit.emitted = false;
                    }
                }
            }
        }
        return Ok(terminal_saved(
            session,
            "The next assessment batch could not be retained; remaining cells were not assessed.",
            usage,
            runtime,
            context,
        ));
    }
    Ok(Outcome { output, usage })
}

fn terminal_saved(
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

fn terminal(session: &mut Session, reason: &str, usage: ClassificationUsage) -> Outcome {
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

fn emit(session: &mut Session, usage: &ClassificationUsage, terminal: Option<&str>) -> Value {
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
    let mut output = json!({"id":session.query_id,"operation":"sections","questions":session.questions,"resources":resources,"usage":usage.output()});
    if let Some(model) = session.resolved_model.as_ref().or(session.model.as_ref()) {
        output["model"] = json!(model);
    }
    output
}

fn new_id(input: &Value) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let now = crate::civil_date::unix_nanos();
    crate::digest::json_sha256(
        &json!({"input":input,"time":now.to_string(),"process":std::process::id(),"serial":serial}),
    )
}

fn answered_cell(mut answer: Value) -> Value {
    if let Some(fields) = answer.as_object_mut() {
        if fields.get("type").and_then(Value::as_str) == Some("score") {
            fields.remove("legend");
        }
        fields.remove("type");
    }
    json!({"status":"answered", "answer":answer})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context(timeout: Duration) -> ExecutionContext {
        ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + timeout,
            walk_threads: None,
            response_window: None,
            github_credential: None,
        }
    }

    fn session() -> Session {
        Session {
            id: "a".repeat(64),
            query_id: "research".into(),
            questions: json!({"a":{"type":"noul","instructions":"a"},"b":{"type":"noul","instructions":"b"}}),
            keys: vec!["a".into(), "b".into()],
            model: Some("jev-1.13.0".into()),
            resolved_model: None,
            long_context: false,
            resources: vec![Resource {
                key: "resource".into(),
                metadata: json!({"status":"ready"}),
                local_check: None,
                units: vec![Unit {
                    metadata: json!({"id":"s0","kind":"text","target":{"bytes":[0,4],"lines":[1,1]},"context":[]}),
                    state: json!({"target":{"text":"text"}}),
                    chars: 26,
                    cells: vec![Some(answered_cell(json!({"type":"noul","noul":0.9}))), None],
                    emitted: false,
                }],
            }],
            github_identity: None,
            batches: Vec::new(),
            active: Vec::new(),
        }
    }

    #[test]
    fn section_answer_projection_preserves_values_and_distributions_with_shared_type() {
        for (provider, expected) in [
            (
                json!({"type":"noul","noul":0.9}),
                json!({"status":"answered","answer":{"noul":0.9}}),
            ),
            (
                json!({"type":"choice","choice":"present","confidence":0.75,"probabilities":{"present":0.75,"absent":0.2,"unknown":0.05}}),
                json!({"status":"answered","answer":{"choice":"present","confidence":0.75,"probabilities":{"present":0.75,"absent":0.2,"unknown":0.05}}}),
            ),
            (
                json!({"type":"score","score":1,"confidence":0.8,"probabilities":{"0":0.2,"1":0.8},"legend":{"0":"absent","1":"present"}}),
                json!({"status":"answered","answer":{"score":1,"confidence":0.8,"probabilities":{"0":0.2,"1":0.8}}}),
            ),
        ] {
            assert_eq!(answered_cell(provider), expected);
        }
    }

    #[test]
    fn partial_cells_are_held_until_the_complete_section_emits_exactly_once() {
        let mut session = session();
        let usage = ClassificationUsage::default();
        let first = emit(&mut session, &usage, None);
        assert_eq!(first["resources"]["resource"]["sections"], json!([]));
        assert_eq!(
            first["resources"]["resource"]["coverage"]["answered"],
            json!(1)
        );
        assert_eq!(
            first["resources"]["resource"]["coverage"]["unassessed"],
            json!(1)
        );
        session.resources[0].units[0].cells[1] = Some(
            json!({"status":"error","error":{"errorCode":"classificationProviderError","error":"refused"}}),
        );
        let second = emit(&mut session, &usage, None);
        assert_eq!(
            second["resources"]["resource"]["sections"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            second["resources"]["resource"]["sections"][0]["answers"]
                .as_object()
                .map(serde_json::Map::len),
            Some(2)
        );
        let third = emit(&mut session, &usage, None);
        assert_eq!(third["resources"]["resource"]["sections"], json!([]));
        assert_eq!(
            third["resources"]["resource"]["coverage"]["remainingSections"],
            json!(0)
        );
    }

    #[test]
    fn terminal_outcome_preserves_answer_and_explains_every_unassessed_cell() {
        let mut session = session();
        let outcome = terminal(&mut session, "budget ended", ClassificationUsage::default());
        let section = &outcome.output["resources"]["resource"]["sections"][0];
        assert_eq!(section["answers"]["a"]["answer"]["noul"], json!(0.9));
        assert_eq!(
            section["answers"]["b"],
            json!({"status":"unassessed","reason":"budget ended"})
        );
        assert_eq!(
            outcome.output["resources"]["resource"]["coverage"]["terminalReason"],
            json!("budget ended")
        );
        assert!(outcome.output.get("next").is_none());
    }

    #[test]
    fn assessment_locks_exclude_the_same_plan_but_not_an_independent_plan() {
        let directory = tempfile::tempdir().expect("lock directory");
        let first = Sessions::new(
            CacheConfig::default(),
            Some(directory.path().to_owned()),
            "policy".into(),
        );
        let second = Sessions::new(
            CacheConfig::default(),
            Some(directory.path().to_owned()),
            "policy".into(),
        );
        let key = first.key(&[1; 32], &"a".repeat(64));
        let other = first.key(&[1; 32], &"b".repeat(64));
        let held = first
            .transaction_lock(&key, &test_context(Duration::from_secs(1)))
            .expect("first transaction");
        assert!(
            second
                .transaction_lock(&key, &test_context(Duration::from_millis(20)))
                .is_err()
        );
        assert!(
            second
                .transaction_lock(&other, &test_context(Duration::from_millis(20)))
                .is_ok()
        );
        assert!(second.save(
            other.clone(),
            session(),
            &test_context(Duration::from_secs(1))
        ));
        assert!(
            second
                .current(&other, &test_context(Duration::from_secs(1)))
                .expect("read")
                .is_some()
        );
        drop(held);
        assert!(
            second
                .transaction_lock(&key, &test_context(Duration::from_millis(20)))
                .is_ok()
        );
    }

    #[test]
    fn work_token_is_bounded_and_does_not_accept_arbitrary_plan_keys() {
        let id = "c".repeat(64);
        let value = token(&id, usize::MAX);
        assert!(
            value.len() <= 128,
            "work token exceeded the canonical bound"
        );
        assert_eq!(decode(&value), Some((id.as_str(), usize::MAX)));
        assert!(decode("sections-v1:../other:0").is_none());
        assert!(decode(&format!("sections-v1:{}:no", id)).is_none());
    }

    #[test]
    fn permanently_unsupported_cells_are_settled_before_lifetime_preflight() {
        let mut session = session();
        let limits = SelectionLimits {
            max_candidates: 1,
            max_chars: 1,
            timeout: Duration::from_secs(1),
        };
        assert_eq!(settle_unsupported(&mut session, limits), 1);
        assert_eq!(pending_cost(&session), (0, 0));
        let output = emit(&mut session, &ClassificationUsage::default(), None);
        let answers = &output["resources"]["resource"]["sections"][0]["answers"];
        assert_eq!(answers["a"]["status"], "answered");
        assert_eq!(answers["b"]["status"], "unsupported");
        assert_eq!(
            answers["b"]["error"]["errorCode"],
            "classificationContextTooLarge"
        );
        assert!(output.get("error").is_none());
        assert!(output.get("next").is_none());
    }

    #[test]
    fn admission_is_shared_and_never_skips_an_over_budget_unit_then_loops() {
        let budget = Budget::new(SelectionLimits {
            max_candidates: 2,
            max_chars: 9,
            timeout: Duration::from_secs(1),
        });
        budget.deadline(Instant::now() + Duration::from_secs(2));
        assert!(budget.admit(5));
        assert!(!budget.admit(5));
        assert!(budget.admit(4));
        assert!(!budget.admit(0));
    }

    #[test]
    fn retained_work_is_partitioned_by_access_and_terminal_receipts_replay() {
        let sessions = Sessions::new(CacheConfig::default(), None, "policy".into());
        let runtime = Runtime {
            sessions: &sessions,
            access: [1; 32],
            can_resume: true,
        };
        let mut session = session();
        let output = terminal_saved(
            &mut session,
            "terminal",
            ClassificationUsage::default(),
            runtime,
            &test_context(Duration::from_secs(5)),
        )
        .output;
        let key = sessions.key(&runtime.access, &session.id);
        let retained = sessions.store.get(&key).expect("terminal batch retained");
        assert_eq!(retained.value.batches[0], output);
        assert!(
            sessions
                .store
                .get(&sessions.key(&[2; 32], &session.id))
                .is_none()
        );
        let other = Sessions::new(CacheConfig::default(), None, "other-policy".into());
        assert!(
            other
                .store
                .get(&other.key(&runtime.access, &session.id))
                .is_none()
        );
    }
}
