//! Resource-major semantic assessment with capture-once paging.
//!
//! Scheduling: delegated reads share per-call and process-wide bounds. Search
//! candidates hydrate concurrently within those bounds. Every expanded page is captured before provider work so
//! the 25-cell ceiling is enforced without racing or dropping candidates.
//! Provider requests then run concurrently and share the process-wide
//! [`gate`](crate::providers::classification::gate) for their endpoint.
use super::{
    ExecutionContext, ExecutionError,
    clasify_locate::{
        LocatedPage, collapse_locate_answer, literal_target_hint, locate_provider_questions,
        located_state, rank_locate,
    },
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
/// One unread search resource may hydrate at most this many files. Five also
/// fits the public 25-cell matrix when all five questions are used.
const MAX_HYDRATED_CANDIDATES: usize = 5;
/// Hard cap on sanitized candidate evidence sent to the provider.
const MAX_HYDRATED_CHARS: usize = 12_000;
const HYDRATED_LINE_RADIUS: u64 = 60;
const MAX_EXPANDED_CELLS: usize = 25;
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
    let ranges = data.get("sourceLineRanges").and_then(Value::as_array);
    if let Some(ranges) = ranges.filter(|ranges| ranges.len() > 1) {
        let pairs = ranges
            .iter()
            .map(|range| Some(json!([range.get("start")?, range.get("end")?])))
            .collect::<Option<Vec<_>>>()?;
        return Some(Value::Array(pairs));
    }
    let range = ranges.and_then(|ranges| ranges.first());
    if let Some(range) = range {
        return Some(json!([range.get("start")?, range.get("end")?]));
    }
    if let (Some(start), Some(end)) = (data.get("startLine"), data.get("endLine"))
        && data
            .get("contentView")
            .and_then(Value::as_str)
            .is_none_or(|view| view == "none")
    {
        return Some(json!([start, end]));
    }
    // A compacted view has its own line positions. Without explicit source
    // ranges, its offsets cannot be presented as source line coordinates.
    if data
        .get("contentView")
        .and_then(Value::as_str)
        .is_some_and(|view| view != "none")
    {
        return None;
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

fn is_candidate_search(source: &Value) -> bool {
    matches!(
        source.get("tool").and_then(Value::as_str),
        Some("localSearch" | "ghSearchCode")
    )
}

fn file_chunks(source: &Value) -> bool {
    source.get("candidateEvidence").and_then(Value::as_str) == Some("fileChunks")
}

fn candidate_identity(source: &Value, state: &Value, file: &Value) -> Option<String> {
    let path = file.get("path")?.as_str()?;
    match source.get("tool").and_then(Value::as_str)? {
        "localSearch" => Some(
            state
                .get("base")
                .and_then(Value::as_str)
                .filter(|_| !std::path::Path::new(path).is_absolute())
                .map_or_else(
                    || path.to_owned(),
                    |base| {
                        std::path::Path::new(base)
                            .join(path)
                            .to_string_lossy()
                            .into_owned()
                    },
                ),
        ),
        "ghSearchCode" => Some(format!(
            "{}/{}/{}",
            file.get("owner")?.as_str()?.to_ascii_lowercase(),
            file.get("repo")?.as_str()?.to_ascii_lowercase(),
            path
        )),
        _ => None,
    }
}

/// Turn one lexical search page into independent, path-deduplicated files.
fn search_candidate_states(source: &Value, state: &Value) -> Option<Vec<Value>> {
    if !is_candidate_search(source) {
        return None;
    }
    let files = state.pointer("/results/0/data/files")?.as_array()?;
    let mut seen = HashSet::new();
    let candidates = files
        .iter()
        .filter(|file| candidate_identity(source, state, file).is_some_and(|id| seen.insert(id)))
        .map(|file| {
            let mut candidate = state.clone();
            candidate["results"][0]["data"]["files"] = json!([file]);
            candidate
        })
        .collect::<Vec<_>>();
    (!candidates.is_empty()).then_some(candidates)
}

fn local_candidate_read(candidate: &Value, max_bytes: usize) -> Option<Value> {
    let file = candidate.pointer("/results/0/data/files/0")?;
    let path = candidate_identity(&json!({"tool":"localSearch"}), candidate, file)?;
    let line = file
        .get("matches")
        .and_then(Value::as_array)
        .and_then(|matches| matches.first())
        .and_then(|matched| matched.get("line"))
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let start = line.saturating_sub(HYDRATED_LINE_RADIUS).max(1);
    let end = line.saturating_add(HYDRATED_LINE_RADIUS);
    Some(json!({
        "tool":"localFetch",
        "query":{
            "reasoning":"Read a bounded search candidate for classification.",
            "path":path,
            "startLine":start,
            "endLine":end,
            "chunkType":"bytes",
            "chunkSize":max_bytes,
            "minify":"none"
        }
    }))
}

fn utf16_slice(value: &str, start: usize, end: usize) -> Option<String> {
    let text = value.encode_utf16().collect::<Vec<_>>();
    String::from_utf16(text.get(start..end)?).ok()
}

fn github_candidate_read(candidate: &Value, max_bytes: usize) -> Option<(Value, bool)> {
    let file = candidate.pointer("/results/0/data/files/0")?;
    let mut query = json!({
        "reasoning":"Read a bounded GitHub search candidate for classification.",
        "owner":file.get("owner")?,
        "repo":file.get("repo")?,
        "path":file.get("path")?,
        "chunkType":"bytes",
        "chunkSize":max_bytes,
        "minify":"none"
    });
    let anchor = file
        .get("matches")
        .and_then(Value::as_array)
        .and_then(|matches| matches.first())
        .and_then(|matched| {
            let value = matched.get("value")?.as_str()?;
            let range = matched.get("matchIndices")?.as_array()?.first()?;
            let start = usize::try_from(range.get("start")?.as_u64()?).ok()?;
            let end = usize::try_from(range.get("end")?.as_u64()?).ok()?;
            utf16_slice(value, start, end).filter(|value| !value.trim().is_empty())
        });
    let anchored = anchor.is_some();
    if let Some(anchor) = anchor {
        query["matchString"] = json!(anchor);
        query["contextLines"] = json!(20);
    } else {
        query["startLine"] = json!(1);
        query["endLine"] = json!(HYDRATED_LINE_RADIUS * 2 + 1);
    }
    Some((json!({"tool":"ghGetFileContent","query":query}), anchored))
}

fn candidate_read(source: &Value, candidate: &Value, max_bytes: usize) -> Option<(Value, bool)> {
    match source.get("tool").and_then(Value::as_str) {
        Some("localSearch") => local_candidate_read(candidate, max_bytes).map(|read| (read, true)),
        Some("ghSearchCode") => github_candidate_read(candidate, max_bytes),
        _ => None,
    }
}

fn pin_github_read(read: &mut Value, state: &Value) {
    if read.get("tool").and_then(Value::as_str) != Some("ghGetFileContent") {
        return;
    }
    if let Some(commit) = state
        .pointer("/results/0/data/files/0/commitSha")
        .and_then(Value::as_str)
    {
        read["query"]["branch"] = json!(commit);
    }
}

fn default_search_page_size(source: &Value) -> u64 {
    if source.get("tool").and_then(Value::as_str) == Some("ghSearchCode") {
        return 30;
    }
    match source.pointer("/query/resultView").and_then(Value::as_str) {
        Some("files" | "filesWithout" | "discovery" | "countLines" | "countMatches") => 100,
        _ => 20,
    }
}

/// Bound candidate fan-out before executing search. Rewriting page size is
/// safe only when the original offset is representable by the new page size.
fn bounded_search_source(
    source: &Value,
    candidate_limit: usize,
) -> Result<Value, ClassificationError> {
    if !is_candidate_search(source) || candidate_limit == 0 {
        return Ok(source.clone());
    }
    let mut bounded = source.clone();
    let query = bounded
        .get_mut("query")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            ClassificationError::new(
                "invalidClassificationContext",
                "Search candidate context is missing its query.",
                "Pass one ordinary localSearch or ghSearchCode query.",
            )
        })?;
    let original_size = query
        .get("pageSize")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| default_search_page_size(source));
    let limit = u64::try_from(candidate_limit).unwrap_or(u64::MAX);
    if original_size <= limit {
        return Ok(bounded);
    }
    let page = query.get("page").and_then(Value::as_u64).unwrap_or(1);
    let offset = page.saturating_sub(1).saturating_mul(original_size);
    if offset % limit != 0 {
        return Err(ClassificationError::new(
            "classificationExpandedCellsExceeded",
            format!(
                "Search page {page} with pageSize {original_size} cannot be safely bounded to {limit} candidates without skipping or repeating files."
            ),
            "Restart this clasify resource at page 1 or set pageSize to the returned bounded size.",
        ));
    }
    query.insert("pageSize".into(), json!(limit));
    query.insert("page".into(), json!(offset / limit + 1));
    Ok(bounded)
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

fn hydrate_candidate(
    source: &Value,
    candidate: Value,
    mut read: Value,
    anchored: bool,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> CapturedPage {
    match super::clasify_context::resolve(&read, dispatcher, execution) {
        Ok((hydrated_state, hydrated_receipt)) => {
            let evidence = provider_state(&read, hydrated_state.clone());
            let evidence_chars = evidence_chars(&evidence);
            let mut context = hydrated_receipt.unwrap_or_else(|| fallback_context(&read));
            super::clasify_context::append_limitation(
                &mut context,
                "Only a bounded candidate chunk was assessed; unread file content may change the verdict.",
            );
            if !anchored {
                super::clasify_context::append_limitation(
                    &mut context,
                    "No stable match anchor was available; only the file's opening chunk was assessed.",
                );
            }
            pin_github_read(&mut read, &hydrated_state);
            read["confidence"] = json!("exact");
            super::clasify_context::attach_read(&mut context, read);
            if evidence_chars == 0 {
                CapturedPage::Failed {
                    error: ClassificationError::new(
                        "classificationContextEmpty",
                        "The hydrated candidate contained no evidence to judge.",
                        "Read the candidate directly or choose a different search anchor.",
                    ),
                    context,
                }
            } else if evidence_chars > MAX_HYDRATED_CHARS {
                CapturedPage::Failed {
                    error: ClassificationError::new(
                        "classificationCandidateChunkTooLarge",
                        format!(
                            "The sanitized candidate chunk is {evidence_chars} characters; the limit is {MAX_HYDRATED_CHARS}."
                        ),
                        "Use next.read to select a smaller exact region.",
                    ),
                    context,
                }
            } else {
                CapturedPage::Ready {
                    state: evidence,
                    context,
                }
            }
        }
        Err(failure) => {
            let mut context = failure
                .receipt
                .unwrap_or_else(|| super::clasify_context::candidate_receipt(source, &candidate));
            super::clasify_context::append_limitation(
                &mut context,
                "Candidate hydration failed; classification was not run for this file.",
            );
            CapturedPage::Failed {
                error: failure.error,
                context,
            }
        }
    }
}

fn hydrate_candidates(
    source: &Value,
    candidates: Vec<Value>,
    max_bytes: usize,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Vec<CapturedPage>, ExecutionError> {
    std::thread::scope(|scope| {
        let mut completed = Vec::with_capacity(candidates.len());
        let mut tasks = Vec::with_capacity(candidates.len());
        for (index, candidate) in candidates.into_iter().enumerate() {
            execution.check()?;
            let Some((read, anchored)) = candidate_read(source, &candidate, max_bytes) else {
                completed.push((
                    index,
                    CapturedPage::Failed {
                        error: ClassificationError::new(
                            "classificationCandidateUnhydratable",
                            "A search result did not contain a usable file identity.",
                            "Run the search directly and inspect the malformed candidate.",
                        ),
                        context: super::clasify_context::candidate_receipt(source, &candidate),
                    },
                ));
                continue;
            };
            // Acquire before spawning so at most the permitted number of
            // blocking workers exists; later candidates wait in this loop.
            let call_permit = reads.acquire(execution)?;
            let process_permit = PROCESS_READS.acquire(execution)?;
            tasks.push((
                index,
                scope.spawn(move || {
                    let (_call_permit, _process_permit) = (call_permit, process_permit);
                    hydrate_candidate(source, candidate, read, anchored, dispatcher, execution)
                }),
            ));
        }
        for (index, task) in tasks {
            completed.push((
                index,
                task.join().map_err(|_| ExecutionError::WorkerFailed)?,
            ));
        }
        completed.sort_by_key(|(index, _)| *index);
        Ok(completed.into_iter().map(|(_, page)| page).collect())
    })
}

fn capture_resource(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
    candidate_limit: usize,
) -> Result<(Vec<CapturedPage>, Option<Value>), ExecutionError> {
    if let Some(windows) = prefilter_windows(resource, dispatcher, execution, reads)? {
        let mut pages = Vec::new();
        for window in &windows {
            let (window_pages, _) =
                capture_resource(window, dispatcher, execution, reads, candidate_limit)?;
            pages.extend(window_pages);
        }
        return Ok((pages, None));
    }
    let max_chars = resource
        .get("maxChars")
        .and_then(Value::as_u64)
        .unwrap_or(80_000) as usize;
    let requested_source = resource["context"].clone();
    let hydrated = file_chunks(&requested_source);
    let candidate_limit = if hydrated && is_candidate_search(&requested_source) {
        candidate_limit.min(MAX_HYDRATED_CANDIDATES)
    } else {
        candidate_limit
    };
    let mut source = match bounded_search_source(&requested_source, candidate_limit) {
        Ok(source) => source,
        Err(error) => {
            return Ok((
                vec![CapturedPage::Failed {
                    error,
                    context: fallback_context(&requested_source),
                }],
                None,
            ));
        }
    };
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
        match resolve_limited(&source, dispatcher, execution, reads)? {
            Ok((state, receipt)) => {
                if let Some(candidates) = search_candidate_states(&source, &state) {
                    let mut next = if hydrated
                        && source.get("tool").and_then(Value::as_str) == Some("localSearch")
                    {
                        super::clasify_context::continuation_named(
                            &receipt.clone().unwrap_or_else(|| fallback_context(&source)),
                            "nextPage",
                        )
                    } else {
                        receipt
                            .as_ref()
                            .and_then(super::clasify_context::continuation)
                    };
                    if hydrated
                        && let (Some(next), Some(candidate_evidence)) = (
                            next.as_mut().and_then(Value::as_object_mut),
                            requested_source.get("candidateEvidence"),
                        )
                    {
                        next.insert("candidateEvidence".into(), candidate_evidence.clone());
                    }
                    if !hydrated {
                        pages.extend(candidates.into_iter().map(|candidate| {
                            let context =
                                super::clasify_context::candidate_receipt(&source, &candidate);
                            CapturedPage::Ready {
                                state: provider_state(&source, candidate),
                                context,
                            }
                        }));
                        remaining = next;
                        break;
                    }

                    let max_bytes = max_chars
                        .checked_div(candidates.len().max(1))
                        .unwrap_or(max_chars)
                        .clamp(1, MAX_HYDRATED_CHARS);
                    let hydrated_pages = hydrate_candidates(
                        &source, candidates, max_bytes, dispatcher, execution, reads,
                    )?;
                    pages.extend(hydrated_pages);
                    remaining = next;
                    break;
                }
                let state_chars = assessed_payload_chars(&source, &state);
                let remaining_chars = max_chars.saturating_sub(captured_chars);
                if state_chars > remaining_chars && !pages.is_empty() {
                    remaining = Some(source);
                    break;
                }
                let mut context = receipt.unwrap_or_else(|| fallback_context(&source));
                // Never classify an arbitrary prefix with the full page's
                // source receipt. The caller can choose a smaller complete section.
                if state_chars > remaining_chars {
                    pages.push(CapturedPage::Failed {
                        error: ClassificationError::new(
                            "classificationContextTooLarge",
                            format!(
                                "The first captured page is {state_chars} characters, above maxChars {max_chars}; no classification was run."
                            ),
                            "Select a smaller complete section, reduce the read page size, or raise maxChars within its limit.",
                        ),
                        context,
                    });
                    break;
                }
                captured_chars = captured_chars.saturating_add(state_chars);
                // An empty file has nothing to judge: a provider "no" would be a
                // confident false negative and still cost a request.
                if state_chars == 0 && is_file_read(&source) && pages.is_empty() {
                    // An oversized whole-file read (ghGetFileContent
                    // fullContentLimit) returns no body plus an exact page
                    // continuation: page through it instead.
                    if let Some(next) = super::clasify_context::continuation(&context)
                        && !seen.contains(&next.to_string())
                    {
                        captured_chars = 0;
                        source = next;
                        continue;
                    }
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

/// Lines per prefilter window and windows kept.
const PREFILTER_WINDOW_LINES: u64 = 600;
const PREFILTER_WINDOWS: usize = 3;

/// `prefilter` on a file resource: read the same file once for the literal
/// terms, then capture only the densest `PREFILTER_WINDOWS` windows of hits
/// as bounded reads instead of the whole file. `None` (no prefilter, not a
/// file read, or no hits) captures the resource as given.
fn prefilter_windows(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Option<Vec<Value>>, ExecutionError> {
    let Some(terms) = resource.get("prefilter").and_then(Value::as_array) else {
        return Ok(None);
    };
    let terms = terms
        .iter()
        .filter_map(Value::as_str)
        .filter(|term| !term.trim().is_empty())
        .map(regex::escape)
        .collect::<Vec<_>>();
    let tool = resource["context"]["tool"].as_str().unwrap_or_default();
    if terms.is_empty() || !matches!(tool, "localFetch" | "ghGetFileContent") {
        return Ok(None);
    }
    let mut query = resource["context"]["query"].clone();
    let Some(object) = query.as_object_mut() else {
        return Ok(None);
    };
    for field in [
        "fullContent",
        "startLine",
        "endLine",
        "offset",
        "chunkSize",
        "chunkType",
        "minify",
    ] {
        object.remove(field);
    }
    object.insert("matchString".into(), json!(terms.join("|")));
    object.insert("matchStringIsRegex".into(), json!(true));
    object.insert("matchStringCaseSensitive".into(), json!(false));
    object.insert("contextLines".into(), json!(0));
    let probe = json!({"tool":tool,"query":query});
    let Ok((_, Some(receipt))) = resolve_limited(&probe, dispatcher, execution, reads)? else {
        return Ok(None);
    };
    let scope = &receipt["scope"];
    let Some(total) = scope["totalLines"].as_u64() else {
        return Ok(None);
    };
    let ranges = scope["lineRanges"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![scope.clone()]);
    let mut density = std::collections::BTreeMap::<u64, usize>::new();
    for range in &ranges {
        if let (Some(start), Some(end)) = (range["startLine"].as_u64(), range["endLine"].as_u64()) {
            for line in start..=end {
                *density
                    .entry((line - 1) / PREFILTER_WINDOW_LINES)
                    .or_default() += 1;
            }
        }
    }
    if density.is_empty() {
        return Ok(None);
    }
    let mut buckets = density.into_iter().collect::<Vec<_>>();
    buckets.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    buckets.truncate(PREFILTER_WINDOWS);
    buckets.sort_by_key(|(bucket, _)| *bucket);
    Ok(Some(
        buckets
            .into_iter()
            .map(|(bucket, _)| {
                let mut window = resource.clone();
                if let Some(object) = window.as_object_mut() {
                    object.remove("prefilter");
                }
                let query = &mut window["context"]["query"];
                if let Some(object) = query.as_object_mut() {
                    object.remove("fullContent");
                }
                query["startLine"] = json!(bucket * PREFILTER_WINDOW_LINES + 1);
                query["endLine"] = json!(((bucket + 1) * PREFILTER_WINDOW_LINES).min(total));
                window
            })
            .collect(),
    ))
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

async fn assess_provider_page(
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

enum PublicAnswerPlan {
    Direct(usize),
    Locate {
        choice: usize,
        exists: usize,
        page: LocatedPage,
    },
    Failed(ClassificationError),
}

async fn assess_page(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
) -> (Vec<Result<Value, ClassificationError>>, Option<Value>) {
    let has_locate = questions
        .iter()
        .any(|question| clasify::questions::is_locate(&question["question"]));
    let located = has_locate.then(|| located_state(state));
    let (provider_state, page, locate_error) = match located {
        Some(Ok((state, page))) => (state, page, None),
        Some(Err(error)) => (state.clone(), LocatedPage::default(), Some(error)),
        None => (state.clone(), LocatedPage::default(), None),
    };
    let mut provider_questions = Vec::new();
    let mut plans = Vec::with_capacity(questions.len());
    for question in questions {
        if clasify::questions::is_locate(&question["question"]) {
            if let Some(error) = &locate_error {
                plans.push(PublicAnswerPlan::Failed(error.clone()));
                continue;
            }
            let target = question["question"]["target"].as_str().unwrap_or_default();
            let [choice, exists] = locate_provider_questions(target, &page);
            let choice_index = provider_questions.len();
            provider_questions.push(json!({"id":question["id"],"question":choice}));
            let exists_index = provider_questions.len();
            provider_questions.push(json!({"id":question["id"],"question":exists}));
            plans.push(PublicAnswerPlan::Locate {
                choice: choice_index,
                exists: exists_index,
                page: page.clone(),
            });
        } else {
            let index = provider_questions.len();
            provider_questions.push(question.clone());
            plans.push(PublicAnswerPlan::Direct(index));
        }
    }
    let (answers, usage) = if provider_questions.is_empty() {
        (Vec::new(), None)
    } else {
        assess_provider_page(&provider_state, &provider_questions, config, budget, gate).await
    };
    let projected = plans
        .into_iter()
        .map(|plan| match plan {
            PublicAnswerPlan::Direct(index) => answers.get(index).cloned().unwrap_or_else(|| {
                Err(ClassificationError::new(
                    "invalidClassificationResponse",
                    "Provider answer count did not match the requested questions.",
                    "Inspect provider compatibility before using the answer.",
                ))
            }),
            PublicAnswerPlan::Locate {
                choice,
                exists,
                page,
            } => match (answers.get(choice), answers.get(exists)) {
                (Some(choice), Some(exists)) => collapse_locate_answer(choice, exists, &page),
                _ => Err(ClassificationError::new(
                    "invalidClassificationResponse",
                    "Provider answer count did not match the locate questions.",
                    "Inspect provider compatibility before using the answer.",
                )),
            },
            PublicAnswerPlan::Failed(error) => Err(error),
        })
        .collect();
    (projected, usage)
}

type Capture = (Vec<CapturedPage>, Option<Value>);

/// Run one delegated read within the shared call/process limits. Supplied
/// values do not consume a permit.
fn resolve_limited(
    source: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Result<(Value, Option<Value>), super::clasify_context::ContextFailure>, ExecutionError>
{
    if source.get("tool").is_none() {
        return Ok(super::clasify_context::resolve(
            source, dispatcher, execution,
        ));
    }
    let _call = reads.acquire(execution)?;
    let _process = PROCESS_READS.acquire(execution)?;
    Ok(super::clasify_context::resolve(
        source, dispatcher, execution,
    ))
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
);

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
    let resolved_questions = clasify::preflight(query).map_err(|_| ExecutionError::WorkerFailed)?;
    let questions = query["questions"]
        .as_array()
        .ok_or(ExecutionError::WorkerFailed)?;
    let resolved_questions = resolved_questions.as_slice();
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
    let candidate_limit = (MAX_EXPANDED_CELLS / questions.len().max(1)).max(1);
    // Capture every page before the first provider request. Search fan-out and
    // file paging expand the input matrix, so only the completed capture can
    // enforce the public 25-cell ceiling without racing or silently dropping
    // candidates.
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
                    let capture =
                        capture_resource(resource, dispatcher, execution, reads, candidate_limit);
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
    let mut captured = captures
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

    let expanded_pages = captured
        .iter()
        .flat_map(|resource| resource.pages.iter())
        .filter(|page| matches!(page, CapturedPage::Ready { .. }))
        .count();
    let expanded_cells = expanded_pages.saturating_mul(questions.len());
    if expanded_cells > MAX_EXPANDED_CELLS {
        for resource in &mut captured {
            for page in &mut resource.pages {
                let CapturedPage::Ready { context, .. } = page else {
                    continue;
                };
                *page = CapturedPage::Failed {
                    error: ClassificationError::new(
                        "classificationExpandedCellsExceeded",
                        format!(
                            "Captured {expanded_pages} pages × {} questions = {expanded_cells} cells; the limit is {MAX_EXPANDED_CELLS}. No provider request was made.",
                            questions.len()
                        ),
                        "Reduce resources, questions, or search pageSize and retry.",
                    ),
                    context: context.clone(),
                };
            }
        }
    }

    // Candidates remain separate states: combining files and rewriting the
    // caller's atomic questions would make answers comparative and break
    // resource/page correlation. Provider calls are nevertheless concurrent,
    // and assess_page batches all questions for one candidate whenever `fits`.
    let mut assessments = if expanded_cells > MAX_EXPANDED_CELLS {
        Vec::new()
    } else {
        dispatcher.handle.block_on(async {
            let mut pending = FuturesUnordered::new();
            for (resource_index, resource) in captured.iter().enumerate() {
                for (page_index, page) in resource.pages.iter().enumerate() {
                    let CapturedPage::Ready { state, .. } = page else {
                        continue;
                    };
                    let state = state.clone();
                    let config = &config;
                    pending.push(async move {
                        let (answers, usage) =
                            assess_page(&state, resolved_questions, config, budget, gate).await;
                        (resource_index, page_index, answers, usage)
                    });
                }
            }
            let mut assessments = Vec::<PageAssessment>::new();
            while let Some(assessed) = pending.next().await {
                assessments.push(assessed);
            }
            assessments
        })
    };
    assessments.sort_by_key(|(resource_index, page_index, _, _)| (*resource_index, *page_index));
    let mut assessments = assessments.into_iter();
    let question_ids = questions
        .iter()
        .map(|question| &question["id"])
        .collect::<Vec<_>>();
    let mut rendered = Vec::with_capacity(resources.len());
    let mut continuation_resources = Vec::new();
    let mut usage_records = Vec::new();

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
                    let Some((assessed_resource, assessed_page, answers, usage)) =
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
                    outcomes.push(PageOutcome::Assessed {
                        receipt: context,
                        answers,
                    });
                }
            }
        }

        let has_continuation = continuation.is_some();
        if let Some(mut context) = continuation {
            if let Some(tool) = context["tool"].as_str().map(str::to_owned)
                && let Some(query) = context.get_mut("query")
            {
                super::continuations::compact_input(&tool, query);
            }
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
    let locate_targets = query["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|question| clasify::questions::is_locate(&question["question"]))
        .filter_map(|question| {
            Some((
                question["id"].as_str()?,
                question["question"]["target"].as_str()?,
            ))
        })
        .collect::<Vec<_>>();
    let locate_ids = locate_targets.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    // `carry` is the running best from earlier calls of this walk; the merged
    // ranking is file-wide on the final call and travels in next.clasify.
    let best = rank_locate(&rendered, &locate_ids, query.get("carry"));
    if let Some(best) = &best {
        output["best"] = best.clone();
    }
    let hints = locate_targets
        .iter()
        .filter_map(|(_, target)| literal_target_hint(target))
        .collect::<Vec<_>>();
    if !hints.is_empty() {
        output["hints"] = json!(hints);
    }
    output["resources"] = Value::Array(rendered);
    if !continuation_resources.is_empty() {
        let public_questions = query["questions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|question| {
                let mut public = question["question"].clone();
                public["id"] = question["id"].clone();
                public
            })
            .collect::<Vec<_>>();
        output["next"] = json!({"clasify":{
            "id":query["id"],
            "reasoning":query["reasoning"],
            "resources":continuation_resources,
            "questions":public_questions
        }});
        if let Some(best) = best {
            output["next"]["clasify"]["carry"] = best;
        }
    }
    Ok((dispatch::value_result(output), usage_records))
}

fn next_unused_id(prefix: &str, position: usize, used: &mut HashSet<String>) -> String {
    let base = format!("{prefix}-{}", position + 1);
    let mut candidate = base.clone();
    let mut suffix = 2usize;
    while used.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    used.insert(candidate.clone());
    candidate
}

/// Correlation IDs are presentation metadata, not provider input. Derive them
/// after contract validation so callers can omit repetitive bookkeeping while
/// preserving stable keyed output and executable continuations.
fn normalize_ids(queries: &mut [Value]) {
    let mut matrix_ids = queries
        .iter()
        .filter_map(|query| query.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    for (query_index, query) in queries.iter_mut().enumerate() {
        if query.get("id").is_none() {
            query["id"] = json!(next_unused_id("matrix", query_index, &mut matrix_ids));
        }
        if let Some(resources) = query.get_mut("resources").and_then(Value::as_array_mut) {
            let mut used = resources
                .iter()
                .filter_map(|row| row.get("id").and_then(Value::as_str))
                .map(str::to_owned)
                .collect::<HashSet<_>>();
            for (position, resource) in resources.iter_mut().enumerate() {
                if resource.get("id").is_none() {
                    resource["id"] = json!(next_unused_id("resource", position, &mut used));
                }
            }
        }
        if let Some(questions) = query.get_mut("questions").and_then(Value::as_array_mut) {
            let mut used = questions
                .iter()
                .filter_map(|row| row.get("id").and_then(Value::as_str))
                .map(str::to_owned)
                .collect::<HashSet<_>>();
            for (position, row) in questions.iter_mut().enumerate() {
                if row.get("question").is_none() {
                    let mut question = row.as_object().cloned().unwrap_or_default();
                    let id = question
                        .remove("id")
                        .unwrap_or_else(|| json!(next_unused_id("question", position, &mut used)));
                    *row = json!({
                        "id":id,
                        "question":Value::Object(question)
                    });
                } else if row.get("id").is_none() {
                    row["id"] = json!(next_unused_id("question", position, &mut used));
                }
            }
        }
    }
}

pub(super) fn execute(
    queries: &[Value],
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    record_usage: impl FnOnce(ClassificationUsage),
) -> Result<Vec<DomainResult>, ExecutionError> {
    let mut normalized_queries = queries.to_vec();
    normalize_ids(&mut normalized_queries);
    let queries = normalized_queries.as_slice();
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
    fn flat_questions_and_omitted_ids_are_normalized_for_internal_execution() {
        let mut queries = vec![
            json!({
                "reasoning":"Locate facts",
                "resources":[
                    {"context":{"value":"a"}},
                    {"id":"resource-1","context":{"value":"b"}}
                ],
                "questions":[
                    {"type":"noul","instructions":"first"},
                    {"id":"named","type":"noul","instructions":"second"}
                ]
            }),
            json!({
                "id":"matrix-1",
                "reasoning":"Judge state",
                "resources":[{"context":{"value":"c"}}],
                "questions":[{"type":"noul","instructions":"third"}]
            }),
        ];
        normalize_ids(&mut queries);
        assert_eq!(queries[0]["id"], "matrix-1-2");
        assert_eq!(queries[0]["resources"][0]["id"], "resource-1-2");
        assert_eq!(queries[0]["resources"][1]["id"], "resource-1");
        assert_eq!(queries[0]["questions"][0]["id"], "question-1");
        assert_eq!(
            queries[0]["questions"][0]["question"]["instructions"],
            "first"
        );
        assert_eq!(queries[0]["questions"][1]["id"], "named");
        assert_eq!(
            queries[0]["questions"][1]["question"]["instructions"],
            "second"
        );
        assert_eq!(queries[1]["id"], "matrix-1");
        assert_eq!(queries[1]["resources"][0]["id"], "resource-1");
        assert_eq!(queries[1]["questions"][0]["id"], "question-1");
    }

    #[test]
    fn disjoint_file_evidence_keeps_distinct_source_ranges() {
        let state = json!({"results":[{"data":{
            "path":"/tmp/example.rs","content":(0..100).map(|i| format!("line {i}\n")).collect::<String>(),
            "sourceLineRanges":[{"start":4,"end":53},{"start":1000,"end":1049}]
        }}]});
        let evidence = file_evidence(&state).expect("file evidence");
        assert_eq!(evidence["lines"], json!([[4, 53], [1000, 1049]]));
    }

    #[test]
    fn transformed_symbols_are_not_treated_as_original_source_lines() {
        let content = (1..=100)
            .map(|line| format!("{line}| ## Heading\n"))
            .collect::<String>();
        let state = json!({"results":[{"data":{
            "path":"Hooks.md", "content":content,
            "contentView":"symbols", "totalLines":954, "returnedLines":100
        }}]});
        let evidence = file_evidence(&state).expect("file evidence");
        assert!(evidence.get("lines").is_none());
    }

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
    fn code_search_pages_split_into_stable_file_candidates() {
        let state = json!({"results":[{"data":{"files":[
            {"owner":"o","repo":"r","path":"a.rs","matches":[{"value":"alpha"}]},
            {"owner":"O","repo":"R","path":"a.rs","matches":[{"value":"duplicate"}]},
            {"owner":"o","repo":"r","path":"b.rs","matches":[{"value":"beta"}]}
        ],"next":{"nextPage":{"tool":"ghSearchCode","query":{"page":2}}}}}]});
        let code = json!({"tool":"ghSearchCode","query":{}});
        let candidates = search_candidate_states(&code, &state).expect("code candidates");
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0]["results"][0]["data"]["files"][0]["path"],
            "a.rs"
        );
        assert_eq!(
            candidates[1]["results"][0]["data"]["files"][0]["path"],
            "b.rs"
        );
        assert_eq!(
            candidates[0]["results"][0]["data"]["next"], state["results"][0]["data"]["next"],
            "the original search continuation stays available after fan-out"
        );
        let receipt = super::super::clasify_context::candidate_receipt(&code, &candidates[0]);
        assert_eq!(receipt["source"]["path"], "o/r/a.rs");
        let tree = json!({"tool":"ghStructure","query":{}});
        assert!(
            search_candidate_states(&tree, &state).is_none(),
            "tree entries are one discovery page, not code candidates"
        );
    }

    #[test]
    fn hydrated_candidate_reads_are_bounded_and_source_specific() {
        let local = json!({"base":"/repo","results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":90,"value":"needle"}]
        }]}}]});
        let read = local_candidate_read(&local, 12_000).expect("local read");
        assert_eq!(read["tool"], "localFetch");
        assert_eq!(read["query"]["path"], "/repo/src/a.rs");
        assert_eq!(read["query"]["startLine"], 30);
        assert_eq!(read["query"]["endLine"], 150);
        assert_eq!(read["query"]["chunkSize"], 12_000);

        let github = json!({"results":[{"data":{"files":[{
            "owner":"o","repo":"r","path":"src/a.rs","matches":[{
                "value":"fn needle() {}", "matchIndices":[{"start":3,"end":9}]
            }]
        }]}}]});
        let (mut read, anchored) = github_candidate_read(&github, 8_000).expect("github read");
        assert!(anchored);
        assert_eq!(read["query"]["matchString"], "needle");
        assert_eq!(read["query"]["chunkSize"], 8_000);
        let hydrated = json!({"results":[{"data":{"files":[{"commitSha":"abc123"}]}}]});
        pin_github_read(&mut read, &hydrated);
        assert_eq!(read["query"]["branch"], "abc123");
    }

    #[test]
    fn candidate_page_bound_preserves_the_original_search_offset() {
        let first = json!({"tool":"localSearch","query":{
            "reasoning":"find","path":"/repo","searchText":"x","pageSize":20
        }});
        let bounded = bounded_search_source(&first, 5).expect("first page");
        assert_eq!(bounded["query"]["page"], 1);
        assert_eq!(bounded["query"]["pageSize"], 5);

        let aligned = json!({"tool":"ghSearchCode","query":{
            "reasoning":"find","owner":"o","keywords":["x"],
            "page":2,"pageSize":20
        }});
        let bounded = bounded_search_source(&aligned, 5).expect("aligned offset");
        assert_eq!(bounded["query"]["page"], 5);
        assert_eq!(bounded["query"]["pageSize"], 5);

        let unaligned = json!({"tool":"localSearch","query":{
            "reasoning":"find","path":"/repo","searchText":"x","page":2,"pageSize":6
        }});
        assert_eq!(
            bounded_search_source(&unaligned, 5)
                .expect_err("offset cannot be represented")
                .code,
            "classificationExpandedCellsExceeded"
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
            {"path":"b.js","content":"x","startLine":3,"endLine":3,"commitSha":"sha"}]}}]});
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
