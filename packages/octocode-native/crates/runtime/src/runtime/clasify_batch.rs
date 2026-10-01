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
        LocateRead, LocatedPage, bare_identifier, bare_target_hint, collapse_locate_answer,
        drop_redundant_page_reads, literal_search, literal_target_hint, locate_provider_questions,
        located_state, rank_locate, readable_best, with_row_reads,
    },
    clasify_output::{self, PageOutcome},
    dispatch::{self, DomainResult},
    domain_dispatch::DomainDispatcher,
    session_stats::ClassificationUsage,
};
use crate::policy::path::PathPolicy;
use crate::providers::classification::gate::{self, GateLease};
use crate::tools::clasify::{self, transport::ClassificationError};
use crate::tools::id::ToolId;
use futures_util::{StreamExt, stream, stream::FuturesUnordered};
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
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
    MAX_CELLS as MAX_EXPANDED_CELLS, MAX_FILE_CANDIDATES as MAX_HYDRATED_CANDIDATES,
    MAX_FILE_CHUNK_CHARS as MAX_HYDRATED_CHARS, MAX_RESOURCE_CHARS, PREFILTER_WINDOWS,
};
const HYDRATED_LINE_RADIUS: u64 = 60;
#[path = "clasify_items.rs"]
mod items;

#[path = "clasify_budget.rs"]
mod budget;
use budget::{
    Candidate, CaptureBudget, MAX_SHRINK_ATTEMPTS, assessed_payload_chars, budget_candidates,
    budget_spent, candidate_chars, evidence_chars, restore_chunk, resume_search, shrunk_page,
    too_large,
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
    source
        .get("tool")
        .and_then(Value::as_str)
        .is_some_and(clasify::is_file_read_tool)
}

/// A direct file read's own call, pinned to the returned ref when the caller
/// named no branch. Located pages and `best` rows narrow it to one exact
/// window; the receipt keeps it private (`fileRead`), never published whole.
fn file_read_template(source: &Value, receipt: &Value) -> Option<Value> {
    let tool = source["tool"].as_str().filter(|_| is_file_read(source))?;
    let mut query = source
        .get("query")
        .filter(|query| query.is_object())?
        .clone();
    if tool == ToolId::GhGetFileContent.as_str()
        && query.get("branch").is_none()
        && let Some(reference) = receipt.pointer("/source/ref").filter(|r| r.is_string())
    {
        query["branch"] = reference.clone();
    }
    Some(json!({"tool":tool,"query":query}))
}

/// Path-valued fields of local tool envelopes and queries.
const LOCAL_PATH_FIELDS: [&str; 4] = ["base", "path", "uri", "workspaceRoot"];

/// An absolute local path as the provider sees it: workspace-relative, else
/// the file name. `None` leaves non-absolute values (GitHub paths, refs) as is.
fn provider_path(paths: &PathPolicy, value: &str) -> Option<String> {
    let path = Path::new(value.strip_prefix("file://").unwrap_or(value));
    if !path.is_absolute() {
        return None;
    }
    let shown = paths.redact(path);
    if shown.is_empty() || shown == "~" || shown.starts_with("~/") {
        return Some(path.file_name().map_or_else(
            || ".".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        ));
    }
    Some(shown)
}

/// Provider state and read briefs leave the host: absolute local paths
/// (envelope `base`, query `path`/`uri`/`workspaceRoot`) would disclose the
/// user's directory layout to the external classifier.
fn relativize_local_paths(value: &mut Value, paths: &PathPolicy) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields.iter_mut() {
                if LOCAL_PATH_FIELDS.contains(&key.as_str())
                    && let Some(shown) = field.as_str().and_then(|raw| provider_path(paths, raw))
                {
                    *field = Value::String(shown);
                } else {
                    relativize_local_paths(field, paths);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                relativize_local_paths(item, paths);
            }
        }
        _ => {}
    }
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

/// One candidate split from a list page: its row without the list's paging
/// metadata or snippet highlight offsets, which do not inform its verdict.
fn candidate_state(source: &Value, state: Value) -> Value {
    let mut payload = provider_state(source, state);
    let data = if payload.get("base").is_some() {
        &mut payload["data"]
    } else {
        &mut payload
    };
    if let Some(fields) = data.as_object_mut() {
        fields.remove("pagination");
        fields.remove("effectiveQuery");
        for file in fields
            .get_mut("files")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            for matched in file
                .get_mut("matches")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(matched) = matched.as_object_mut() {
                    matched.remove("matchIndices");
                }
            }
        }
    }
    payload
}

fn is_candidate_search(source: &Value) -> bool {
    source
        .get("tool")
        .and_then(Value::as_str)
        .is_some_and(clasify::is_candidate_search_tool)
}

fn file_chunks(source: &Value) -> bool {
    clasify::candidate_evidence(source) == Some(clasify::CandidateEvidence::FileChunks)
}

fn candidate_identity(source: &Value, file: &Value) -> Option<String> {
    let path = file.get("path")?.as_str()?;
    match source
        .get("tool")
        .and_then(Value::as_str)
        .and_then(ToolId::from_name)?
    {
        // Rows are workspace-relative (base is the workspace root) or
        // absolute outside it, so either form reads back through localFetch.
        ToolId::LocalSearch => Some(path.to_owned()),
        ToolId::GhSearchCode => Some(format!(
            "{}/{}/{}",
            file.get("owner")?.as_str()?.to_ascii_lowercase(),
            file.get("repo")?.as_str()?.to_ascii_lowercase(),
            path
        )),
        _ => None,
    }
}

/// Turn one lexical search page into independent, path-deduplicated files.
#[cfg(test)]
fn search_candidate_states(source: &Value, state: &Value) -> Option<Vec<Value>> {
    positioned_search_candidates(source, state)
        .map(|candidates| candidates.into_iter().map(|(_, state)| state).collect())
}

/// Search candidates with the position of their file row on the page.
fn positioned_search_candidates(source: &Value, state: &Value) -> Option<Vec<(usize, Value)>> {
    if !is_candidate_search(source) {
        return None;
    }
    let files = state.pointer("/results/0/data/files")?.as_array()?;
    // A repo-scoped ghSearchCode page names owner/repo once, on `data`, or
    // (minimized as a request echo) only in the search query itself.
    let field = |name: &str| {
        state
            .pointer(&format!("/results/0/data/{name}"))
            .or_else(|| source.pointer(&format!("/query/{name}")))
            .cloned()
    };
    let page_repo = Some((field("owner"), field("repo")));
    let mut seen = HashSet::new();
    let candidates = files
        .iter()
        .enumerate()
        .filter_map(|(position, file)| {
            let file = if let Some(row) = file.as_str() {
                if source["tool"] == ToolId::GhSearchCode.as_str() {
                    let (repo, path) = row.split_once(':')?;
                    let (owner, repo) = repo.split_once('/')?;
                    json!({"owner":owner, "repo":repo, "path":path})
                } else {
                    json!({"path":row})
                }
            } else {
                let mut file = file.clone();
                if source["tool"] == ToolId::GhSearchCode.as_str()
                    && let (Some(object), Some((Some(owner), Some(repo)))) =
                        (file.as_object_mut(), page_repo.clone())
                {
                    object.entry("owner").or_insert(owner);
                    object.entry("repo").or_insert(repo);
                }
                file
            };
            candidate_identity(source, &file)
                .is_some_and(|id| seen.insert(id))
                .then_some((position, file))
        })
        .map(|(position, file)| {
            let mut candidate = state.clone();
            candidate["results"][0]["data"]["files"] = json!([file]);
            (position, candidate)
        })
        .collect::<Vec<_>>();
    (!candidates.is_empty()).then_some(candidates)
}

/// Every hit line of one local candidate: its shown rows plus the rows a
/// clipped file names in `pagination.moreLines`.
fn candidate_hit_lines(file: &Value) -> Vec<u64> {
    let shown = file
        .get("matches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|matched| matched.get("line").and_then(Value::as_u64));
    // `moreLines` names runs (`711-717,802`) and may end with a count of
    // omitted lines (`+40 more`), which names no line.
    let more = file
        .pointer("/pagination/moreLines")
        .and_then(Value::as_str)
        .into_iter()
        .flat_map(|lines| lines.split(','))
        .flat_map(|run| {
            let (start, end) = run
                .trim()
                .split_once('-')
                .unwrap_or((run.trim(), run.trim()));
            match (start.parse::<u64>(), end.parse::<u64>()) {
                (Ok(start), Ok(end)) => Some(start..=end),
                _ => None,
            }
            .into_iter()
            .flatten()
        });
    let mut lines = shown.chain(more).collect::<Vec<_>>();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// Contiguous windows (`2 × HYDRATED_LINE_RADIUS + 1` lines) covering every
/// hit cluster, densest first: each centers on the densest remaining run of
/// hits (an incidental first hit must not pull a window off its cluster), and
/// the hits it covers leave the pool. No hits yields the opening window.
fn hit_cluster_windows(mut lines: Vec<u64>) -> Vec<(u64, u64)> {
    let mut windows = Vec::new();
    while let Some((first, last)) = densest_run(&lines, HYDRATED_LINE_RADIUS * 2) {
        let center = first + (last - first) / 2;
        let start = center.saturating_sub(HYDRATED_LINE_RADIUS).max(1);
        let end = center.saturating_add(HYDRATED_LINE_RADIUS);
        windows.push((start, end));
        lines.retain(|line| *line < start || *line > end);
    }
    if windows.is_empty() {
        windows.push((1, HYDRATED_LINE_RADIUS * 2 + 1));
    }
    windows
}

/// Windows judged per candidate within `budget` pages: every candidate gets
/// its densest cluster first, then further clusters go round-robin, so a
/// deciding line far from the densest cluster is still judged.
fn allocate_windows(clusters: &[usize], budget: usize) -> Vec<usize> {
    let mut taken = clusters
        .iter()
        .map(|count| (*count).min(1))
        .collect::<Vec<_>>();
    let mut left = budget.saturating_sub(taken.iter().sum());
    let mut round = 1;
    while left > 0 && clusters.iter().any(|count| *count > round) {
        for (index, count) in clusters.iter().enumerate() {
            if left > 0 && *count > round {
                taken[index] += 1;
                left -= 1;
            }
        }
        round += 1;
    }
    taken
}

/// Longest span of merged windows: three windows' lines.
const MAX_MERGED_WINDOW_LINES: u64 = 3 * (HYDRATED_LINE_RADIUS * 2 + 1);

/// Sorted windows of one file, joined when they overlap or the gap between
/// them is at most one window radius and the joined span stays within
/// [`MAX_MERGED_WINDOW_LINES`]: one contiguous page judges both clusters in
/// one provider call instead of two (and never judges overlap lines twice).
fn merge_near_windows(windows: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(windows.len());
    for (start, end) in windows {
        match merged.last_mut() {
            Some(last)
                if start <= last.1 + 1 + HYDRATED_LINE_RADIUS
                    && end.max(last.1) - last.0 < MAX_MERGED_WINDOW_LINES =>
            {
                last.1 = last.1.max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn local_window_read(path: &str, (start, end): (u64, u64), max_bytes: usize) -> Value {
    json!({
        "tool":ToolId::LocalFetch.as_str(),
        "query":{
            "reasoning":"Read a bounded search candidate for classification.",
            "path":path,
            "startLine":start,
            "endLine":end,
            "chunkType":"bytes",
            "chunkSize":max_bytes,
            "minify":"none"
        }
    })
}

/// One bounded read per hit cluster of a local candidate, densest first.
fn local_candidate_reads(candidate: &Value, max_bytes: usize) -> Option<Vec<Value>> {
    let file = candidate.pointer("/results/0/data/files/0")?;
    let path = candidate_identity(&json!({"tool":ToolId::LocalSearch.as_str()}), file)?;
    Some(
        hit_cluster_windows(candidate_hit_lines(file))
            .into_iter()
            .map(|window| local_window_read(&path, window, max_bytes))
            .collect(),
    )
}

fn local_candidate_read(candidate: &Value, max_bytes: usize) -> Option<Value> {
    local_candidate_reads(candidate, max_bytes)?
        .into_iter()
        .next()
}

/// Center of the densest run of match lines that fits one hydrated window.
fn densest_match_line(mut lines: Vec<u64>) -> Option<u64> {
    lines.sort_unstable();
    let (first, last) = densest_run(&lines, HYDRATED_LINE_RADIUS * 2)?;
    Some(first + (last - first) / 2)
}

/// First and last line of the longest run of sorted `lines` spanning at most
/// `width` lines.
fn densest_run(lines: &[u64], width: u64) -> Option<(u64, u64)> {
    let mut best = (0, 0);
    let mut first = 0;
    for last in 0..lines.len() {
        while lines[last] - lines[first] > width {
            first += 1;
        }
        if last - first > best.1 - best.0 {
            best = (first, last);
        }
    }
    Some((*lines.get(best.0)?, *lines.get(best.1)?))
}

fn utf16_slice(value: &str, start: usize, end: usize) -> Option<String> {
    let text = value.encode_utf16().collect::<Vec<_>>();
    String::from_utf16(text.get(start..end)?).ok()
}

/// Longest matched line used verbatim as a read anchor; longer (minified)
/// lines fall back to the matched term.
const MAX_ANCHOR_LINE_CHARS: usize = 160;

/// Literal anchor for one GitHub snippet: the line holding the most matched
/// terms, trimmed. A bare keyword recurs across the file, so anchoring on it
/// reads every occurrence instead of the hit.
fn github_match_anchor(matched: &Value) -> Option<String> {
    let value = matched.get("value")?.as_str()?;
    let ranges = matched.get("matchIndices")?.as_array()?;
    let line_of = |range: &Value| -> Option<usize> {
        if let Some(offset) = range.get("lineOffset").and_then(Value::as_u64) {
            return usize::try_from(offset).ok();
        }
        let start = usize::try_from(range.get("start")?.as_u64()?).ok()?;
        Some(utf16_slice(value, 0, start)?.matches('\n').count())
    };
    let mut counts = Vec::<(usize, usize)>::new();
    for line in ranges.iter().filter_map(line_of) {
        match counts.iter_mut().find(|(seen, _)| *seen == line) {
            Some((_, count)) => *count += 1,
            None => counts.push((line, 1)),
        }
    }
    let densest = counts
        .iter()
        .fold(None::<(usize, usize)>, |best, &(line, count)| match best {
            Some((_, top)) if top >= count => best,
            _ => Some((line, count)),
        })
        .map(|(line, _)| line);
    let line = densest
        .and_then(|line| value.split('\n').nth(line))
        .map(str::trim)
        .filter(|line| !line.is_empty() && line.chars().count() <= MAX_ANCHOR_LINE_CHARS);
    if let Some(line) = line {
        return Some(line.to_owned());
    }
    let range = ranges.first()?;
    let start = usize::try_from(range.get("start")?.as_u64()?).ok()?;
    let end = usize::try_from(range.get("end")?.as_u64()?).ok()?;
    utf16_slice(value, start, end).filter(|value| !value.trim().is_empty())
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
    // A line-resolved row (`"N\ttext"`) anchors on its first hit line.
    let line_anchor = file
        .get("lines")
        .and_then(Value::as_array)
        .and_then(|lines| lines.first())
        .and_then(Value::as_str)
        .and_then(|line| line.split_once(crate::runtime::numbered::SEPARATOR))
        .map(|(_, text)| text.trim())
        .filter(|text| !text.is_empty() && text.chars().count() <= MAX_ANCHOR_LINE_CHARS)
        .map(str::to_owned);
    let anchor = line_anchor.or_else(|| {
        file.get("matches")
            .and_then(Value::as_array)
            .and_then(|matches| matches.first())
            .and_then(github_match_anchor)
    });
    let anchored = anchor.is_some();
    if let Some(anchor) = anchor {
        query["matchString"] = json!(anchor);
        query["contextLines"] = json!(20);
    } else {
        query["startLine"] = json!(1);
        query["endLine"] = json!(HYDRATED_LINE_RADIUS * 2 + 1);
    }
    Some((
        json!({"tool":ToolId::GhGetFileContent.as_str(),"query":query}),
        anchored,
    ))
}

fn candidate_read(source: &Value, candidate: &Value, max_bytes: usize) -> Option<(Value, bool)> {
    let (mut read, anchored) = match source
        .get("tool")
        .and_then(Value::as_str)
        .and_then(ToolId::from_name)
    {
        Some(ToolId::LocalSearch) => (local_candidate_read(candidate, max_bytes)?, true),
        Some(ToolId::GhSearchCode) => github_candidate_read(candidate, max_bytes)?,
        _ => return None,
    };
    inherit_search_goal(&mut read, source);
    Some((read, anchored))
}

/// The hydrated read is a new tool call. It keeps the search brief so the
/// required goal is the decision the search was opened for.
fn inherit_search_goal(read: &mut Value, source: &Value) {
    if read
        .pointer("/query/goal")
        .and_then(Value::as_str)
        .is_none_or(|text| text.trim().is_empty())
        && let Some(goal) = source
            .pointer("/query/goal")
            .filter(|value| value.as_str().is_some_and(|text| !text.trim().is_empty()))
    {
        read["query"]["goal"] = goal.clone();
    }
}

fn pin_github_read(read: &mut Value, state: &Value) {
    if read.get("tool").and_then(Value::as_str) != Some(ToolId::GhGetFileContent.as_str()) {
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
    if source.get("tool").and_then(Value::as_str) == Some(ToolId::GhSearchCode.as_str()) {
        return 30;
    }
    match source.pointer("/query/resultView").and_then(Value::as_str) {
        Some("files" | "filesWithout" | "discovery" | "countLines" | "countMatches") => 100,
        _ => 20,
    }
}

/// Bound candidate fan-out before executing search. The rewritten page size
/// divides the original starting offset, so the page starts at the same file.
fn bounded_search_source(
    source: &Value,
    candidate_limit: usize,
) -> Result<Value, ClassificationError> {
    let list = items::is_paged_list(source);
    if !(is_candidate_search(source) || list) || candidate_limit == 0 {
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
    let page = query.get("page").and_then(Value::as_u64).unwrap_or(1);
    let original_size = match query.get("pageSize").and_then(Value::as_u64) {
        Some(size) => size,
        // A list tool's first page starts at offset 0 under any page size.
        None if list && page <= 1 => u64::MAX,
        // A later page with an unknown default cannot be re-paged safely;
        // the expanded-cell check still bounds provider work.
        None if list => return Ok(bounded),
        None => default_search_page_size(source),
    };
    let mut limit = u64::try_from(candidate_limit).unwrap_or(u64::MAX);
    if original_size <= limit {
        return Ok(bounded);
    }
    let offset = page.saturating_sub(1).saturating_mul(original_size);
    if offset % limit != 0 {
        // A smaller divisor starts at the same file while fitting the cell
        // budget. Rounding the page number would skip or repeat candidates.
        while offset % limit != 0 {
            limit -= 1;
        }
    }
    query.insert("pageSize".into(), json!(limit));
    // artifactSearch pages by cursor and has no page field to rewrite.
    if source.get("tool").and_then(Value::as_str) != Some(ToolId::ArtifactSearch.as_str()) {
        query.insert("page".into(), json!(offset / limit + 1));
    }
    Ok(bounded)
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

/// The read a host runs for a kept search candidate: the same anchored
/// window hydration would judge, without the provider byte budget.
fn host_read(source: &Value, candidate: &Value) -> Option<Value> {
    let (mut read, _) = candidate_read(source, candidate, MAX_HYDRATED_CHARS)?;
    let query = read.get_mut("query")?.as_object_mut()?;
    query.remove("chunkType");
    query.remove("chunkSize");
    // The matrix brief is copied on by the response stage.
    query.remove("goal");
    query.remove("reasoning");
    read["confidence"] = json!("high");
    Some(read)
}

/// One list candidate as its own page: narrowed evidence, its identity, and
/// the read that fetches it.
fn item_page(source: &Value, item: items::Item) -> CapturedPage {
    let mut context = super::clasify_context::candidate_receipt(source, &item.state);
    if let Some(receipt_source) = context.get_mut("source").and_then(Value::as_object_mut) {
        if let Some(path) = item.path {
            receipt_source.insert("path".into(), json!(path));
        }
        if let Some(identity) = item.item {
            receipt_source.insert("item".into(), json!(identity));
        }
    }
    if let Some(read) = item.read {
        super::clasify_context::attach_read(&mut context, read);
    }
    CapturedPage::Ready {
        state: candidate_state(source, item.state),
        context,
    }
}

/// One candidate read judged as one page. With `whole`, a read the byte
/// budget cut short yields `None` so the caller judges its parts instead.
fn hydrate_candidate(
    source: &Value,
    candidate: Value,
    mut read: Value,
    anchored: bool,
    whole: bool,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> Option<CapturedPage> {
    Some(
        match super::clasify_context::resolve(&read, dispatcher, execution) {
            Ok((hydrated_state, hydrated_receipt)) => {
                if whole
                    && hydrated_receipt
                        .as_ref()
                        .and_then(super::clasify_context::continuation)
                        .is_some()
                {
                    return None;
                }
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
                let mut context = failure.receipt.unwrap_or_else(|| {
                    super::clasify_context::candidate_receipt(source, &candidate)
                });
                super::clasify_context::append_limitation(
                    &mut context,
                    "Candidate hydration failed; classification was not run for this file.",
                );
                CapturedPage::Failed {
                    error: failure.error,
                    context,
                }
            }
        },
    )
}

/// One hydration: a read judged as one page, or a merged span of several
/// cluster windows (`parts`) judged whole when it fits one bounded page and
/// window by window otherwise.
struct HydrationJob {
    read: Value,
    anchored: bool,
    parts: Vec<Value>,
}

impl HydrationJob {
    fn single((read, anchored): (Value, bool)) -> Self {
        Self {
            read,
            anchored,
            parts: Vec::new(),
        }
    }

    fn run(
        self,
        source: &Value,
        candidate: &Value,
        dispatcher: &DomainDispatcher,
        execution: &ExecutionContext,
    ) -> Vec<CapturedPage> {
        let hydrate = |read, whole| {
            hydrate_candidate(
                source,
                candidate.clone(),
                read,
                self.anchored,
                whole,
                dispatcher,
                execution,
            )
        };
        if let Some(page) = hydrate(self.read.clone(), !self.parts.is_empty()) {
            return vec![page];
        }
        self.parts
            .iter()
            .filter_map(|part| hydrate(part.clone(), false))
            .collect()
    }
}

/// Reads for each candidate: local candidates get one window per hit
/// cluster within `page_budget` pages in all; others get their one read.
fn candidate_jobs(
    source: &Value,
    candidates: &[Value],
    max_bytes: usize,
    page_budget: usize,
) -> Vec<Option<Vec<HydrationJob>>> {
    if source.get("tool").and_then(Value::as_str) != Some(ToolId::LocalSearch.as_str()) {
        return candidates
            .iter()
            .map(|candidate| {
                candidate_read(source, candidate, max_bytes)
                    .map(|read| vec![HydrationJob::single(read)])
            })
            .collect();
    }
    let windows = candidates
        .iter()
        .map(|candidate| {
            let file = candidate.pointer("/results/0/data/files/0")?;
            let path = candidate_identity(&json!({"tool":ToolId::LocalSearch.as_str()}), file)?;
            Some((path, hit_cluster_windows(candidate_hit_lines(file))))
        })
        .collect::<Vec<_>>();
    let clusters = windows
        .iter()
        .map(|entry| entry.as_ref().map_or(0, |(_, windows)| windows.len()))
        .collect::<Vec<_>>();
    let taken = allocate_windows(&clusters, page_budget);
    windows
        .into_iter()
        .zip(taken)
        .map(|(entry, taken)| {
            let (path, mut windows) = entry?;
            windows.truncate(taken.max(1));
            // Judge a file's windows in source order.
            windows.sort_unstable();
            let read = |window| {
                let mut read = local_window_read(&path, window, max_bytes);
                inherit_search_goal(&mut read, source);
                read
            };
            Some(
                merge_near_windows(windows.clone())
                    .into_iter()
                    .map(|span| {
                        let parts = windows
                            .iter()
                            .filter(|window| span.0 <= window.0 && window.1 <= span.1)
                            .map(|window| read(*window))
                            .collect::<Vec<_>>();
                        HydrationJob {
                            read: read(span),
                            anchored: true,
                            parts: if parts.len() > 1 { parts } else { Vec::new() },
                        }
                    })
                    .collect(),
            )
        })
        .collect()
}

/// Hydrate candidates with `budget` characters shared by every read: each of
/// the planned reads is bounded to an equal share.
fn hydrate_candidates(
    source: &Value,
    candidates: Vec<Value>,
    budget: usize,
    page_budget: usize,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Vec<CapturedPage>, ExecutionError> {
    let planned = candidate_jobs(source, &candidates, 1, page_budget)
        .iter()
        .flatten()
        .map(Vec::len)
        .sum::<usize>();
    let max_bytes = budget
        .checked_div(planned.max(1))
        .unwrap_or(budget)
        .clamp(1, MAX_HYDRATED_CHARS);
    let jobs = candidate_jobs(source, &candidates, max_bytes, page_budget);
    std::thread::scope(|scope| {
        let mut completed = Vec::with_capacity(candidates.len());
        let mut tasks = Vec::with_capacity(candidates.len());
        for (index, (candidate, job)) in candidates.into_iter().zip(jobs).enumerate() {
            execution.check()?;
            let Some(job) = job else {
                completed.push((
                    (index, 0),
                    vec![CapturedPage::Failed {
                        error: ClassificationError::new(
                            "classificationCandidateUnhydratable",
                            "A search result did not contain a usable file identity.",
                            "Run the search directly and inspect the malformed candidate.",
                        ),
                        context: super::clasify_context::candidate_receipt(source, &candidate),
                    }],
                ));
                continue;
            };
            for (window, job) in job.into_iter().enumerate() {
                // Acquire before spawning so at most the permitted number of
                // blocking workers exists; later reads wait in this loop.
                let call_permit = reads.acquire(execution)?;
                let process_permit = PROCESS_READS.acquire(execution)?;
                let candidate = candidate.clone();
                tasks.push((
                    (index, window),
                    scope.spawn(move || {
                        let (_call_permit, _process_permit) = (call_permit, process_permit);
                        job.run(source, &candidate, dispatcher, execution)
                    }),
                ));
            }
        }
        for (key, task) in tasks {
            completed.push((key, task.join().map_err(|_| ExecutionError::WorkerFailed)?));
        }
        completed.sort_by_key(|(key, _)| *key);
        Ok(completed.into_iter().flat_map(|(_, pages)| pages).collect())
    })
}

/// A resource's `maxChars`; the contract default is `maxResourceChars`.
fn max_chars(resource: &Value) -> usize {
    resource
        .get("maxChars")
        .and_then(Value::as_u64)
        .map_or(MAX_RESOURCE_CHARS, |chars| chars as usize)
}

fn capture_resource(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
    candidate_limit: usize,
) -> Result<(Vec<CapturedPage>, Option<Value>), ExecutionError> {
    if let Some(plan) = prefilter_windows(resource, dispatcher, execution, reads)? {
        return capture_prefiltered(
            resource,
            &plan,
            dispatcher,
            execution,
            reads,
            candidate_limit,
        );
    }
    let (pages, remaining, _) = capture_pages(
        resource,
        dispatcher,
        execution,
        reads,
        candidate_limit,
        CaptureBudget::whole(max_chars(resource)),
    )?;
    Ok((pages, remaining))
}

/// Capture one resource's pages within its budget. Returns the pages, the
/// resource continuation, and the characters captured.
fn capture_pages(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
    candidate_limit: usize,
    budget: CaptureBudget,
) -> Result<(Vec<CapturedPage>, Option<Value>, usize), ExecutionError> {
    let CaptureBudget {
        left: max_chars,
        cap,
        defer: defer_oversize,
    } = budget;
    let requested_source = resource["context"].clone();
    let hydrated = file_chunks(&requested_source);
    // Pages this resource may judge (one per candidate, or per hit cluster
    // of a hydrated local candidate) before the matrix cell budget binds.
    let page_budget = candidate_limit;
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
                0,
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
    // The page size a shrunk page's continuation returns to.
    let mut walk_chunk: Option<Value> = None;

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
                let remaining_chars = max_chars.saturating_sub(captured_chars);
                if let Some(candidates) = positioned_search_candidates(&source, &state) {
                    let mut next = if hydrated
                        && source.get("tool").and_then(Value::as_str)
                            == Some(ToolId::LocalSearch.as_str())
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
                        let candidates = candidates
                            .into_iter()
                            .map(|(position, candidate)| {
                                let mut context =
                                    super::clasify_context::candidate_receipt(&source, &candidate);
                                if let Some(read) = host_read(&source, &candidate) {
                                    super::clasify_context::attach_read(&mut context, read);
                                }
                                let state = candidate_state(&source, candidate);
                                Candidate {
                                    chars: candidate_chars(&state),
                                    page: CapturedPage::Ready { state, context },
                                    position: Some(position),
                                }
                            })
                            .collect();
                        let (kept, chars, deferred) =
                            budget_candidates(candidates, remaining_chars, cap);
                        captured_chars = captured_chars.saturating_add(chars);
                        pages.extend(kept);
                        remaining = match deferred.first().and_then(|first| first.position) {
                            None => next,
                            Some(position) => {
                                match resume_search(&source, receipt.as_ref(), position) {
                                    Some(resume) => Some(resume),
                                    None => {
                                        pages.extend(deferred.into_iter().map(|candidate| {
                                            budget_spent(candidate, remaining_chars)
                                        }));
                                        next
                                    }
                                }
                            }
                        };
                        break;
                    }

                    let candidates = candidates
                        .into_iter()
                        .map(|(_, candidate)| candidate)
                        .collect();
                    let hydrated_pages = hydrate_candidates(
                        &source,
                        candidates,
                        remaining_chars,
                        page_budget,
                        dispatcher,
                        execution,
                        reads,
                    )?;
                    let hydrated_pages = hydrated_pages
                        .into_iter()
                        .map(|page| Candidate {
                            chars: match &page {
                                CapturedPage::Ready { state, .. } => evidence_chars(state),
                                CapturedPage::Failed { .. } => 0,
                            },
                            page,
                            position: None,
                        })
                        .collect();
                    let (kept, chars, deferred) =
                        budget_candidates(hydrated_pages, remaining_chars, cap);
                    captured_chars = captured_chars.saturating_add(chars);
                    pages.extend(kept);
                    pages.extend(
                        deferred
                            .into_iter()
                            .map(|candidate| budget_spent(candidate, remaining_chars)),
                    );
                    remaining = next;
                    break;
                }
                let (state, outline_next) = if items::is_symbol_outline(&source, &state) {
                    whole_outline_files(
                        &source,
                        state,
                        receipt.as_ref(),
                        dispatcher,
                        execution,
                        reads,
                    )?
                } else {
                    (state, None)
                };
                // Split only when every candidate fits the cell budget; a
                // larger list falls through and is judged as one page.
                if let Some(items) = items::split(&source, &state)
                    .filter(|items| items.len() <= candidate_limit.max(1))
                {
                    remaining = outline_next.unwrap_or_else(|| {
                        receipt
                            .as_ref()
                            .and_then(super::clasify_context::continuation)
                    });
                    let items = items
                        .into_iter()
                        .map(|item| {
                            let page = item_page(&source, item);
                            Candidate {
                                chars: match &page {
                                    CapturedPage::Ready { state, .. } => candidate_chars(state),
                                    CapturedPage::Failed { .. } => 0,
                                },
                                page,
                                position: None,
                            }
                        })
                        .collect();
                    let (kept, chars, deferred) = budget_candidates(items, remaining_chars, cap);
                    captured_chars = captured_chars.saturating_add(chars);
                    pages.extend(kept);
                    pages.extend(
                        deferred
                            .into_iter()
                            .map(|candidate| budget_spent(candidate, remaining_chars)),
                    );
                    break;
                }
                let (mut state, mut receipt) = (state, receipt);
                let mut state_chars = assessed_payload_chars(&source, &state);
                // A page over the whole cap would fail on every replay; read
                // the same start in smaller chunks until it fits.
                let mut attempts = 0;
                while state_chars > cap && attempts < MAX_SHRINK_ATTEMPTS {
                    let Some(smaller) = shrunk_page(&source, &state, state_chars, cap) else {
                        break;
                    };
                    attempts += 1;
                    let Ok((shrunk_state, shrunk_receipt)) =
                        resolve_limited(&smaller, dispatcher, execution, reads)?
                    else {
                        break;
                    };
                    if walk_chunk.is_none() {
                        walk_chunk = state
                            .pointer("/results/0/data/pagination/chunkSize")
                            .or_else(|| {
                                state.pointer("/results/0/data/files/0/pagination/chunkSize")
                            })
                            .cloned();
                    }
                    seen.insert(smaller.to_string());
                    source = smaller;
                    state = shrunk_state;
                    receipt = shrunk_receipt;
                    state_chars = assessed_payload_chars(&source, &state);
                }
                if state_chars <= cap
                    && state_chars > remaining_chars
                    && (defer_oversize || !pages.is_empty())
                {
                    remaining = Some(source);
                    break;
                }
                let mut context = receipt.unwrap_or_else(|| fallback_context(&source));
                if context.get("read").is_none()
                    && let Some(template) = file_read_template(&source, &context)
                {
                    context["fileRead"] = template;
                }
                // Never classify an arbitrary prefix with the full page's
                // source receipt. The caller can choose a smaller complete
                // section; no continuation replays a page that cannot fit.
                if state_chars > remaining_chars {
                    pages.push(CapturedPage::Failed {
                        error: too_large(state_chars, cap),
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
                // An outline page that read ahead for its last file resumes
                // after those rows, not at its own next page.
                if let Some(next) = outline_next {
                    pages.push(CapturedPage::Ready { state, context });
                    remaining = next;
                    break;
                }
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
                let next = restore_chunk(next, walk_chunk.as_ref());
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
    Ok((pages, remaining, captured_chars))
}

/// Extra outline pages one call may read to finish its last file.
const MAX_OUTLINE_FOLLOW_PAGES: usize = 20;

/// Page a symbols outline by file: a file's rows are judged together, on the
/// page that holds its first row. That page reads the following pages for
/// the rest of its last file and resumes after them; the resumed page skips
/// the rows of the file an earlier page started. Returns the page state and,
/// when following pages were read, the continuation that replaces the page's
/// own (`Some(None)`: the outline is exhausted).
#[allow(clippy::type_complexity)]
fn whole_outline_files(
    source: &Value,
    mut state: Value,
    receipt: Option<&Value>,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<(Value, Option<Option<Value>>), ExecutionError> {
    let page = source
        .pointer("/query/page")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    if page > 1 {
        let mut previous = source.clone();
        previous["query"]["page"] = json!(page - 1);
        if let Ok((prior, _)) = resolve_limited(&previous, dispatcher, execution, reads)?
            && items::last_outline_path(&prior).is_some()
            && items::last_outline_path(&prior) == items::first_outline_path(&state)
        {
            items::drop_first_outline_file(&mut state);
        }
    }
    let mut next = receipt.and_then(super::clasify_context::continuation);
    let mut followed = false;
    for _ in 0..MAX_OUTLINE_FOLLOW_PAGES {
        let Some(candidate) = next.clone() else {
            break;
        };
        let Ok((following, following_receipt)) =
            resolve_limited(&candidate, dispatcher, execution, reads)?
        else {
            break;
        };
        match items::extend_last_outline_file(&mut state, &following) {
            // The last file ends inside that page: resume there.
            Some(true) => {
                followed = true;
                break;
            }
            // That page held only the last file: resume after it.
            Some(false) => {
                followed = true;
                next = following_receipt
                    .as_ref()
                    .and_then(super::clasify_context::continuation);
            }
            // The next page starts a new file: the page's own continuation.
            None => break,
        }
    }
    Ok((state, followed.then_some(next)))
}

fn page_context(page: &CapturedPage) -> &Value {
    match page {
        CapturedPage::Ready { context, .. } | CapturedPage::Failed { context, .. } => context,
    }
}

fn page_context_mut(page: &mut CapturedPage) -> &mut Value {
    match page {
        CapturedPage::Ready { context, .. } | CapturedPage::Failed { context, .. } => context,
    }
}

/// Judge prefilter windows in file order under one shared `maxChars` budget.
/// Hits the windows do not reach (more hit clusters than windows, an
/// unfinished probe, or a spent budget) get a limitation and a `next.clasify`
/// continuation: the same prefiltered read from the first unjudged line.
fn capture_prefiltered(
    resource: &Value,
    plan: &PrefilterPlan,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
    candidate_limit: usize,
) -> Result<(Vec<CapturedPage>, Option<Value>), ExecutionError> {
    let budget = max_chars(resource);
    let mut used = 0usize;
    let mut pages = Vec::new();
    let mut resume = plan.resume;
    for &(start, end) in &plan.windows {
        let left = budget.saturating_sub(used);
        if left == 0 {
            resume = Some(start);
            break;
        }
        let window = prefilter_window(resource, start, end);
        let (window_pages, next, chars) = capture_pages(
            &window,
            dispatcher,
            execution,
            reads,
            candidate_limit,
            CaptureBudget {
                left,
                cap: budget,
                defer: !pages.is_empty(),
            },
        )?;
        used = used.saturating_add(chars);
        let covered = window_pages
            .iter()
            .filter_map(|page| page_context(page)["scope"]["endLine"].as_u64())
            .max();
        pages.extend(window_pages);
        if next.is_some() {
            resume = Some(covered.map_or(start, |line| line.saturating_add(1).max(start)));
            break;
        }
    }
    let resume = resume.filter(|from| {
        *from <= plan.range_end && (plan.probe_open || plan.hits.iter().any(|hit| hit >= from))
    });
    if let Some(from) = resume
        && let Some(last) = pages.last_mut()
    {
        super::clasify_context::append_limitation(
            page_context_mut(last),
            &format!(
                "Prefilter windows stopped before line {from}; later hits are unjudged. next.clasify resumes there."
            ),
        );
    }
    Ok((
        pages,
        resume.map(|from| prefilter_resume(resource, from, plan.range_end)),
    ))
}

/// Lines per prefilter window (contract `prefilterWindowLines`, as a line number).
const PREFILTER_WINDOW_LINES: u64 = crate::tools::id::clasify_policy::PREFILTER_WINDOW_LINES as u64;
/// Match pages the prefilter probe follows before it stops collecting hits.
const PREFILTER_PROBE_PAGES: usize = 20;

/// Windows one prefiltered call judges, and where the walk resumes.
struct PrefilterPlan {
    /// Inclusive line windows in file order, non-overlapping.
    windows: Vec<(u64, u64)>,
    /// Every known hit in range, sorted.
    hits: Vec<u64>,
    /// Last line of the walk: the caller's `endLine`, else the file end.
    range_end: u64,
    /// First line after the windows that may still hold unjudged hits.
    resume: Option<u64>,
    /// The probe stopped before its last match page: hits past the known
    /// ones may exist.
    probe_open: bool,
}

/// `prefilter` on a file resource: read the same file for the literal terms,
/// then capture only hit windows as bounded reads instead of the whole file.
/// When the hits fit in `PREFILTER_WINDOWS` windows they are the densest runs;
/// otherwise windows follow file order and `resume` continues after the last
/// one. A caller `startLine`/`endLine` bounds the walk (continuations use it).
/// `None` (no prefilter, not a file read, or no hits) captures the resource
/// as given.
fn prefilter_windows(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    reads: &ReadLimiter,
) -> Result<Option<PrefilterPlan>, ExecutionError> {
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
    if terms.is_empty() || !clasify::is_file_read_tool(tool) {
        return Ok(None);
    }
    let mut query = resource["context"]["query"].clone();
    let Some(object) = query.as_object_mut() else {
        return Ok(None);
    };
    let range_start = object
        .get("startLine")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1);
    let requested_end = object.get("endLine").and_then(Value::as_u64);
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
    let pattern = terms.join("|");
    object.insert("matchString".into(), json!(pattern));
    object.insert("matchStringIsRegex".into(), json!(true));
    object.insert("matchStringCaseSensitive".into(), json!(false));
    object.insert("contextLines".into(), json!(0));
    let mut probe = json!({"tool":tool,"query":query});
    let mut total = None;
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    let mut probe_open = false;
    for page in 0..=PREFILTER_PROBE_PAGES {
        if page == PREFILTER_PROBE_PAGES || !seen.insert(probe.to_string()) {
            probe_open = page == PREFILTER_PROBE_PAGES;
            break;
        }
        let Ok((_, Some(receipt))) = resolve_limited(&probe, dispatcher, execution, reads)? else {
            if page == 0 {
                return Ok(None);
            }
            probe_open = true;
            break;
        };
        let scope = &receipt["scope"];
        let Some(lines) = scope["totalLines"].as_u64() else {
            if page == 0 {
                return Ok(None);
            }
            probe_open = true;
            break;
        };
        total = Some(lines);
        let ranges = scope["lineRanges"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![scope.clone()]);
        hits.extend(
            ranges
                .iter()
                .filter_map(|range| Some(range["startLine"].as_u64()?..=range["endLine"].as_u64()?))
                .flatten(),
        );
        // Follow only match pages of the same probe; any other continuation
        // (e.g. a plain read) would count unmatched lines as hits.
        match super::clasify_context::continuation(&receipt) {
            Some(next)
                if next["tool"] == tool && next["query"]["matchString"] == json!(pattern) =>
            {
                probe = next;
            }
            Some(_) => {
                probe_open = true;
                break;
            }
            None => break,
        }
    }
    let Some(total) = total else {
        return Ok(None);
    };
    let range_end = requested_end.unwrap_or(total).min(total);
    hits.retain(|line| (range_start..=range_end).contains(line));
    hits.sort_unstable();
    hits.dedup();
    if hits.is_empty() {
        return Ok(None);
    }
    let span = PREFILTER_WINDOW_LINES - 1;
    // A window centered on a run of hits: a fixed bucket grid cuts a hit near
    // a boundary away from the lines that introduce it.
    let centered = |first: u64, last: u64| {
        let center = first + (last - first) / 2;
        let end = (center.saturating_sub(PREFILTER_WINDOW_LINES / 2).max(1) + span).min(range_end);
        (end.saturating_sub(span).max(range_start), end)
    };
    let mut windows = Vec::new();
    let mut left = hits.clone();
    while windows.len() < PREFILTER_WINDOWS
        && let Some((first, last)) = densest_run(&left, span - 1)
    {
        let (start, end) = centered(first, last);
        windows.push((start, end));
        left.retain(|line| *line < start || *line > end);
    }
    let mut resume = None;
    if !left.is_empty() || probe_open {
        // More hit clusters than windows: densest windows would strand hits
        // on both sides, so judge in file order and resume after the last.
        windows.clear();
        left.clone_from(&hits);
        while windows.len() < PREFILTER_WINDOWS
            && let Some(&first) = left.first()
        {
            let last = left
                .iter()
                .copied()
                .take_while(|line| *line < first + span)
                .last()
                .unwrap_or(first);
            let (start, end) = centered(first, last);
            windows.push((start, end));
            left.retain(|line| *line > end);
        }
        resume = windows
            .last()
            .map(|(_, end)| end.saturating_add(1))
            .filter(|from| !left.is_empty() || probe_open && *from <= range_end);
    }
    windows.sort_unstable();
    // Later windows hold only hits outside earlier ones, so trimming an
    // overlap keeps every hit exactly once.
    for index in 1..windows.len() {
        windows[index].0 = windows[index].0.max(windows[index - 1].1 + 1);
    }
    Ok(Some(PrefilterPlan {
        windows,
        hits,
        range_end,
        resume,
        probe_open,
    }))
}

/// One prefilter window as a bounded read of the same resource.
fn prefilter_window(resource: &Value, start: u64, end: u64) -> Value {
    let mut window = resource.clone();
    if let Some(object) = window.as_object_mut() {
        object.remove("prefilter");
    }
    let query = &mut window["context"]["query"];
    if let Some(object) = query.as_object_mut() {
        object.remove("fullContent");
    }
    query["startLine"] = json!(start);
    query["endLine"] = json!(end);
    window
}

/// The resource context that continues a prefiltered walk at `from`; the
/// continued resource keeps its `prefilter`, which honors the range.
fn prefilter_resume(resource: &Value, from: u64, range_end: u64) -> Value {
    let mut context = resource["context"].clone();
    if let Some(object) = context["query"].as_object_mut() {
        object.remove("fullContent");
    }
    context["query"]["startLine"] = json!(from);
    context["query"]["endLine"] = json!(range_end);
    context
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

/// Judgments already made in this process for the exact same provider input.
static JUDGMENTS: clasify::cache::JudgmentCache = clasify::cache::JudgmentCache::new();

/// One provider page, answered from [`JUDGMENTS`] when this exact state and
/// question set was judged before (a resumed `next.clasify`, a repeated
/// matrix, or an identical page judged concurrently in the same call). A
/// replay reports no usage because no request was made. Only a fully
/// successful answer set is stored. The key covers what the provider sees;
/// correlation IDs are never sent, so they do not split it.
async fn assess_provider_page(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
) -> (Vec<Result<Value, ClassificationError>>, Option<Value>) {
    let endpoint = format!("{}/{}", config.base_url, config.endpoint_path);
    let provider_questions = questions
        .iter()
        .map(|question| question["question"].clone())
        .collect::<Vec<_>>();
    let key = clasify::cache::key(&endpoint, config.model, state, &provider_questions);
    let flight = JUDGMENTS.flight(&key);
    let assessed = {
        let _turn = flight.lock().await;
        match JUDGMENTS.get(&key) {
            Some(answers) => (answers.into_iter().map(Ok).collect(), None),
            None => request_and_store(state, questions, config, budget, gate, key).await,
        }
    };
    drop(flight);
    JUDGMENTS.land(&key);
    assessed
}

async fn request_and_store(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
    key: [u8; 32],
) -> (Vec<Result<Value, ClassificationError>>, Option<Value>) {
    let (answers, usage) = request_provider_page(state, questions, config, budget, gate).await;
    if answers.iter().all(Result::is_ok) {
        let stored = answers
            .iter()
            .filter_map(|answer| answer.as_ref().ok().cloned())
            .map(|mut answer| {
                if let Some(fields) = answer.as_object_mut() {
                    fields.remove("usage");
                }
                answer
            })
            .collect();
        JUDGMENTS.put(key, stored);
    }
    (answers, usage)
}

async fn request_provider_page(
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

/// Put the caller's goal, next-read reason, and the read that produced the
/// evidence on the state sent to Jev. An evidence object gains the missing
/// sibling keys. A string, array, or object that already uses any of those
/// names is wrapped so a judged field is kept.
fn with_briefs(state: Value, reasoning: &str, goal: &str, read: Option<Value>) -> Value {
    let reasoning = reasoning.trim();
    let goal = goal.trim();
    if reasoning.is_empty() && goal.is_empty() && read.is_none() {
        return state;
    }
    let mut briefs = serde_json::Map::new();
    if !reasoning.is_empty() {
        briefs.insert("reasoning".into(), Value::String(reasoning.to_owned()));
    }
    if !goal.is_empty() {
        briefs.insert("goal".into(), Value::String(goal.to_owned()));
    }
    if let Some(read) = read {
        briefs.insert("read".into(), read);
    }
    match state {
        Value::Object(mut map) if briefs.keys().all(|key| !map.contains_key(key)) => {
            map.extend(briefs);
            Value::Object(map)
        }
        other => {
            briefs.insert("evidence".into(), other);
            Value::Object(briefs)
        }
    }
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
    let context = resource.get("context")?;
    let tool = context.get("tool")?.as_str()?;
    let mut query = context.get("query")?.as_object()?.clone();
    query.retain(|key, value| match key.as_str() {
        "snapshot" | "cursor" | "diagnosticSnapshot" => false,
        "goal" => value
            .as_str()
            .is_some_and(|text| text.trim() != goal.trim()),
        "reasoning" => value
            .as_str()
            .is_some_and(|text| text.trim() != reasoning.trim()),
        _ => true,
    });
    Some(json!({"tool":tool,"query":query}))
}

/// Attach the caller's search goal to one provider question. Public questions
/// and continuations keep the original text on the query, not inside each question.
fn stamp_goal(mut question: Value, goal: &str) -> Value {
    let goal = goal.trim();
    if goal.is_empty() {
        return question;
    }
    let Some(instructions) = question.get_mut("instructions") else {
        return question;
    };
    if let Some(map) = instructions.as_object_mut() {
        map.entry("goal")
            .or_insert_with(|| Value::String(goal.to_owned()));
    } else {
        let prior = instructions.take();
        *instructions = json!({"question": prior, "goal": goal});
    }
    question
}

fn copy_goal(next: &mut Value, query: &Value) {
    if let Some(goal) = query.get("goal").filter(|value| value.is_string()) {
        next["goal"] = goal.clone();
    }
}

/// A delegated read belongs to the same decision as the matrix. Fill a blank
/// brief from the matrix so the tool call stays valid without a second essay.
fn inherit_call_brief(resource: &mut Value, goal: &str, reasoning: &str) {
    let Some(query) = resource
        .pointer_mut("/context/query")
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
    fill(query, "goal", goal);
    fill(query, "reasoning", reasoning);
}

async fn assess_page(
    state: &Value,
    read: Option<Value>,
    questions: &[Value],
    goal: &str,
    reasoning: &str,
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
    let provider_state = with_briefs(provider_state, reasoning, goal, read);
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
            provider_questions
                .push(json!({"id":question["id"],"question":stamp_goal(choice, goal)}));
            let exists_index = provider_questions.len();
            provider_questions
                .push(json!({"id":question["id"],"question":stamp_goal(exists, goal)}));
            plans.push(PublicAnswerPlan::Locate {
                choice: choice_index,
                exists: exists_index,
                page: page.clone(),
            });
        } else {
            let index = provider_questions.len();
            let mut cloned = question.clone();
            if let Some(provider_question) = cloned.get_mut("question") {
                *provider_question = stamp_goal(provider_question.take(), goal);
            }
            provider_questions.push(cloned);
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

/// One provider usage summary: calls and reported tokens (each total only
/// when every call reported it).
fn usage_summary(records: &[Value]) -> Value {
    let total = |key: &str| {
        records
            .iter()
            .map(|record| record.get(key).and_then(Value::as_u64))
            .sum::<Option<u64>>()
    };
    let mut usage = json!({"calls":records.len()});
    if let Some(tokens) = total("input_tokens") {
        usage["inputTokens"] = json!(tokens);
    }
    if let Some(tokens) = total("output_tokens") {
        usage["outputTokens"] = json!(tokens);
    }
    usage
}

/// One matrix's output: compact by default; `debug:true` keeps every page's
/// full answers and adds provider usage receipts (per page and per matrix).
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
    let started = std::time::Instant::now();
    let (mut result, usage) =
        execute_query_verbose(query, dispatcher, execution, config, budget, gate, reads)?;
    if query.get("debug").and_then(Value::as_bool) == Some(true) {
        let mut summary = usage_summary(&usage);
        summary["ms"] = json!(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        result.data["usage"] = summary;
        return Ok((result, usage));
    }
    let locate_ids = query["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|question| clasify::questions::is_locate(&question["question"]))
        .filter_map(|question| question["id"].as_str())
        .collect::<Vec<_>>();
    let single = match query["resources"].as_array().map(Vec::as_slice) {
        Some([resource]) => resource["id"].as_str(),
        _ => None,
    };
    super::clasify_compact::compact_query(
        &mut result.data,
        &super::clasify_compact::Matrix {
            single_resource: single,
            locate_ids: &locate_ids,
            default_max_chars: MAX_RESOURCE_CHARS as u64,
        },
    );
    Ok((result, usage))
}

#[allow(clippy::too_many_arguments)]
fn execute_query_verbose(
    query: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    gate: &GateLease,
    reads: &ReadLimiter,
) -> Result<(DomainResult, Vec<Value>), ExecutionError> {
    execution.check()?;
    let resolved_questions = match clasify::preflight(query) {
        Ok(questions) => questions,
        Err(error) => {
            let resources = query["resources"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|resource| {
                    clasify_output::resource(
                        &resource["id"],
                        &[],
                        vec![PageOutcome::Failed {
                            error: error.clone(),
                            receipt: json!({"limitations":[
                                "Matrix rejected before context retrieval or classification."
                            ]}),
                        }],
                        false,
                    )
                })
                .collect::<Vec<_>>();
            return Ok((
                dispatch::value_result(json!({"queryId":query["id"],"resources":resources})),
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
    let goal = query
        .get("goal")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    let reasoning = query
        .get("reasoning")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    // Capture every page before the first provider request. Search fan-out and
    // file paging expand the input matrix, so only the completed capture can
    // enforce the public 25-cell ceiling without racing or silently dropping
    // candidates.
    let captures = std::thread::scope(|scope| {
        for _ in 0..workers {
            let sender = sender.clone();
            let cursor = &cursor;
            let goal = goal.clone();
            let reasoning = reasoning.clone();
            scope.spawn(move || {
                loop {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(resource) = resources.get(index) else {
                        break;
                    };
                    let mut resource = resource.clone();
                    inherit_call_brief(&mut resource, &goal, &reasoning);
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
            // No page is judged, so a continuation past these pages would
            // skip them for good; the caller retries a smaller matrix.
            resource.continuation = None;
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
                    let mut state = state.clone();
                    relativize_local_paths(&mut state, &dispatcher.paths);
                    // The read request leaves the host with the evidence, so it
                    // passes the same input policy as the context read itself.
                    let read = read_brief(resource.resource, &goal, &reasoning)
                        .and_then(|read| secured_read(read, &dispatcher.security))
                        .map(|mut read| {
                            relativize_local_paths(&mut read, &dispatcher.paths);
                            read
                        });
                    let config = &config;
                    let goal = goal.clone();
                    let reasoning = reasoning.clone();
                    pending.push(async move {
                        let (answers, usage) = assess_page(
                            &state,
                            read,
                            resolved_questions,
                            &goal,
                            &reasoning,
                            config,
                            budget,
                            gate,
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
    let mut locate_reads = Vec::new();
    let mut read_failures = Vec::new();
    let mut judged = false;

    for (resource_index, captured_resource) in captured.into_iter().enumerate() {
        let CapturedResource {
            resource,
            pages,
            continuation,
        } = captured_resource;
        let mut outcomes = Vec::with_capacity(pages.len());
        let mut page_usage = Vec::with_capacity(pages.len());
        let resource_id = resource["id"].as_str().unwrap_or_default();
        for (page_index, mut page) in pages.into_iter().enumerate() {
            clasify_output::host_receipt(page_context_mut(&mut page), &dispatcher.paths);
            match page {
                CapturedPage::Failed { error, context } => {
                    read_failures.push(error.failure);
                    page_usage.push(None);
                    outcomes.push(PageOutcome::Failed {
                        error,
                        receipt: context,
                    });
                }
                CapturedPage::Ready { context, .. } => {
                    judged = true;
                    let Some((assessed_resource, assessed_page, answers, usage)) =
                        assessments.next()
                    else {
                        return Err(ExecutionError::WorkerFailed);
                    };
                    if assessed_resource != resource_index || assessed_page != page_index {
                        return Err(ExecutionError::WorkerFailed);
                    }
                    let records = match usage {
                        Some(usage) => match usage.get("calls").and_then(Value::as_array) {
                            Some(calls) => calls.clone(),
                            None => vec![usage],
                        },
                        None => Vec::new(),
                    };
                    page_usage.push(Some(usage_summary(&records)));
                    usage_records.extend(records);
                    if let Some(read) = clasify_output::read_template(&context) {
                        locate_reads.push(LocateRead {
                            resource_id: resource_id.to_owned(),
                            path: context
                                .pointer("/source/path")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            scope: context["scope"]["startLine"]
                                .as_u64()
                                .zip(context["scope"]["endLine"].as_u64()),
                            read: read.clone(),
                        });
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
        let mut rendered_resource =
            clasify_output::resource(&resource["id"], &question_ids, outcomes, has_continuation);
        if query.get("debug").and_then(Value::as_bool) == Some(true)
            && let Some(pages) = rendered_resource["pages"].as_array_mut()
            && pages.len() == page_usage.len()
        {
            for (page, usage) in pages.iter_mut().zip(page_usage) {
                if let Some(usage) = usage {
                    page["usage"] = usage;
                }
            }
        }
        rendered.push(rendered_resource);
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
    let walk_open = !continuation_resources.is_empty();
    let visible = best
        .as_ref()
        .and_then(|best| readable_best(best, walk_open))
        // Public rows gain an exact read; `carry` keeps the copyable rows.
        .map(|visible| with_row_reads(visible, &locate_reads));
    drop_redundant_page_reads(&mut rendered, visible.as_ref());
    if let Some(visible) = visible {
        output["best"] = visible;
    }
    let hints = locate_targets
        .iter()
        .filter_map(|(_, target)| literal_target_hint(target))
        .collect::<Vec<_>>();
    if !hints.is_empty() {
        output["hints"] = json!(hints);
    }
    let literal = literal_search(locate_targets.iter().map(|(_, target)| *target), resources);
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
        output["next"] = json!({(ToolId::Clasify.as_str()):{
            "id":query["id"],
            "reasoning":query["reasoning"],
            "resources":continuation_resources,
            "questions":public_questions
        }});
        if let Some(best) = best {
            output["next"][ToolId::Clasify.as_str()]["carry"] = best;
        }
        copy_goal(&mut output["next"][ToolId::Clasify.as_str()], query);
        // A debug walk stays one, as every other tool's continuation keeps
        // `debug`: the receipt shape does not change mid-walk.
        if query.get("debug").and_then(Value::as_bool) == Some(true) {
            output["next"][ToolId::Clasify.as_str()]["debug"] = json!(true);
        }
    }
    if let Some(literal) = literal {
        output["next"][ToolId::LocalSearch.as_str()] = literal;
    }
    let mut result = dispatch::value_result(output);
    // Nothing judged and every read failed alike (e.g. every file missing):
    // the call fails the way that read tool fails.
    if !judged {
        result.failure = shared_kind(&read_failures);
    }
    Ok((result, usage_records))
}

fn shared_kind(kinds: &[Option<super::engine::FailureKind>]) -> Option<super::engine::FailureKind> {
    let first = (*kinds.first()?)?;
    kinds
        .iter()
        .all(|kind| *kind == Some(first))
        .then_some(first)
}

/// Every question locates a bare identifier over local sources: an exact
/// literal search answers it, so the matrix routes to `next.localSearch`
/// without a read or provider request.
fn literal_route(query: &Value) -> Option<Value> {
    let identifiers = query["questions"]
        .as_array()
        .filter(|questions| !questions.is_empty())?
        .iter()
        .map(|question| {
            let question = &question["question"];
            clasify::questions::is_locate(question)
                .then(|| question["target"].as_str().and_then(bare_identifier))
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    let search = literal_search(identifiers.iter().copied(), query["resources"].as_array()?)?;
    let mut hints = Vec::<String>::new();
    for identifier in identifiers {
        let hint = bare_target_hint(identifier);
        if !hints.contains(&hint) {
            hints.push(hint);
        }
    }
    Some(json!({
        "queryId":query["id"],
        "hints":hints,
        "resources":[],
        "next":{(ToolId::LocalSearch.as_str()):search}
    }))
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

/// Finished clasify output: the sanitized `{"queries": rows}` envelope and
/// what the response stage needs besides it.
pub(super) struct Receipts {
    pub structured: Value,
    pub source_digest: Option<String>,
    pub failure: Option<super::engine::FailureKind>,
}

/// Clasify's entry: evaluate every matrix under the provider scheduler and
/// return the finished `{"queries": rows}` envelope. Rows are receipts, not
/// ordinary result rows: no result-row shaping, minimizing, path compaction,
/// or cross-tool handoff. Rejected input rows keep their input positions.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    queries: &[Value],
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
    super::engine::merge_rejected_rows(&mut rows, rejected_rows);
    let mut structured = json!({"queries": rows});
    // Email redaction is GitHub-only; receipts get the shared field sanitizer.
    super::response::sanitize_fields(&mut structured, &dispatcher.security, context)?;
    Ok(Receipts {
        structured,
        source_digest,
        failure,
    })
}

fn evaluate(
    queries: &[Value],
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    record_usage: impl FnOnce(ClassificationUsage),
) -> Result<Vec<DomainResult>, ExecutionError> {
    let mut normalized_queries = queries.to_vec();
    // Flat resources and `type`+`ask` questions run as their nested form.
    normalized_queries
        .iter_mut()
        .for_each(clasify::aliases::canonicalize);
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
    fn default_max_chars_matches_every_contract_default() {
        fn walk(value: &Value, found: &mut Vec<u64>) {
            match value {
                Value::Object(fields) => {
                    for (key, field) in fields {
                        if key == "maxChars"
                            && let Some(default) = field.get("default").and_then(Value::as_u64)
                        {
                            found.push(default);
                        }
                        walk(field, found);
                    }
                }
                Value::Array(items) => items.iter().for_each(|item| walk(item, found)),
                _ => {}
            }
        }
        let contract = crate::contracts::parsed_contract().expect("contract");
        let clasify = contract["tools"]
            .as_array()
            .and_then(|tools| tools.iter().find(|tool| tool["name"] == "clasify"))
            .expect("clasify contract");
        let mut defaults = Vec::new();
        walk(clasify, &mut defaults);
        assert!(!defaults.is_empty(), "clasify maxChars default not found");
        assert!(
            defaults
                .iter()
                .all(|default| *default == MAX_RESOURCE_CHARS as u64)
        );
    }

    #[test]
    fn provider_paths_are_workspace_relative_or_file_names() {
        let root = std::env::temp_dir().join("octocode-clasify-paths");
        let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.clone()),
            home_dir: Some(std::path::PathBuf::from("/home/someone")),
            ..Default::default()
        })
        .expect("policy");
        let inside = root.join("src/a.rs").to_string_lossy().into_owned();
        let mut state = json!({
            "base": root.to_string_lossy(),
            "results":[{"data":{"path":"src/a.rs","uri":format!("file://{inside}")}}],
            "query":{"path":inside,"workspaceRoot":"/elsewhere/proj"},
            "home":{"path":"/home/someone/notes/b.md"},
            "gh":{"path":"docs/c.md","base":"main"}
        });
        relativize_local_paths(&mut state, &paths);
        assert_eq!(state["base"], ".");
        assert_eq!(state["results"][0]["data"]["path"], "src/a.rs");
        assert_eq!(state["results"][0]["data"]["uri"], "src/a.rs");
        assert_eq!(state["query"]["path"], "src/a.rs");
        assert_eq!(state["query"]["workspaceRoot"], "proj");
        assert_eq!(state["home"]["path"], "b.md");
        assert_eq!(state["gh"], json!({"path":"docs/c.md","base":"main"}));
    }

    #[test]
    fn flat_questions_and_omitted_ids_are_normalized_for_internal_execution() {
        let mut queries = vec![
            json!({
                "goal": "test", "reasoning":"Locate facts",
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
                "goal": "test", "reasoning":"Judge state",
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

    /// A repo-scoped page names owner/repo once (or only in the query, when
    /// the echo was minimized) and lists numbered `lines`: each candidate
    /// keeps its identity and reads from its first hit line.
    #[test]
    fn repo_scoped_code_pages_keep_candidate_identity_and_line_anchors() {
        let state = json!({"results":[{"data":{"files":[
            {"path":"a.rs","lines":["12\tfn alpha() {}","40\talpha();"]},
            {"path":"b.rs","matches":[{"value":"beta"}],"lineResolved":false}
        ]}}]});
        let code =
            json!({"tool":"ghSearchCode","query":{"owner":"o","repo":"r","keywords":["alpha"]}});
        let candidates = search_candidate_states(&code, &state).expect("code candidates");
        assert_eq!(candidates.len(), 2);
        let first = &candidates[0]["results"][0]["data"]["files"][0];
        assert_eq!(
            (first["owner"].as_str(), first["repo"].as_str()),
            (Some("o"), Some("r"))
        );
        let (read, anchored) = github_candidate_read(&candidates[0], 4000).expect("read");
        assert!(anchored);
        assert_eq!(read["query"]["matchString"], "fn alpha() {}", "{read}");
        assert_eq!(read["query"]["owner"], "o");
    }

    #[test]
    fn more_lines_runs_expand_and_their_omitted_count_names_no_line() {
        let file = json!({"matches":[{"line":3}],
            "pagination":{"moreLines":"1,5-7,+40 more"}});
        assert_eq!(candidate_hit_lines(&file), [1, 3, 5, 6, 7]);
    }

    #[test]
    fn a_clipped_candidate_is_judged_at_every_hit_cluster() {
        // Shown rows cluster near the top; the rows a clipped file lists in
        // moreLines include a far cluster that holds the deciding line.
        let candidate = json!({"base":"/repo","results":[{"data":{"files":[{
            "path":"scrape.go","matches":[{"line":700},{"line":711},{"line":718}],
            "pagination":{"totalMatches":6,"moreLines":"1969,2159,2163"}
        }]}}]});
        let reads = local_candidate_reads(&candidate, 12_000).expect("reads");
        let windows = reads
            .iter()
            .map(|read| {
                (
                    read["query"]["startLine"].clone(),
                    read["query"]["endLine"].clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            windows,
            [
                (json!(649), json!(769)),
                (json!(2101), json!(2221)),
                (json!(1909), json!(2029))
            ],
            "densest cluster first, then the rest"
        );
    }

    /// Windows of one file that overlap or sit within a window radius of each
    /// other are judged as one contiguous page (one provider call), up to
    /// three windows' span; far windows stay separate.
    #[test]
    fn near_windows_of_one_file_merge_into_one_span() {
        assert_eq!(
            merge_near_windows(vec![(1, 74), (51, 171), (182, 302)]),
            [(1, 302)]
        );
        assert_eq!(
            merge_near_windows(vec![(2351, 2471), (2658, 2778)]),
            [(2351, 2471), (2658, 2778)]
        );
        assert_eq!(
            merge_near_windows(vec![(1, 121), (122, 242), (243, 363), (364, 484)]),
            [(1, 363), (364, 484)]
        );
        assert_eq!(merge_near_windows(vec![(5, 125)]), [(5, 125)]);
    }

    #[test]
    fn cluster_windows_share_the_page_budget_round_robin() {
        assert_eq!(allocate_windows(&[3, 1, 4], 25), [3, 1, 4]);
        assert_eq!(allocate_windows(&[3, 1, 4], 5), [2, 1, 2]);
        // Every candidate keeps its densest cluster even past the budget.
        assert_eq!(allocate_windows(&[2, 2, 2], 2), [1, 1, 1]);
        assert_eq!(allocate_windows(&[0, 2], 5), [0, 2]);
    }

    #[test]
    fn hydrated_candidate_reads_are_bounded_and_source_specific() {
        let local = json!({"base":"/repo","results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":90,"value":"needle"}]
        }]}}]});
        let read = local_candidate_read(&local, 12_000).expect("local read");
        assert_eq!(read["tool"], "localFetch");
        // Rows are workspace-relative, which localFetch resolves as-is.
        assert_eq!(read["query"]["path"], "src/a.rs");
        assert_eq!(read["query"]["startLine"], 30);
        assert_eq!(read["query"]["endLine"], 150);
        assert_eq!(read["query"]["chunkSize"], 12_000);

        // An incidental first hit must not pull the window away from the
        // cluster that holds the declaration and its uses.
        let clustered = json!({"base":"/repo","results":[{"data":{"files":[{
            "path":"src/a.rs","matches":[{"line":21},{"line":283},{"line":288},
                {"line":293},{"line":294},{"line":507}]
        }]}}]});
        let read = local_candidate_read(&clustered, 12_000).expect("clustered read");
        assert_eq!(read["query"]["startLine"], 228);
        assert_eq!(read["query"]["endLine"], 348);

        let github = json!({"results":[{"data":{"files":[{
            "owner":"o","repo":"r","path":"src/a.rs","matches":[{
                "value":"fn needle() {}", "matchIndices":[{"start":3,"end":9}]
            }]
        }]}}]});
        let (mut read, anchored) = github_candidate_read(&github, 8_000).expect("github read");
        assert!(anchored);
        assert_eq!(read["query"]["matchString"], "fn needle() {}");
        assert_eq!(read["query"]["chunkSize"], 8_000);
        let hydrated = json!({"results":[{"data":{"files":[{"commitSha":"abc123"}]}}]});
        pin_github_read(&mut read, &hydrated);
        assert_eq!(read["query"]["branch"], "abc123");
    }

    #[test]
    fn candidate_pages_drop_list_paging_and_highlight_offsets() {
        let history = json!({"tool":"ghSearchHistory","query":{"operation":"pullRequest"}});
        let item = json!({"results":[{"data":{
            "type":"pullRequests","pullRequests":[{"number":7,"title":"mpsc: release permits"}],
            "effectiveQuery":"mpsc is:pr","pagination":{"currentPage":1,"hasMore":true}
        }}]});
        assert_eq!(
            candidate_state(&history, item),
            json!({"type":"pullRequests","pullRequests":[{"number":7,"title":"mpsc: release permits"}]})
        );
        let code = json!({"tool":"ghSearchCode","query":{"owner":"o"}});
        let hit = json!({"results":[{"data":{"files":[{"owner":"o","repo":"r","path":"a.rs",
            "matches":[{"value":"acquire(n)","matchIndices":[{"start":0,"end":7}]}]}],
            "pagination":{"currentPage":1}}}]});
        assert_eq!(
            candidate_state(&code, hit),
            json!({"files":[{"owner":"o","repo":"r","path":"a.rs","matches":[{"value":"acquire(n)"}]}]})
        );
    }

    #[test]
    fn github_candidate_read_anchors_on_the_densest_matched_line() {
        // A bare keyword ("semaphore") recurs across the file, so anchoring on
        // it reads every occurrence; the matched line is the hit itself.
        let value = "        }\n\n        let guard = WakeReceiverOnDrop { chan: &self.chan };\n        let result = self.chan.semaphore().semaphore.acquire(n).await;\n\n        match result {";
        let github = json!({"results":[{"data":{"files":[{
            "owner":"o","repo":"r","path":"src/bounded.rs","matches":[{
                "value":value,
                "matchIndices":[
                    {"start":103,"end":112,"lineOffset":3},
                    {"start":115,"end":124,"lineOffset":3},
                    {"start":125,"end":132,"lineOffset":3}
                ]
            }]
        }]}}]});
        let (read, anchored) = github_candidate_read(&github, 8_000).expect("github read");
        assert!(anchored);
        assert_eq!(
            read["query"]["matchString"],
            "let result = self.chan.semaphore().semaphore.acquire(n).await;"
        );
        // Without lineOffset the line is derived from the match start; an
        // overlong (minified) line falls back to the matched term.
        let long = format!("{} needle {}", "x".repeat(300), "y".repeat(300));
        let github = json!({"results":[{"data":{"files":[{
            "owner":"o","repo":"r","path":"a.min.js","matches":[
                {"value":format!("a\n{long}"),"matchIndices":[{"start":303,"end":309}]}
            ]
        }]}}]});
        let (read, _) = github_candidate_read(&github, 8_000).expect("minified read");
        assert_eq!(read["query"]["matchString"], "needle");
    }

    #[test]
    fn list_tools_bound_their_first_page_to_the_cell_budget() {
        let repos =
            json!({"tool":"ghSearchRepo","query":{"goal":"g","reasoning":"r","keywords":["x"]}});
        let bounded = bounded_search_source(&repos, 6).expect("first page");
        assert_eq!(bounded["query"]["pageSize"], 6);
        assert_eq!(bounded["query"]["page"], 1);
        let packages = json!({"tool":"artifactSearch","query":{
            "goal":"g","reasoning":"r","type":"npm","keywords":["x"],"cursor":"c","pageSize":20
        }});
        let bounded = bounded_search_source(&packages, 5).expect("cursor page");
        assert_eq!(bounded["query"]["pageSize"], 5);
        assert!(
            bounded["query"].get("page").is_none(),
            "cursor tools have no page"
        );
        let later = json!({"tool":"ghSearchHistory","query":{"operation":"issue","page":3}});
        assert_eq!(
            bounded_search_source(&later, 5).expect("unknown default"),
            later
        );
        let symbols = json!({"tool":"astSearch","query":{"operation":"symbols","path":"/r"}});
        assert_eq!(
            bounded_search_source(&symbols, 5).expect("grouped"),
            symbols
        );
    }

    #[test]
    fn candidate_page_bound_preserves_the_original_search_offset() {
        let first = json!({"tool":"localSearch","query":{
            "goal": "test", "reasoning":"find","path":"/repo","searchText":"x","pageSize":20
        }});
        let bounded = bounded_search_source(&first, 5).expect("first page");
        assert_eq!(bounded["query"]["page"], 1);
        assert_eq!(bounded["query"]["pageSize"], 5);

        let aligned = json!({"tool":"ghSearchCode","query":{
            "goal": "test", "reasoning":"find","owner":"o","keywords":["x"],
            "page":2,"pageSize":20
        }});
        let bounded = bounded_search_source(&aligned, 5).expect("aligned offset");
        assert_eq!(bounded["query"]["page"], 5);
        assert_eq!(bounded["query"]["pageSize"], 5);

        let unaligned = json!({"tool":"localSearch","query":{
            "goal": "test", "reasoning":"find","path":"/repo","searchText":"x","page":2,"pageSize":6
        }});
        let bounded = bounded_search_source(&unaligned, 5).expect("aligned smaller page");
        assert_eq!(bounded["query"]["page"], 3);
        assert_eq!(bounded["query"]["pageSize"], 3);
        assert_eq!(
            (bounded["query"]["page"].as_u64().expect("page") - 1)
                * bounded["query"]["pageSize"].as_u64().expect("page size"),
            6
        );
    }

    #[test]
    fn concise_search_candidates_keep_per_file_identity_and_executable_reads() {
        let source = json!({"tool":"ghSearchCode", "query":{"goal":"find evidence"}});
        let state = json!({"results":[{"data":{"files":["o/r:src/a.rs", "o/r:src/b.rs", "o/r:src/a.rs"]}}]});
        let candidates = search_candidate_states(&source, &state).expect("concise candidates");
        assert_eq!(candidates.len(), 2);
        for (candidate, path) in candidates.iter().zip(["src/a.rs", "src/b.rs"]) {
            let file = &candidate["results"][0]["data"]["files"][0];
            assert_eq!(
                candidate_identity(&source, file),
                Some(format!("o/r/{path}"))
            );
            let read = host_read(&source, candidate).expect("candidate read");
            assert_eq!(read["tool"], "ghGetFileContent");
            assert_eq!(read["query"]["owner"], "o");
            assert_eq!(read["query"]["repo"], "r");
            assert_eq!(read["query"]["path"], path);
        }
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
    fn briefs_reach_the_provider_state_beside_the_evidence() {
        let file = json!({"path":"a.rs","lines":[1,1],"content":"fn a() {}\n"});
        let tagged = with_briefs(
            file,
            "  The next read is the writer.  ",
            "  The function that writes the continuation.  ",
            None,
        );
        assert_eq!(tagged["reasoning"], "The next read is the writer.");
        assert_eq!(tagged["goal"], "The function that writes the continuation.");
        assert_eq!(tagged["path"], "a.rs");
        assert_eq!(tagged["content"], "fn a() {}\n");
        assert_eq!(
            with_briefs(json!("plain"), "why", "what", None),
            json!({"reasoning":"why","goal":"what","evidence":"plain"})
        );
        assert_eq!(
            with_briefs(json!(["a", "b"]), "why", "what", None)["evidence"],
            json!(["a", "b"])
        );
        let existing = with_briefs(
            json!({"goal": "test", "reasoning":"field","content":"x"}),
            "why",
            "what",
            None,
        );
        assert_eq!(existing["reasoning"], "why");
        assert_eq!(existing["goal"], "what");
        assert_eq!(existing["evidence"]["reasoning"], "field");
        assert_eq!(
            with_briefs(json!({"a":1}), "   ", "   ", None),
            json!({"a":1})
        );
    }

    #[test]
    fn read_briefs_pass_the_input_security_policy() {
        let security = crate::security::ContentSecurity;
        let read = json!({"tool":"localSearch","query":{"path":"/repo","searchText":"retry"}});
        assert_eq!(secured_read(read.clone(), &security), Some(read));
        let leaky = json!({"tool":"localSearch","query":{
            "path":"/repo","searchText":"ghp_abcdefghijklmnopqrstuvwxyz0123456789"
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
        let resource = json!({"id":"hits","context":{"tool":"localSearch","query":{
            "path":"/repo","searchText":"Retry-After","snapshot":"opaque",
            "goal":"Find retries.","reasoning":"Only snippets name the retry file."
        }}});
        let read = read_brief(&resource, "Find retries.", "Screen first.").expect("tool read");
        assert_eq!(
            read,
            json!({"tool":"localSearch","query":{
                "path":"/repo","searchText":"Retry-After",
                "reasoning":"Only snippets name the retry file."
            }})
        );
        let state = with_briefs(
            json!({"files":[]}),
            "Screen first.",
            "Find retries.",
            Some(read),
        );
        assert_eq!(state["read"]["query"]["searchText"], "Retry-After");
        assert_eq!(state["goal"], "Find retries.");
        assert!(read_brief(&json!({"context":{"value":"x"}}), "g", "r").is_none());
    }

    #[test]
    fn goal_reaches_every_provider_question_and_the_continuation() {
        let goal = "  Searching for retry handling. Need files that decide a retry.  ";
        let direct = json!({"type":"noul","instructions":"Does this decide a retry?"});
        let stamped = stamp_goal(direct.clone(), goal);
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
            stamp_goal(preset, goal)["instructions"]["goal"],
            "Searching for retry handling. Need files that decide a retry."
        );
        let owned = json!({"type":"noul","instructions":{"question":"prompt","goal":"own"}});
        assert_eq!(stamp_goal(owned, goal)["instructions"]["goal"], "own");
        assert_eq!(
            stamp_goal(direct, "   ")["instructions"],
            "Does this decide a retry?"
        );
        let [choice, exists] = locate_provider_questions("retry decision", &LocatedPage::default());
        for question in [choice, exists] {
            assert_eq!(
                stamp_goal(question, goal)["instructions"]["goal"],
                "Searching for retry handling. Need files that decide a retry."
            );
        }
        let mut next = json!({});
        copy_goal(
            &mut next,
            &json!({"goal":"Searching for retry handling. Need files that decide a retry."}),
        );
        assert_eq!(
            next["goal"],
            "Searching for retry handling. Need files that decide a retry."
        );
        let mut blank = json!({});
        copy_goal(&mut blank, &json!({}));
        assert!(blank.get("goal").is_none());
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
