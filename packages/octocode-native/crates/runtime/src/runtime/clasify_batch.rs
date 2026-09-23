//! Resource-major semantic assessment with capture-once paging.
//!
//! Scheduling: delegated reads run on a small worker pool (bounded per call
//! and process-wide) and each captured resource streams straight into
//! provider assessment, so reads and provider latency overlap within the one
//! shared deadline. Provider requests from every matrix, every concurrent
//! tool call, and semantic rerank share the process-wide
//! [`gate`](crate::providers::classification::gate) for their endpoint.
use super::{
    ExecutionContext, ExecutionError,
    clasify_output::{self, PageOutcome},
    dispatch::{self, DomainResult},
    domain_dispatch::DomainDispatcher,
    session_stats::ClassificationUsage,
};
use crate::providers::classification::gate::{self, GateLease};
use crate::tools::clasify::{self, transport::ClassificationError};
use futures_util::{StreamExt, stream, stream::FuturesUnordered};
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
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
pub(super) struct ProviderConfig<'a> {
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

fn fallback_context(source: &Value) -> Value {
    let digest = hex::encode(Sha256::digest(source.to_string().as_bytes()));
    match source.get("tool").and_then(Value::as_str) {
        Some(tool) => json!({"source":"tool","tool":tool,"resultHash":digest,"coverage":"partial",
            "limitations":["Context retrieval failed before a complete page was captured."]}),
        None => json!({"source":"value","resultHash":digest,"coverage":"partial",
            "limitations":["Context retrieval failed before a complete page was captured."]}),
    }
}

fn bounded_prefix(state: &Value, context: &mut Value, max_chars: usize) -> Value {
    let prefix = state
        .to_string()
        .chars()
        .take(max_chars)
        .collect::<String>();
    context["resultHash"] = json!(hex::encode(Sha256::digest(prefix.as_bytes())));
    context["coverage"] = json!("partial");
    if let Some(object) = context.as_object_mut() {
        object.remove("next");
    }
    context["limitations"] = json!([
        "The first sanitized page exceeded maxChars and was assessed only as a bounded prefix; no safe within-page continuation is available."
    ]);
    Value::String(prefix)
}

fn logical_chars(value: &Value) -> usize {
    match value {
        Value::Null => 0,
        Value::Bool(value) => value.to_string().chars().count(),
        Value::Number(value) => value.to_string().chars().count(),
        Value::String(value) => value.chars().count(),
        Value::Array(values) => values.iter().map(logical_chars).sum(),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| key.chars().count().saturating_add(logical_chars(value)))
            .sum(),
    }
}

fn tool_data_chars(data: &Value) -> usize {
    let Some(fields) = data.as_object() else {
        return logical_chars(data);
    };
    fields
        .iter()
        .filter(|(key, _)| !CONTROL_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| key.chars().count().saturating_add(logical_chars(value)))
        .sum()
}

/// Evidence the provider judges: row data without the response envelope or
/// control fields. The envelope's shared `base` stays so relative paths keep
/// their root.
fn tool_payload(state: &Value) -> Value {
    let Some(rows) = state.get("results").and_then(Value::as_array) else {
        return state.clone();
    };
    let mut payload = rows
        .iter()
        .filter_map(|row| row.get("data"))
        .cloned()
        .collect::<Vec<_>>();
    for data in &mut payload {
        if let Some(fields) = data.as_object_mut() {
            fields.retain(|key, _| !CONTROL_FIELDS.contains(&key.as_str()));
        }
    }
    let payload = if payload.len() == 1 {
        payload.pop().unwrap_or(Value::Null)
    } else {
        Value::Array(payload)
    };
    match state.get("base") {
        Some(base) => json!({"base":base,"data":payload}),
        None => payload,
    }
}

/// `[start, end]` source lines of one file-read row, when reported.
fn evidence_lines(data: &Value) -> Option<Value> {
    let range = data
        .get("sourceLineRanges")
        .and_then(Value::as_array)
        .and_then(|ranges| ranges.first());
    if let Some(range) = range {
        return Some(json!([range.get("start")?, range.get("end")?]));
    }
    if let (Some(start), Some(end)) = (data.get("startLine"), data.get("endLine")) {
        return Some(json!([start, end]));
    }
    // Redacted or complete reads may omit ranges; derive them from the line
    // pagination window, or the whole file when unpaginated.
    let returned = data
        .get("returnedLines")
        .or_else(|| data.get("totalLines"))?
        .as_u64()?;
    let offset = match data.get("pagination") {
        Some(pagination) if pagination["chunkType"] == "lines" => pagination["offset"].as_u64()?,
        Some(_) => return None,
        None => 0,
    };
    (returned > 0).then(|| json!([offset + 1, offset + returned]))
}

fn evidence_entry(repo: Option<String>, data: &Value) -> Option<Value> {
    let content = data.get("content").and_then(Value::as_str)?;
    let mut entry = serde_json::Map::new();
    if let Some(repo) = repo {
        entry.insert("repo".into(), json!(repo));
    }
    entry.insert("path".into(), data.get("path")?.clone());
    if let Some(lines) = evidence_lines(data) {
        entry.insert("lines".into(), lines);
    }
    entry.insert("content".into(), json!(content));
    Some(Value::Object(entry))
}

/// File reads judged as evidence only: `{repo?, path, lines, content}`. Read
/// metadata (absolute base, timestamps, byte counters, pagination, file type)
/// cost ~26% extra provider tokens and budget without informing a verdict.
fn file_evidence(state: &Value) -> Option<Value> {
    let mut entries = Vec::new();
    for data in state
        .get("results")?
        .as_array()?
        .iter()
        .filter_map(|row| row.get("data"))
    {
        match data.get("files").and_then(Value::as_array) {
            Some(files) => {
                let repo = match (data["owner"].as_str(), data["repo"].as_str()) {
                    (Some(owner), Some(repo)) => Some(format!("{owner}/{repo}")),
                    _ => None,
                };
                entries.extend(
                    files
                        .iter()
                        .filter_map(|file| evidence_entry(repo.clone(), file)),
                );
            }
            None => entries.extend(evidence_entry(None, data)),
        }
    }
    match entries.len() {
        0 => None,
        1 => entries.pop(),
        _ => Some(Value::Array(entries)),
    }
}

fn is_file_read(source: &Value) -> bool {
    matches!(
        source.get("tool").and_then(Value::as_str),
        Some("localFetch" | "ghGetFileContent")
    )
}

fn provider_state(source: &Value, state: Value) -> Value {
    if source.get("value").is_some() {
        state
    } else if let Some(evidence) = is_file_read(source)
        .then(|| file_evidence(&state))
        .flatten()
    {
        evidence
    } else {
        tool_payload(&state)
    }
}

fn evidence_chars(evidence: &Value) -> usize {
    match evidence {
        Value::Array(entries) => entries.iter().map(evidence_chars).sum(),
        entry => entry["content"]
            .as_str()
            .map_or(0, |content| content.chars().count()),
    }
}

/// The runtime already continued this page, so its continuation and
/// "continue explicitly" limitation no longer describe caller work.
fn mark_followed(context: &mut Value) {
    let Some(receipt) = context.as_object_mut() else {
        return;
    };
    receipt.remove("next");
    if let Some(limitations) = receipt.get_mut("limitations").and_then(Value::as_array_mut) {
        limitations.retain(|limitation| {
            limitation.as_str() != Some(super::clasify_context::PAGE_ONLY_LIMITATION)
        });
        if limitations.is_empty() {
            receipt.remove("limitations");
        }
    }
}

/// Count the sanitized resource payload rather than its transport envelope.
/// JSON punctuation, escaping, row wrappers, and executable continuations are
/// control-plane overhead and must not reduce the caller's `maxChars` budget.
fn assessed_payload_chars(source: &Value, state: &Value) -> usize {
    if source.get("value").is_some() {
        return state.to_string().chars().count();
    }
    if let Some(evidence) = is_file_read(source).then(|| file_evidence(state)).flatten() {
        return evidence_chars(&evidence);
    }
    state
        .get("results")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("data"))
                .map(tool_data_chars)
                .sum()
        })
        .unwrap_or_else(|| logical_chars(state))
}

fn bounded_payload_prefix(
    source: &Value,
    state: &Value,
    context: &mut Value,
    max_chars: usize,
) -> Value {
    if source.get("value").is_some() {
        bounded_prefix(state, context, max_chars)
    } else {
        bounded_prefix(&provider_state(source, state.clone()), context, max_chars)
    }
}

fn capture_resource(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> Result<(Vec<CapturedPage>, Option<Value>), ExecutionError> {
    let max_chars = resource
        .get("maxChars")
        .and_then(Value::as_u64)
        .unwrap_or(80_000) as usize;
    let mut source = resource["context"].clone();
    let pages_within = source
        .get("tool")
        .and_then(Value::as_str)
        .is_some_and(clasify::pages_within_resource);
    let mut pages = Vec::new();
    let mut captured_chars = 0usize;
    let mut seen = HashSet::new();
    let mut remaining = None;

    loop {
        execution.check()?;
        let identity = source.to_string();
        if !seen.insert(identity) {
            pages.push(CapturedPage::Failed {
                error: ClassificationError {
                    code: "classificationContextContinuationLoop".into(),
                    message: "Context continuation repeated without advancing.".into(),
                    hints: vec![
                        "Run the ordinary context tool and inspect its executable continuation."
                            .into(),
                    ],
                    ..Default::default()
                },
                context: fallback_context(&source),
            });
            break;
        }
        match super::clasify_context::resolve(&source, dispatcher, execution) {
            Ok((state, receipt)) => {
                let state_chars = assessed_payload_chars(&source, &state);
                let remaining_chars = max_chars.saturating_sub(captured_chars);
                if state_chars > remaining_chars && !pages.is_empty() {
                    remaining = Some(source);
                    break;
                }
                let mut context = receipt.unwrap_or_else(|| fallback_context(&source));
                // A search page cut mid-JSON is poor evidence and has no safe
                // continuation; ask for a smaller page instead of judging it.
                if state_chars > remaining_chars && !pages_within && source.get("tool").is_some() {
                    pages.push(CapturedPage::Failed {
                        error: ClassificationError::new(
                            "classificationContextTooLarge",
                            format!(
                                "The search page is {state_chars} characters, above maxChars {max_chars}."
                            ),
                            "Lower pageSize or matchContentLength, use a files-only view, or give each candidate its own resource.",
                        ),
                        context,
                    });
                    break;
                }
                if state_chars > remaining_chars {
                    let state = bounded_payload_prefix(
                        &source,
                        &state,
                        &mut context,
                        remaining_chars.max(1),
                    );
                    pages.push(CapturedPage::Ready { state, context });
                    break;
                }
                captured_chars = captured_chars.saturating_add(state_chars);
                // An empty file has nothing to judge: a provider "no" would be a
                // confident false negative and still cost a request.
                if state_chars == 0 && is_file_read(&source) && pages.is_empty() {
                    pages.push(CapturedPage::Failed {
                        error: ClassificationError::new(
                            "classificationContextEmpty",
                            "The resource has no content to judge.",
                            "Drop empty files from the matrix; absence of content is not a verdict.",
                        ),
                        context,
                    });
                    break;
                }
                let state = provider_state(&source, state);
                let Some(next) = super::clasify_context::continuation(&context) else {
                    pages.push(CapturedPage::Ready { state, context });
                    break;
                };
                // A continuation that repeats a read already captured does not
                // advance (e.g. an item whose optional content menu echoes the
                // same query): the resource is fully captured, not looping.
                if seen.contains(&next.to_string()) {
                    mark_followed(&mut context);
                    if let Some(receipt) = context.as_object_mut() {
                        receipt.insert("coverage".into(), json!("bounded"));
                    }
                    pages.push(CapturedPage::Ready { state, context });
                    break;
                }
                if !pages_within || captured_chars >= max_chars || pages.len() + 1 >= 100 {
                    pages.push(CapturedPage::Ready { state, context });
                    remaining = Some(next);
                    break;
                }
                mark_followed(&mut context);
                pages.push(CapturedPage::Ready { state, context });
                source = next;
            }
            Err(failure) => {
                let context = failure.receipt.unwrap_or_else(|| fallback_context(&source));
                if let Some(next) = super::clasify_context::exact_continuation(&context) {
                    source = next;
                    continue;
                }
                pages.push(CapturedPage::Failed {
                    error: failure.error,
                    context,
                });
                break;
            }
        }
    }
    let pages = if pages_within {
        coalesce_pages(pages)
    } else {
        pages
    };
    Ok((pages, remaining))
}

/// Join runs of adjacent captured file pages into larger provider states so
/// one judgment covers a contiguous scope; failed pages break a run.
fn coalesce_pages(pages: Vec<CapturedPage>) -> Vec<CapturedPage> {
    let mut output = Vec::with_capacity(pages.len());
    let mut run = Vec::new();
    let flush = |run: &mut Vec<(Value, Value)>, output: &mut Vec<CapturedPage>| {
        output.extend(
            clasify_output::coalesce(std::mem::take(run), clasify_output::COALESCE_BYTES)
                .into_iter()
                .map(|(state, context)| CapturedPage::Ready { state, context }),
        );
    };
    for page in pages {
        match page {
            CapturedPage::Ready { state, context } => run.push((state, context)),
            failed @ CapturedPage::Failed { .. } => {
                flush(&mut run, &mut output);
                output.push(failed);
            }
        }
    }
    flush(&mut run, &mut output);
    output
}

async fn assess_page(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
) -> (Vec<Result<Value, ClassificationError>>, Option<Value>) {
    let indexed = questions
        .iter()
        .enumerate()
        .map(|(index, question)| (index, &question["question"]))
        .collect::<Vec<_>>();
    if indexed.len() > 1 && clasify::batch::fits(state, &indexed, config.model, config.provider) {
        let result = clasify::batch::execute(
            state,
            &indexed,
            config.key,
            config.base_url,
            config.endpoint_path,
            config.model,
            config.provider,
            budget,
            config.retries,
            gate,
        )
        .await;
        return match result {
            Ok(mut response) => {
                for answer in response.answers.iter_mut().skip(1).flatten() {
                    if let Some(object) = answer.as_object_mut() {
                        object.remove("usage");
                    }
                }
                (response.answers, Some(response.usage))
            }
            Err(error) => (vec![Err(error); questions.len()], None),
        };
    }

    let assessed = stream::iter(questions.iter())
        .map(|question| {
            clasify::execute(
                state,
                &question["question"],
                config.key.clone(),
                config.base_url,
                config.endpoint_path,
                config.model,
                config.provider,
                budget.clone(),
                config.retries,
                gate,
            )
        })
        .buffered(questions.len().max(1))
        .collect::<Vec<_>>()
        .await;
    let usages = assessed
        .iter()
        .filter_map(|result| result.as_ref().ok().map(|data| data["usage"].clone()))
        .collect::<Vec<_>>();
    (assessed, Some(json!({"calls":usages})))
}

type Capture = (Vec<CapturedPage>, Option<Value>);

/// Capture one resource, holding a per-call and a process-wide read permit
/// while a delegated read runs. Supplied values need no permit.
fn capture_limited(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Capture, ExecutionError> {
    if resource["context"].get("tool").is_none() {
        return capture_resource(resource, dispatcher, execution);
    }
    let _call = reads.acquire(execution)?;
    let _process = PROCESS_READS.acquire(execution)?;
    capture_resource(resource, dispatcher, execution)
}

struct CapturedResource<'a> {
    resource: &'a Value,
    pages: Vec<CapturedPage>,
    continuation: Option<Value>,
}

type PageAssessment = (
    usize,
    usize,
    Vec<Result<Value, ClassificationError>>,
    Option<Value>,
    Option<Value>,
);

/// Lines per focus window; pages shorter than two windows are already small.
const FOCUS_WINDOW_LINES: usize = 40;
/// Below this the window choice is too diffuse to be worth reading first.
const MIN_FOCUS_CONFIDENCE: f64 = 0.5;

/// Narrow a high-scoring file page to its best ~40-line window with one
/// Choice over window IDs (the provider's line-search pattern: the page's own
/// Noul is the "exists" check, the Choice ranks where). Returns the focus
/// scope and the extra request's usage.
async fn focus_page(
    state: &Value,
    questions: &[Value],
    answers: &[Result<Value, ClassificationError>],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
) -> Option<(Value, Value)> {
    let content = state.get("content")?.as_str()?;
    let first_line = state.get("lines")?.get(0)?.as_u64()? as usize;
    let lines = content.lines().collect::<Vec<_>>();
    if lines.len() < FOCUS_WINDOW_LINES * 2 {
        return None;
    }
    let instructions = questions
        .iter()
        .zip(answers)
        .find_map(|(question, answer)| {
            let answer = answer.as_ref().ok()?;
            (question["question"]["type"] == "noul"
                && answer.pointer("/answer/noul").and_then(Value::as_f64)? >= 0.8)
                .then(|| question["question"]["instructions"].clone())
        })?;
    let instructions = match instructions {
        Value::String(text) => text,
        other => other.to_string(),
    };
    let mut windows = serde_json::Map::new();
    let mut criteria = serde_json::Map::new();
    let mut spans = Vec::new();
    for (index, chunk) in lines.chunks(FOCUS_WINDOW_LINES).enumerate() {
        let id = format!("w{}", index + 1);
        let start = first_line + index * FOCUS_WINDOW_LINES;
        windows.insert(id.clone(), json!(chunk.join("\n")));
        criteria.insert(id, Value::Null);
        spans.push((start, start + chunk.len() - 1));
    }
    let mut focus_state = json!({"windows": windows});
    if let Some(path) = state.get("path") {
        focus_state["path"] = path.clone();
    }
    let question = json!({
        "type": "choice",
        "instructions": format!("Which window of this content best matches: {instructions}"),
        "criteria": criteria,
    });
    let data = clasify::execute(
        &focus_state,
        &question,
        config.key.clone(),
        config.base_url,
        config.endpoint_path,
        config.model,
        config.provider,
        budget.clone(),
        config.retries,
        gate,
    )
    .await
    .ok()?;
    let choice = data.pointer("/answer/choice")?.as_str()?;
    let index = choice
        .strip_prefix('w')?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)?;
    let (start, end) = *spans.get(index)?;
    let confidence = data.pointer("/answer/confidence").and_then(Value::as_f64)?;
    // A diffuse window choice points nowhere in particular (eval: 0.42 picked a
    // header comment); omit it rather than send the agent to the wrong lines.
    if confidence < MIN_FOCUS_CONFIDENCE {
        return None;
    }
    Some((
        json!({
            "startLine": start,
            "endLine": end,
            "confidence": (confidence * 1000.0).round() / 1000.0
        }),
        data["usage"].clone(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_query(
    query: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
    reads: &ReadLimiter,
) -> Result<(DomainResult, Vec<Value>), ExecutionError> {
    execution.check()?;
    clasify::preflight(query).map_err(|_| ExecutionError::WorkerFailed)?;
    let questions = query["questions"]
        .as_array()
        .ok_or(ExecutionError::WorkerFailed)?
        .iter()
        .map(clasify::with_insufficient_choice)
        .collect::<Vec<_>>();
    let questions = questions.as_slice();
    let resources = query["resources"]
        .as_array()
        .ok_or(ExecutionError::WorkerFailed)?;
    let delegated = resources
        .iter()
        .filter(|resource| resource["context"].get("tool").is_some())
        .count();
    let workers = delegated.clamp(1, MAX_CALL_CAPTURES);
    let cursor = AtomicUsize::new(0);
    let (sender, mut receiver) =
        tokio::sync::mpsc::unbounded_channel::<(usize, Result<Capture, ExecutionError>)>();
    // Capture workers stream each resource to the assessor as soon as it is
    // read, so provider calls start while later resources are still loading.
    let (captures, mut assessments) = std::thread::scope(|scope| {
        for _ in 0..workers {
            let sender = sender.clone();
            let cursor = &cursor;
            scope.spawn(move || {
                loop {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(resource) = resources.get(index) else {
                        break;
                    };
                    let capture = capture_limited(resource, dispatcher, execution, reads);
                    if sender.send((index, capture)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        dispatcher.handle.block_on(async {
            let mut captures = resources.iter().map(|_| None).collect::<Vec<_>>();
            let mut pending = FuturesUnordered::new();
            let mut assessments = Vec::<PageAssessment>::new();
            let mut open = true;
            while open || !pending.is_empty() {
                tokio::select! {
                    received = receiver.recv(), if open => match received {
                        Some((resource_index, capture)) => {
                            if let Ok((pages, _)) = &capture {
                                for (page_index, page) in pages.iter().enumerate() {
                                    let CapturedPage::Ready { state, .. } = page else {
                                        continue;
                                    };
                                    let state = state.clone();
                                    let config = &config;
                                    pending.push(async move {
                                        let (answers, usage) =
                                            assess_page(&state, questions, config, budget, gate)
                                                .await;
                                        let focus = focus_page(
                                            &state, questions, &answers, config, budget, gate,
                                        )
                                        .await;
                                        let usage = match (usage, focus.as_ref()) {
                                            (Some(usage), Some((_, extra))) => {
                                                let mut calls = usage
                                                    .get("calls")
                                                    .and_then(Value::as_array)
                                                    .cloned()
                                                    .unwrap_or_else(|| vec![usage.clone()]);
                                                calls.push(extra.clone());
                                                Some(json!({"calls": calls}))
                                            }
                                            (usage, _) => usage,
                                        };
                                        (
                                            resource_index,
                                            page_index,
                                            answers,
                                            usage,
                                            focus.map(|(scope, _)| scope),
                                        )
                                    });
                                }
                            }
                            if let Some(slot) = captures.get_mut(resource_index) {
                                *slot = Some(capture);
                            }
                        }
                        None => open = false,
                    },
                    Some(assessed) = pending.next(), if !pending.is_empty() => {
                        assessments.push(assessed);
                    }
                }
            }
            (captures, assessments)
        })
    });
    let captured = captures
        .into_iter()
        .map(|capture| capture.unwrap_or(Err(ExecutionError::WorkerFailed)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .zip(resources)
        .map(|((pages, continuation), resource)| CapturedResource {
            resource,
            pages,
            continuation,
        })
        .collect::<Vec<_>>();
    assessments.sort_by_key(|(resource_index, page_index, _, _, _)| (*resource_index, *page_index));
    let mut assessments = assessments.into_iter();
    let question_ids = questions
        .iter()
        .map(|question| &question["id"])
        .collect::<Vec<_>>();
    let mut rendered = Vec::with_capacity(resources.len());
    let mut continuation_resources = Vec::new();
    let mut usage_records = Vec::new();
    let mut resolved_model = None;

    for (resource_index, captured_resource) in captured.into_iter().enumerate() {
        let CapturedResource {
            resource,
            pages,
            continuation,
        } = captured_resource;
        let mut outcomes = Vec::with_capacity(pages.len());
        for (page_index, page) in pages.into_iter().enumerate() {
            match page {
                CapturedPage::Failed { error, context } => {
                    outcomes.push(PageOutcome::Failed {
                        error,
                        receipt: context,
                    });
                }
                CapturedPage::Ready { context, .. } => {
                    let Some((assessed_resource, assessed_page, answers, usage, focus)) =
                        assessments.next()
                    else {
                        return Err(ExecutionError::WorkerFailed);
                    };
                    if assessed_resource != resource_index || assessed_page != page_index {
                        return Err(ExecutionError::WorkerFailed);
                    }
                    if let Some(usage) = usage {
                        if let Some(calls) = usage.get("calls").and_then(Value::as_array) {
                            usage_records.extend(calls.iter().cloned());
                        } else {
                            usage_records.push(usage);
                        }
                    }
                    if resolved_model.is_none() {
                        resolved_model = answers
                            .iter()
                            .flatten()
                            .find_map(|data| data["resolvedModel"].as_str().map(str::to_owned));
                    }
                    outcomes.push(PageOutcome::Assessed {
                        receipt: context,
                        answers,
                        focus,
                    });
                }
            }
        }

        let has_continuation = continuation.is_some();
        if let Some(context) = continuation {
            let mut pending = resource.clone();
            pending["context"] = context;
            continuation_resources.push(pending);
        }
        rendered.push(clasify_output::resource(
            &resource["id"],
            &question_ids,
            outcomes,
            has_continuation,
        ));
    }
    if assessments.next().is_some() {
        return Err(ExecutionError::WorkerFailed);
    }

    let mut output = json!({"queryId":query["id"]});
    for (key, value) in clasify_output::query_meta(&usage_records, resolved_model.as_deref()) {
        output[key.as_str()] = value;
    }
    output["resources"] = Value::Array(rendered);
    if !continuation_resources.is_empty() {
        output["next"] = json!({"clasify":{
            "id":query["id"],
            "reasoning":query["reasoning"],
            "resources":continuation_resources,
            "questions":query["questions"]
        }});
    }
    Ok((dispatch::value_result(output), usage_records))
}

pub(super) fn execute(
    queries: &[Value],
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    record_usage: impl FnOnce(ClassificationUsage),
) -> Result<Vec<DomainResult>, ExecutionError> {
    let budget = clasify::transport::budget(execution.deadline, execution.cancellation.clone());
    let gate_key = clasify::transport::endpoint(config.base_url, config.endpoint_path).map_or_else(
        |_| format!("{}|{}", config.base_url, config.endpoint_path),
        |endpoint| gate::endpoint_key(&endpoint),
    );
    // One lease per tool call: the gate is process-wide, the fairness share
    // and read limiter are per call and shared by every matrix in it.
    let gate = gate::lease(&gate_key, config.max_concurrency);
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

    #[test]
    fn supplied_value_budget_counts_object_keys_and_fields_named_next() {
        let state = json!({"next":{"this-key-is-evidence":"retained"}});
        let source = json!({"value":state});
        assert_eq!(
            assessed_payload_chars(&source, &state),
            state.to_string().chars().count()
        );
    }

    #[test]
    fn tool_budget_excludes_only_the_canonical_data_continuation() {
        let source = json!({"tool":"localFetch","query":{"path":"/tmp/source"}});
        let first = json!({"results":[{"data":{
            "content":{"next":"evidence"},
            "next":{"continue":{"tool":"localFetch","query":{"path":"short"}}}
        }}]});
        let second = json!({"results":[{"data":{
            "content":{"next":"evidence"},
            "next":{"continue":{"tool":"localFetch","query":{"path":"a".repeat(10_000)}}}
        }}]});
        assert_eq!(
            assessed_payload_chars(&source, &first),
            assessed_payload_chars(&source, &second),
            "continuation metadata must not consume the evidence budget"
        );
        assert!(
            assessed_payload_chars(&source, &first) >= "contentnextevidence".chars().count(),
            "nested evidence named next must still consume the budget"
        );
    }

    #[test]
    fn oversized_tool_page_prefix_uses_sanitized_payload_not_envelope() {
        let source = json!({"tool":"localFetch","query":{"path":"/tmp/source"}});
        let state = json!({
            "results":[{"index":0,"data":{
                "content":"deciding-marker",
                "next":{"continue":{"tool":"localFetch","query":{"path":"a".repeat(10_000)}}}
            }}],
            "base":"/tmp"
        });
        let mut context = json!({"next":{"continue":true}});
        let prefix = bounded_payload_prefix(&source, &state, &mut context, 100);
        let prefix = prefix.as_str().expect("bounded prefix string");
        assert!(prefix.contains("deciding-marker"));
        assert!(!prefix.contains("results"));
        assert!(!prefix.contains("continue"));
        assert!(context.get("next").is_none());
    }

    #[test]
    fn provider_payload_keeps_evidence_and_base_but_drops_control_fields() {
        let state = json!({"base":"/repo","results":[{"index":0,"meta":{"x":1},"data":{
            "declarations":[{"name":"apply_row"}],
            "diagnostics":[{"code":"scan.skipped"}],
            "hints":["Try lspSearch"],
            "next":{"continue":{"tool":"localFetch","query":{"path":"a"}}}
        }}]});
        let payload = tool_payload(&state);
        assert_eq!(
            payload,
            json!({"base":"/repo","data":{"declarations":[{"name":"apply_row"}]}})
        );
        let source = json!({"tool":"astSearch","query":{}});
        let bare = json!({"results":[{"data":{"declarations":[{"name":"apply_row"}]}}]});
        assert_eq!(
            assessed_payload_chars(&source, &state),
            assessed_payload_chars(&source, &bare),
            "diagnostics and hints must not consume the evidence budget"
        );
    }

    #[test]
    fn followed_page_receipt_drops_its_continuation_but_keeps_other_limits() {
        let mut context = json!({"coverage":"partial","next":{"continue":{}},"limitations":[
            super::super::clasify_context::PAGE_ONLY_LIMITATION
        ]});
        mark_followed(&mut context);
        assert_eq!(context, json!({"coverage":"partial"}));
        let mut terminal = json!({"next":{"continue":{}},"limitations":["terminal limit"]});
        mark_followed(&mut terminal);
        assert_eq!(terminal, json!({"limitations":["terminal limit"]}));
    }

    #[test]
    fn file_reads_reach_the_provider_as_evidence_only() {
        let local = json!({"base":"/abs/root","results":[{"data":{
            "path":"a.rs","content":"fn a() {}\n","totalLines":1,"modified":"2026",
            "sourceLineRanges":[{"start":1,"end":1}],"sourceBytes":10,"fileType":"code",
            "next":{"continue":{}}}}]});
        let source = json!({"tool":"localFetch","query":{}});
        assert_eq!(
            provider_state(&source, local.clone()),
            json!({"path":"a.rs","lines":[1,1],"content":"fn a() {}\n"})
        );
        assert_eq!(assessed_payload_chars(&source, &local), 10);
        let github = json!({"results":[{"data":{"owner":"o","repo":"r","files":[
            {"path":"b.js","content":"x","startLine":3,"endLine":3,"resolvedBranch":"sha"}]}}]});
        assert_eq!(
            provider_state(&json!({"tool":"ghGetFileContent","query":{}}), github),
            json!({"repo":"o/r","path":"b.js","lines":[3,3],"content":"x"})
        );
    }

    #[test]
    fn evidence_lines_fall_back_to_the_pagination_window() {
        let redacted = json!({"returnedLines":100,
            "pagination":{"chunkType":"lines","offset":600,"chunkSize":100}});
        assert_eq!(evidence_lines(&redacted), Some(json!([601, 700])));
        let whole = json!({"totalLines":42});
        assert_eq!(evidence_lines(&whole), Some(json!([1, 42])));
        let bytes = json!({"returnedLines":5,"pagination":{"chunkType":"bytes","offset":10}});
        assert_eq!(evidence_lines(&bytes), None);
    }

    #[test]
    fn adjacent_ready_pages_coalesce_and_failed_pages_split_runs() {
        let ready = |start: u64| CapturedPage::Ready {
            state: json!({"content":"x"}),
            context: json!({"coverage":"bounded","scope":{"startLine":start,"endLine":start + 99,"totalLines":400}}),
        };
        let pages = coalesce_pages(vec![
            ready(1),
            ready(101),
            CapturedPage::Failed {
                error: ClassificationError::default(),
                context: json!({"coverage":"partial"}),
            },
            ready(201),
        ]);
        assert_eq!(pages.len(), 3);
        let CapturedPage::Ready { state, context } = &pages[0] else {
            panic!("first run is ready");
        };
        assert!(state.is_array());
        assert_eq!(
            context["scope"],
            json!({"startLine":1,"endLine":200,"totalLines":400})
        );
        assert!(matches!(pages[1], CapturedPage::Failed { .. }));
    }
}
