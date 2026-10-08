//! Resource-major semantic assessment with capture-once paging.
//!
//! Scheduling: delegated reads share per-call and process-wide bounds. Search
//! candidates hydrate concurrently within those bounds. Every expanded page is captured before provider work so
//! the 25-cell ceiling is enforced without racing or dropping candidates.
//! Provider requests then run concurrently and share the process-wide
//! [`gate`](crate::providers::classification::gate) for their endpoint.
use crate::policy::path::PathPolicy;
use crate::providers::classification::gate::{self, GateLease};
use crate::runtime::{
    ExecutionContext, ExecutionError,
    dispatch::{self, DomainResult},
    domain_dispatch::DomainDispatcher,
};
use crate::tools::clasify::stats::ClassificationUsage;
use crate::tools::clasify::{self, transport::ClassificationError};
use crate::tools::clasify::{
    locate::{
        LocateRead, bare_target_hint, drop_redundant_page_reads, identifier_target, literal_search,
        literal_target_hint, rank_locate, readable_best, with_row_reads,
    },
    output::{self, PageOutcome},
};
use crate::tools::id::ToolId;
use futures_util::{StreamExt, stream::FuturesUnordered};
use secrecy::SecretString;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        Condvar, Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

/// Delegated reads one clasify call runs at once, across all its matrices.
const MAX_CALL_CAPTURES: usize = 4;
/// Delegated reads across every concurrent clasify call in this process.
const MAX_PROCESS_CAPTURES: usize = 16;
// Contract `clasify.semanticPolicy`: one unread search resource hydrates at
// most `maxFileCandidates` files of `maxFileChunkChars` sanitized evidence each,
// and a matrix judges at most `maxCells` expanded pages × questions.
use crate::tools::id::clasify_policy::{
    MAX_FILE_CANDIDATES as MAX_HYDRATED_CANDIDATES, MAX_FILE_CHUNK_CHARS as MAX_HYDRATED_CHARS,
    MAX_PAGE_CHARS, MAX_PAGES, MAX_RESOURCE_CHARS, PREFILTER_WINDOWS,
};
/// Lines on each side of a hit that one hydrated window reads.
pub(super) const HYDRATED_LINE_RADIUS: u64 = 60;
mod provider;
use provider::assess_page;

mod capture;
mod evidence;
mod hydrate;
mod prefilter;
mod render;
use crate::tools::clasify::items;
use crate::tools::clasify::resource::tool_of;
use capture::capture_resource;
use evidence::relativize_local_paths;
pub(super) use hydrate::judged_line_windows;
use render::{
    Rendered, literal_route, matrix_output, render_captured, shared_kind, unjudged_matrix,
};

mod budget;
use budget::{
    Candidate, CaptureBudget, MAX_SHRINK_ATTEMPTS, after_resumed_list, assessed_payload_chars,
    budget_candidates, budget_spent, candidate_chars, evidence_chars, ranges_read, restore_chunk,
    resume_list, resume_search, shrunk_page, span_lines, split_ranges, too_large,
};

/// Row `data` fields that route the host (continuations, scan diagnostics,
/// follow-up hints) rather than carry evidence. They are withheld from the
/// provider and excluded from the `maxChars` evidence budget.
const CONTROL_FIELDS: [&str; 3] = ["next", "diagnostics", "hints"];

/// Counting semaphore for blocking capture workers; waits observe the
/// execution's cancellation and deadline.
struct ReadLimiter {
    max: usize,
    used: Mutex<usize>,
    released: Condvar,
}

struct ReadPermit<'a>(&'a ReadLimiter);

impl ReadLimiter {
    const fn new(max: usize) -> Self {
        Self {
            max,
            used: Mutex::new(0),
            released: Condvar::new(),
        }
    }

    fn acquire(&self, execution: &ExecutionContext) -> Result<ReadPermit<'_>, ExecutionError> {
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            execution.check()?;
            if *used < self.max {
                *used += 1;
                return Ok(ReadPermit(self));
            }
            used = self
                .released
                .wait_timeout(used, Duration::from_millis(50))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

impl Drop for ReadPermit<'_> {
    fn drop(&mut self) {
        let mut used = self.0.used.lock().unwrap_or_else(PoisonError::into_inner);
        *used = used.saturating_sub(1);
        drop(used);
        self.0.released.notify_one();
    }
}

static PROCESS_READS: ReadLimiter = ReadLimiter::new(MAX_PROCESS_CAPTURES);

#[derive(Clone, Copy)]
pub(crate) struct ProviderConfig<'a> {
    pub key: &'a SecretString,
    pub base_url: &'a str,
    pub endpoint_path: &'a str,
    pub model: &'a str,
    pub provider: &'a dyn crate::providers::classification::ClassificationProvider,
    pub retries: u32,
    /// Resolved `classification.maxConcurrency` for the endpoint gate.
    pub max_concurrency: usize,
}

enum CapturedPage {
    Ready {
        state: Value,
        context: Value,
    },
    Failed {
        error: ClassificationError,
        context: Value,
    },
}

fn secured_read(mut read: Value, security: &crate::security::ContentSecurity) -> Option<Value> {
    let checked = security.validate_input_parameters(read.get("query")?);
    if !checked.is_valid {
        return None;
    }
    read["query"] = Value::Object(checked.sanitized_params);
    Some(read)
}

/// The read request behind a page, as the judge needs it: which tool and
/// what it asked for (search text, symbol, keywords, path). Opaque paging
/// tokens are dropped; a read brief is kept only when it adds to the matrix
/// brief. Supplied values have no read.
fn read_brief(resource: &Value, goal: &str, reasoning: &str) -> Option<Value> {
    let tool = tool_of(resource)?.as_str();
    let mut query = resource.get("query")?.as_object()?.clone();
    query.retain(|key, value| match key.as_str() {
        "snapshot" | "diagnosticSnapshot" => false,
        "mainGoal" => value
            .as_str()
            .is_some_and(|text| text.trim() != goal.trim()),
        "reasoning" => value
            .as_str()
            .is_some_and(|text| text.trim() != reasoning.trim()),
        _ => true,
    });
    Some(json!({"tool":tool,"query":query}))
}

/// The walk carries only the brief fields the caller sent.
fn copy_brief(next: &mut Value, query: &Value) {
    for field in ["mainGoal", "reasoning"] {
        if let Some(value) = query.get(field).filter(|value| value.is_string()) {
            next[field] = value.clone();
        }
    }
}

/// A delegated read belongs to the same decision as the matrix. Fill a blank
/// brief from the matrix brief, when the matrix has one.
fn inherit_call_brief(resource: &mut Value, goal: &str, reasoning: &str) {
    let Some(query) = resource
        .pointer_mut("/query")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let fill = |query: &mut serde_json::Map<String, Value>, field: &str, value: &str| {
        if value.is_empty() {
            return;
        }
        let blank = match query.get(field) {
            None => true,
            Some(Value::String(text)) => text.trim().is_empty(),
            Some(_) => false,
        };
        if blank {
            query.insert(field.into(), Value::String(value.to_owned()));
        }
    };
    fill(query, "mainGoal", goal);
    fill(query, "reasoning", reasoning);
}

/// One resource's capture: its pages, the continuation past them, and the
/// search candidates its read reported past the captured page.
type Capture = (Vec<CapturedPage>, Option<Value>, Option<u64>);

/// Run one delegated read within the shared call/process limits. Supplied
/// values do not consume a permit.
fn resolve_limited(
    source: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<
    Result<(Value, Option<Value>), crate::tools::clasify::context::ContextFailure>,
    ExecutionError,
> {
    if tool_of(source).is_none() {
        return Ok(crate::tools::clasify::context::resolve(
            source, dispatcher, execution,
        ));
    }
    let _call = reads.acquire(execution)?;
    let _process = PROCESS_READS.acquire(execution)?;
    Ok(crate::tools::clasify::context::resolve(
        source, dispatcher, execution,
    ))
}

struct CapturedResource<'a> {
    resource: &'a Value,
    pages: Vec<CapturedPage>,
    continuation: Option<Value>,
    /// Search candidates the read reported past the captured page.
    uncaptured: Option<u64>,
}

type PageAssessment = (
    usize,
    usize,
    Vec<Result<Value, ClassificationError>>,
    Option<Value>,
);

/// One provider usage summary: calls and reported tokens (each total only
/// when every call reported it).
fn usage_summary(records: &[Value]) -> Value {
    let total = |key: &str| {
        records
            .iter()
            .map(|record| {
                (record
                    .get("provider_calls")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    == 1)
                    .then(|| record.get(key).and_then(Value::as_u64))
                    .flatten()
            })
            .sum::<Option<u64>>()
    };
    let calls = records
        .iter()
        .map(|record| {
            record
                .get("provider_calls")
                .and_then(Value::as_u64)
                .unwrap_or(1)
        })
        .sum::<u64>();
    let mut usage = json!({"calls":calls});
    if let Some(tokens) = total("input_tokens") {
        usage["inputTokens"] = json!(tokens);
    }
    if let Some(tokens) = total("output_tokens") {
        usage["outputTokens"] = json!(tokens);
    }
    usage
}

/// One normalized matrix in the public shape the walk echoes, with its
/// questions resolved by [`clasify::preflight`] (or the preflight rejection).
type Matrix = (Value, Result<Vec<Value>, ClassificationError>);

/// One matrix's output: compact by default; `debug:true` keeps every page's
/// full answers and adds provider usage receipts (per page and per matrix).
fn execute_query(
    matrix: &Matrix,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
    reads: &ReadLimiter,
) -> Result<(DomainResult, Vec<Value>), ExecutionError> {
    let started = std::time::Instant::now();
    let (mut result, usage) =
        execute_query_verbose(matrix, dispatcher, execution, config, budget, gate, reads)?;
    let query = &matrix.0;
    let locate_ids = query["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|question| clasify::is_locate(question))
        .filter_map(|question| question["id"].as_str())
        .collect::<Vec<_>>();
    let single = match query["resources"].as_array().map(Vec::as_slice) {
        Some([resource]) => resource["id"].as_str(),
        _ => None,
    };
    let matrix = crate::tools::clasify::compact::Matrix {
        single_resource: single,
        locate_ids: &locate_ids,
        default_max_chars: MAX_RESOURCE_CHARS as u64,
    };
    if query.get("debug").and_then(Value::as_bool) == Some(true) {
        crate::tools::clasify::compact::publish_verbose(&mut result.data, &matrix);
        let mut summary = usage_summary(&usage);
        summary["ms"] = json!(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        result.data["usage"] = summary;
        return Ok((result, usage));
    }
    crate::tools::clasify::compact::compact_query_result(&mut result.data, &matrix);
    Ok((result, usage))
}

fn execute_query_verbose(
    (query, resolved): &Matrix,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
    reads: &ReadLimiter,
) -> Result<(DomainResult, Vec<Value>), ExecutionError> {
    execution.check()?;
    let resolved_questions = match resolved {
        Ok(questions) => questions.as_slice(),
        Err(error) => {
            return Ok((
                unjudged_matrix(
                    query,
                    error,
                    "Matrix rejected before context retrieval or classification.",
                ),
                Vec::new(),
            ));
        }
    };
    if let Some(output) = literal_route(query) {
        return Ok((dispatch::value_result(output), Vec::new()));
    }
    let questions = query["questions"]
        .as_array()
        .ok_or(ExecutionError::WorkerFailed)?;
    let resources = query["resources"]
        .as_array()
        .ok_or(ExecutionError::WorkerFailed)?;
    let brief = Brief::of(query);
    // Each page is one provider request with every question batched, so the
    // matrix's resources share a page budget, not a cell budget.
    let candidate_limit = (MAX_PAGES / resources.len().max(1)).max(1);
    let mut captured = capture_all(
        resources,
        &brief,
        candidate_limit,
        dispatcher,
        execution,
        reads,
    )?;
    let assessments = if over_cell_limit(&mut captured, questions.len()) {
        Vec::new()
    } else {
        let judge = Judge {
            questions: resolved_questions,
            brief: &brief,
            dispatcher,
            config,
            budget,
            gate,
        };
        judge.assess(&captured)
    };
    let debug = query.get("debug").and_then(Value::as_bool) == Some(true);
    let Rendered {
        resources: rendered,
        continuations,
        usage,
        locate_reads,
        read_failures,
        judged,
        withheld,
    } = render_captured(captured, assessments, questions, dispatcher, debug)?;
    let output = matrix_output(
        query,
        resources,
        rendered,
        continuations,
        &withheld,
        &locate_reads,
    );
    let mut result = dispatch::value_result(output);
    // Nothing judged and every read failed alike (e.g. every file missing):
    // the call fails the way that read tool fails.
    if !judged {
        result.failure = shared_kind(&read_failures);
    }
    Ok((result, usage))
}

/// The matrix brief every delegated read and provider request carries.
struct Brief {
    goal: String,
    reasoning: String,
}

impl Brief {
    fn of(query: &Value) -> Self {
        let text = |field: &str| {
            query
                .get(field)
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_default()
        };
        Self {
            goal: text("mainGoal"),
            reasoning: text("reasoning"),
        }
    }
}

/// Capture every page before the first provider request. Search fan-out and
/// file paging expand the input matrix, so only the completed capture can
/// enforce the public 25-cell ceiling without racing or silently dropping
/// candidates.
fn capture_all<'a>(
    resources: &'a [Value],
    brief: &Brief,
    candidate_limit: usize,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Vec<CapturedResource<'a>>, ExecutionError> {
    let delegated = resources
        .iter()
        .filter(|resource| tool_of(resource).is_some())
        .count();
    let workers = delegated.clamp(1, MAX_CALL_CAPTURES);
    let cursor = AtomicUsize::new(0);
    let (sender, mut receiver) =
        tokio::sync::mpsc::unbounded_channel::<(usize, Result<Capture, ExecutionError>)>();
    let captures = std::thread::scope(|scope| {
        for _ in 0..workers {
            let sender = sender.clone();
            let cursor = &cursor;
            scope.spawn(move || {
                loop {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(resource) = resources.get(index) else {
                        break;
                    };
                    let mut resource = resource.clone();
                    inherit_call_brief(&mut resource, &brief.goal, &brief.reasoning);
                    let capture =
                        capture_resource(&resource, dispatcher, execution, reads, candidate_limit);
                    if sender.send((index, capture)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        dispatcher.handle.block_on(async {
            let mut captures = resources.iter().map(|_| None).collect::<Vec<_>>();
            while let Some((resource_index, capture)) = receiver.recv().await {
                if let Some(slot) = captures.get_mut(resource_index) {
                    *slot = Some(capture);
                }
            }
            captures
        })
    });
    Ok(captures
        .into_iter()
        .map(|capture| capture.unwrap_or(Err(ExecutionError::WorkerFailed)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .zip(resources)
        .map(
            |((pages, continuation, uncaptured), resource)| CapturedResource {
                resource,
                pages,
                continuation,
                uncaptured,
            },
        )
        .collect())
}

/// Whether the captured pages × questions exceed the matrix cell limit. Then
/// no page is judged: every captured page fails with the count, and no
/// continuation is offered (it would skip these pages for good); the caller
/// retries a smaller matrix.
fn over_cell_limit(captured: &mut [CapturedResource<'_>], questions: usize) -> bool {
    let expanded_pages = captured
        .iter()
        .flat_map(|resource| resource.pages.iter())
        .filter(|page| matches!(page, CapturedPage::Ready { .. }))
        .count();
    if expanded_pages <= MAX_PAGES {
        return false;
    }
    for resource in captured {
        resource.continuation = None;
        for page in &mut resource.pages {
            let CapturedPage::Ready { context, .. } = page else {
                continue;
            };
            *page = CapturedPage::Failed {
                error: ClassificationError::new(
                    "classificationExpandedCellsExceeded",
                    format!(
                        "Captured {expanded_pages} pages for {questions} questions; the limit is {MAX_PAGES} pages per call. No provider request was made."
                    ),
                    "Reduce resources or search pageSize and retry.",
                ),
                context: context.clone(),
            };
        }
    }
    true
}

/// What judging a matrix's captured pages needs.
struct Judge<'a> {
    questions: &'a [Value],
    brief: &'a Brief,
    dispatcher: &'a DomainDispatcher,
    config: ProviderConfig<'a>,
    budget: &'a crate::providers::RequestBudget,
    gate: &'a GateLease,
}

impl Judge<'_> {
    /// Judge every ready page, in resource and page order. Candidates remain
    /// separate states: combining files and rewriting the caller's atomic
    /// questions would make answers comparative and break resource/page
    /// correlation. Provider calls are nevertheless concurrent, and
    /// `assess_page` batches all questions for one candidate whenever `fits`.
    fn assess(&self, captured: &[CapturedResource<'_>]) -> Vec<PageAssessment> {
        let mut assessments = self.dispatcher.handle.block_on(async {
            let mut pending = FuturesUnordered::new();
            for (resource_index, resource) in captured.iter().enumerate() {
                let read = self.read_of(resource.resource);
                for (page_index, page) in resource.pages.iter().enumerate() {
                    let CapturedPage::Ready { state, .. } = page else {
                        continue;
                    };
                    let mut state = state.clone();
                    relativize_local_paths(&mut state, &self.dispatcher.paths);
                    let read = read.clone();
                    pending.push(async move {
                        let (answers, usage) = assess_page(
                            &state,
                            read,
                            self.questions,
                            &self.brief.goal,
                            &self.brief.reasoning,
                            &self.config,
                            self.budget,
                            self.gate,
                        )
                        .await;
                        (resource_index, page_index, answers, usage)
                    });
                }
            }
            let mut assessments = Vec::<PageAssessment>::new();
            while let Some(assessed) = pending.next().await {
                assessments.push(assessed);
            }
            assessments
        });
        assessments
            .sort_by_key(|(resource_index, page_index, _, _)| (*resource_index, *page_index));
        assessments
    }

    /// The read request behind a resource's pages. It leaves the host with
    /// the evidence, so it passes the same input policy as the context read.
    fn read_of(&self, resource: &Value) -> Option<Value> {
        read_brief(resource, &self.brief.goal, &self.brief.reasoning)
            .and_then(|read| secured_read(read, &self.dispatcher.security))
            .map(|mut read| {
                relativize_local_paths(&mut read, &self.dispatcher.paths);
                read
            })
    }
}

/// Finished clasify output: the sanitized `{"queries": rows}` envelope and
/// what the response stage needs besides it.
pub(crate) struct Receipts {
    pub structured: Value,
    pub source_digest: Option<String>,
    pub failure: Option<crate::tools::result::FailureKind>,
}

/// Clasify's entry: evaluate every matrix under the provider scheduler and
/// return the finished `{"queries": rows}` envelope. Rows are receipts, not
/// ordinary result rows: no result-row shaping, minimizing, path compaction,
/// or cross-tool handoff. Rejected input rows keep their input positions.
pub(crate) fn execute(
    queries: &[clasify::ClasifyQuery],
    rejected_rows: Vec<(usize, Value)>,
    dispatcher: &DomainDispatcher,
    context: &ExecutionContext,
    timeout: Duration,
    config: ProviderConfig<'_>,
    record_usage: impl FnOnce(ClassificationUsage),
) -> Result<Receipts, ExecutionError> {
    let evaluated = {
        let _enter = dispatcher.handle.enter();
        let evaluation = ExecutionContext {
            deadline: context.deadline.min(std::time::Instant::now() + timeout),
            ..context.clone()
        };
        evaluate(queries, dispatcher, &evaluation, config, record_usage)?
    };
    let mut rows = Vec::with_capacity(queries.len());
    let mut source_digest = None;
    let mut failures = Vec::with_capacity(queries.len());
    let mut evaluated = evaluated.into_iter();
    for _ in queries {
        context.check()?;
        let result = evaluated.next().ok_or(ExecutionError::WorkerFailed)?;
        context.check()?;
        if queries.len() == 1 {
            source_digest = result.source_digest;
        }
        failures.push(result.failure);
        rows.push(result.data);
    }
    // A call-level kind holds only when every matrix failed the same way; a
    // rejected matrix is a caller error, not that kind.
    let failure = if rejected_rows.is_empty() {
        shared_kind(&failures)
    } else {
        None
    };
    crate::runtime::engine::merge_rejected_rows(&mut rows, rejected_rows);
    let mut structured = json!({"queries": rows});
    // Email redaction is GitHub-only; receipts get the shared field sanitizer.
    crate::response::rows::sanitize_fields(&mut structured, &dispatcher.security, context)?;
    Ok(Receipts {
        structured,
        source_digest,
        failure,
    })
}

/// Limits and billing belong to the account: one provider gate per endpoint
/// and key.
pub(crate) fn account_gate_key(base_url: &str, endpoint_path: &str, key: &SecretString) -> String {
    let endpoint = clasify::transport::endpoint(base_url, endpoint_path).map_or_else(
        |_| format!("{base_url}|{endpoint_path}"),
        |endpoint| gate::endpoint_key(&endpoint),
    );
    let account = crate::digest::json_sha256(&secrecy::ExposeSecret::expose_secret(key));
    format!("{endpoint}#{}", &account[..16])
}

fn evaluate(
    queries: &[clasify::ClasifyQuery],
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    record_usage: impl FnOnce(ClassificationUsage),
) -> Result<Vec<DomainResult>, ExecutionError> {
    // Matrix rules run on the generated query type; the walk then echoes the
    // normalized matrix in its public shape.
    let mut normalized = queries.to_vec();
    clasify::normalize(&mut normalized);
    let matrices = normalized
        .iter()
        .map(|query| Ok((serde_json::to_value(query)?, clasify::preflight(query))))
        .collect::<Result<Vec<Matrix>, serde_json::Error>>()
        .map_err(|_| ExecutionError::WorkerFailed)?;
    let queries = matrices.as_slice();
    let budget = clasify::transport::budget(execution.deadline, execution.cancellation.clone());
    // One lease per tool call: the gate is process-wide, the fairness share
    // and read limiter are per call and shared by every matrix in it.
    let gate = gate::lease(
        &account_gate_key(config.base_url, config.endpoint_path, config.key),
        config.max_concurrency,
    );
    let reads = ReadLimiter::new(MAX_CALL_CAPTURES);
    let completed = if queries.len() > 1 {
        std::thread::scope(|scope| {
            let tasks = queries
                .iter()
                .map(|query| {
                    let (budget, gate, reads) = (&budget, &gate, &reads);
                    scope.spawn(move || {
                        execute_query(query, dispatcher, execution, config, budget, gate, reads)
                    })
                })
                .collect::<Vec<_>>();
            tasks
                .into_iter()
                .map(|task| task.join().map_err(|_| ExecutionError::WorkerFailed)?)
                .collect::<Result<Vec<_>, _>>()
        })?
    } else {
        queries
            .iter()
            .map(|query| {
                execute_query(query, dispatcher, execution, config, &budget, &gate, &reads)
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut outputs = Vec::with_capacity(completed.len());
    let mut usage = ClassificationUsage::default();
    for (output, usage_records) in completed {
        for record in &usage_records {
            usage.add_record(record);
        }
        outputs.push(output);
    }
    record_usage(usage);
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::clasify::locate::{LocatedPage, locate_provider_questions};
    use provider::with_briefs;

    #[test]
    fn normalize_fills_ids_and_implied_fields_and_reads_a_pasted_lead_query() {
        let mut queries = vec![
            json!({
                "mainGoal": "test", "reasoning":"Locate facts",
                "resources":[
                    {"value":"a"},
                    {"id":"resource-1","value":"b"},
                    {"id":"lead","tool":"localFetch","query":{"queries":[{"path":"/a.rs"}]}},
                    {"id":"search","tool":"localSearch","query":{"path":"/repo","matchString":"x"}}
                ],
                "questions":[
                    {"type":"locate","ask":"first"},
                    {"id":"named","type":"yesno","ask":"second"}
                ]
            }),
            json!({
                "id":"matrix-1",
                "mainGoal": "test", "reasoning":"Judge state",
                "resources":[{"value":"c"}],
                "questions":[{"type":"yesno","ask":"third"}]
            }),
        ];
        clasify::normalize_rows(&mut queries);
        assert_eq!(queries[0]["id"], "matrix-1-2");
        assert_eq!(queries[0]["resources"][0]["id"], "resource-1-2");
        assert_eq!(queries[0]["resources"][1]["id"], "resource-1");
        assert_eq!(
            queries[0]["questions"][0],
            json!({"id":"question-1","type":"locate","ask":"first"})
        );
        assert_eq!(queries[0]["questions"][1]["id"], "named");
        // A lead's `{queries:[row]}` reads its row; a whole-file read is explicit.
        assert_eq!(
            queries[0]["resources"][2]["query"],
            json!({"path":"/a.rs","fullContent":true})
        );
        // locate reads a search's files.
        assert_eq!(
            queries[0]["resources"][3]["candidateEvidence"],
            "fileChunks"
        );
        assert_eq!(queries[1]["id"], "matrix-1");
        assert_eq!(queries[1]["resources"][0]["id"], "resource-1");
        assert_eq!(queries[1]["questions"][0]["id"], "question-1");
        assert!(
            queries[1]["resources"][0]
                .get("candidateEvidence")
                .is_none()
        );
        // Several rows are not one read: they stay as sent for the read to reject.
        let rows = json!({"queries":[{"path":"/a"},{"path":"/b"}]});
        let mut many = vec![json!({
            "resources":[{"tool":"localFetch","query":rows.clone()}],
            "questions":[{"type":"yesno","ask":"x"}]
        })];
        clasify::normalize_rows(&mut many);
        assert_eq!(many[0]["resources"][0]["query"], rows);
    }

    #[test]
    fn read_briefs_pass_the_input_security_policy() {
        let security = crate::security::ContentSecurity;
        let read = json!({"tool":"localSearch","query":{"path":"/repo","matchString":"retry"}});
        assert_eq!(secured_read(read.clone(), &security), Some(read));
        let leaky = json!({"tool":"localSearch","query":{
            "path":"/repo","matchString":"ghp_abcdefghijklmnopqrstuvwxyz0123456789"
        }});
        let secured = secured_read(leaky, &security);
        assert!(
            secured.is_none_or(|read| !read
                .to_string()
                .contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789")),
            "a token in the read query never reaches the provider"
        );
    }

    #[test]
    fn provider_state_names_the_read_that_produced_the_evidence() {
        let resource = json!({"id":"hits","tool":"localSearch","query":{
            "path":"/repo","matchString":"Retry-After","snapshot":"opaque",
            "mainGoal":"Find retries.","reasoning":"Only snippets name the retry file."
        }});
        let read = read_brief(&resource, "Find retries.", "Screen first.").expect("tool read");
        assert_eq!(
            read,
            json!({"tool":"localSearch","query":{
                "path":"/repo","matchString":"Retry-After",
                "reasoning":"Only snippets name the retry file."
            }})
        );
        let state = with_briefs(
            json!({"files":[]}),
            "Screen first.",
            "Find retries.",
            Some(read),
        );
        assert_eq!(state["read"]["query"]["matchString"], "Retry-After");
        assert_eq!(state["goal"], "Find retries.");
        assert!(read_brief(&json!({"value":"x"}), "g", "r").is_none());
    }

    #[test]
    fn goal_reaches_every_provider_question_and_the_continuation() {
        let goal = "  Searching for retry handling. Need files that decide a retry.  ";
        let direct = json!({"type":"noul","instructions":"Does this decide a retry?"});
        let stamped = crate::tools::clasify::transport::with_goal(direct.clone(), goal);
        assert_eq!(
            stamped["instructions"],
            json!({
                "question":"Does this decide a retry?",
                "goal":"Searching for retry handling. Need files that decide a retry."
            })
        );
        assert_eq!(direct["instructions"], "Does this decide a retry?");
        let preset = json!({"type":"noul","instructions":{"question":"prompt","target":"retry"}});
        assert_eq!(
            crate::tools::clasify::transport::with_goal(preset, goal)["instructions"]["goal"],
            "Searching for retry handling. Need files that decide a retry."
        );
        let owned = json!({"type":"noul","instructions":{"question":"prompt","goal":"own"}});
        assert_eq!(
            crate::tools::clasify::transport::with_goal(owned, goal)["instructions"]["goal"],
            "own"
        );
        assert_eq!(
            crate::tools::clasify::transport::with_goal(direct, "   ")["instructions"],
            "Does this decide a retry?"
        );
        let [choice, exists] = locate_provider_questions("retry decision", &LocatedPage::default());
        for question in [choice, exists] {
            assert_eq!(
                crate::tools::clasify::transport::with_goal(question, goal)["instructions"]["goal"],
                "Searching for retry handling. Need files that decide a retry."
            );
        }
        let mut next = json!({});
        copy_brief(
            &mut next,
            &json!({"mainGoal":"Searching for retry handling. Need files that decide a retry.",
                "reasoning":"Read the deciding file."}),
        );
        assert_eq!(
            next,
            json!({"mainGoal":"Searching for retry handling. Need files that decide a retry.",
                "reasoning":"Read the deciding file."})
        );
        // No brief sent, none carried: a null brief fails the output contract.
        let mut blank = json!({});
        copy_brief(&mut blank, &json!({"reasoning":null}));
        assert_eq!(blank, json!({}));
    }

    #[test]
    fn usage_summary_counts_transport_retries_and_omits_unknown_totals() {
        assert_eq!(
            usage_summary(&[json!({"provider_calls":1})]),
            json!({"calls":1})
        );
        assert_eq!(
            usage_summary(&[json!({"provider_calls":2,"input_tokens":10,"output_tokens":3})]),
            json!({"calls":2})
        );
        assert_eq!(
            usage_summary(&[json!({"provider_calls":1,"input_tokens":10,"output_tokens":3})]),
            json!({"calls":1,"inputTokens":10,"outputTokens":3})
        );
        assert_eq!(
            usage_summary(&[]),
            json!({"calls":0,"inputTokens":0,"outputTokens":0})
        );
    }
}
