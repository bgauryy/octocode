//! jevScout — batched, typed Jev judgment over reader-produced anchored spans
//! that ranks which candidate files or pre-fetched rows the host should READ.
//!
//! Shares taxonomy, thresholds, and veto semantics with the skill scout runner.
//! Budget-truncated rejections remain uncertain reads. Provider answers must
//! satisfy the same typed response validator used by the reasoning tool.
//!
//! A scout PRIORITIZES reads. Every verdict is `provisional` and anchored; it
//! never authorizes an irreversible action and is never citable evidence.
//!
//! Two deviations from the JS runner, both required by the native environment:
//!   1. Redaction reuses the native secrets primitive
//!      `octocode_engine::portable::sanitize_content` (the same one the localFetch
//!      read path calls via `ContentSecurity::sanitize_text`) instead of the JS
//!      regex stand-in. Redaction MARKERS therefore differ (`[REDACTED-…]` vs
//!      `«redacted-token»`); the set of redacted bytes is what matters.
//!   2. Truncation to `spanBudget` slices on UTF-8 char boundaries
//!      (`char_indices`) so a code point is never split. JS slices by UTF-16 code
//!      units, so for astral code points (emoji beyond U+FFFF) straddling the
//!      budget the two implementations can differ by up to one code point of
//!      content length. For BMP text (ASCII, CJK) the boundaries coincide. The
//!      property tests therefore assert identical span sources/line ranges for all
//!      encodings and content-length equality only for ASCII fixtures.

use crate::providers::RequestBudget;
use crate::tools::jev_reasoning::{JevProviderError, endpoint, post};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Map, Value, json};
use std::path::Path;

mod sandbox;
mod text;
use sandbox::{bounded_root, lexical_normalize};
use text::{default_levels, is_object, redact, round3, truncate_utf16, utf16_len};

const WINDOW: u64 = 6;
const SPAN_BUDGET: u64 = 3000;
const MAX_SPANS: usize = 12;
/// Upper bound on anchor patterns per scout call. Anchors run against every line
/// of every candidate file, so an unbounded count is a work-amplification vector.
const MAX_ANCHORS: usize = 64;
/// Upper bound on a single anchor's source length (bytes).
const MAX_ANCHOR_BYTES: usize = 4_096;
const T_SKIP: f64 = 0.25;
const DISTINGUISH: &str =
    "importing or calling a capability defined elsewhere is NOT implementing it";
const NO_MATCH: &str = "no anchor matches in this file";

fn err(code: &str, message: impl Into<String>, hint: &str) -> JevProviderError {
    JevProviderError {
        code: code.to_owned(),
        message: message.into(),
        hints: vec![hint.to_owned()],
    }
}

#[derive(Clone, Debug)]
struct Span {
    source: String,
    content: String,
}

#[derive(Clone, Debug)]
struct Located {
    spans: Vec<Span>,
    coverage: f64,
    truncated: bool,
    /// Full-source length (UTF-16 units), mirroring scout.mjs `fileChars`. Kept
    /// for the S3 session-stats bytes-off-host accounting; not part of the output
    /// contract, so unread in this step.
    #[allow(dead_code)]
    file_chars: usize,
}

#[derive(Clone, Debug)]
struct Dim {
    key: String,
    role: String,
    claim: String,
    levels: Vec<Value>,
}

// ---------------------------------------------------------------------------
// Locate (local mode): port of scout.mjs locateSpans.
// ---------------------------------------------------------------------------

fn merge_ranges(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    ranges.sort_by_key(|range| range.0);
    let mut out: Vec<(u64, u64)> = Vec::new();
    for range in ranges {
        if let Some(last) = out.last_mut()
            && range.0 <= last.1 + 1
        {
            last.1 = last.1.max(range.1);
        } else {
            out.push(range);
        }
    }
    out
}

fn locate_spans(
    root_dir: &Path,
    file: &str,
    anchors: &[String],
    window: u64,
    span_budget: u64,
) -> Result<Located, JevProviderError> {
    let abs = bounded_root(root_dir, file)?;
    let bytes = std::fs::read(&abs).map_err(|error| {
        err(
            "fileAccessFailed",
            format!("scout could not read candidate {file:?}: {error}."),
            "Verify the candidate path exists inside the sandbox root.",
        )
    })?;
    // Node readFileSync('utf8') replaces malformed sequences with U+FFFD, exactly
    // like from_utf8_lossy — keeps byte-for-byte parity with the JS reader.
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    // split('\n') (NOT lines()) so CRLF '\r' stays attached, matching JS.
    let lines: Vec<&str> = raw.split('\n').collect();
    let line_count = lines.len() as u64;

    let mut compiled = Vec::with_capacity(anchors.len());
    for anchor in anchors {
        // Compile anchors with the linear-time `regex` engine (a finite automaton
        // with no catastrophic backtracking) rather than the backtracking
        // `regress` engine. Scout runs every anchor against every line of
        // untrusted candidate files, so a pattern such as `(a+)+$` must not be
        // able to pin a CPU core (ReDoS). Patterns that require ECMA-only
        // backtracking features (lookahead, backreferences) fail to compile here
        // and are rejected rather than executed unbounded.
        let regex = regex::RegexBuilder::new(anchor)
            .case_insensitive(true)
            .size_limit(1 << 20)
            .build()
            .map_err(|error| {
                err(
                    "invalidJevRequest",
                    format!("scout anchor {anchor:?} is not a valid regular expression: {error}."),
                    "Provide a linear (non-backtracking) ECMAScript-compatible anchor pattern.",
                )
            })?;
        compiled.push(regex);
    }

    let mut hits: Vec<(u64, u64)> = Vec::new();
    for regex in &compiled {
        for (index, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                let center = index as u64 + 1;
                let start = center.saturating_sub(window).max(1);
                let end = (center + window).min(line_count);
                hits.push((start, end));
            }
        }
    }

    let mut spans: Vec<Span> = Vec::new();
    let mut judged: u64 = 0;
    let mut truncated = false;
    for (start, end) in merge_ranges(hits) {
        if spans.len() >= MAX_SPANS || judged >= span_budget {
            truncated = true;
            break;
        }
        let slice = &lines[(start as usize - 1)..(end as usize)];
        let mut content = redact(&slice.join("\n"));
        let remaining = span_budget - judged;
        if utf16_len(&content) as u64 > remaining {
            truncated = true;
            content = truncate_utf16(&content, remaining as usize);
        }
        if content.trim().is_empty() {
            continue;
        }
        judged += utf16_len(&content) as u64;
        spans.push(Span {
            source: format!("{file}:L{start}-L{end}"),
            content,
        });
    }

    let file_chars = utf16_len(&raw);
    let coverage = if file_chars > 0 {
        round3(judged as f64 / file_chars as f64)
    } else {
        0.0
    };
    Ok(Located {
        spans,
        coverage,
        truncated,
        file_chars,
    })
}

// ---------------------------------------------------------------------------
// Locate (items mode): port of scout.mjs locateItems.
// ---------------------------------------------------------------------------

fn locate_items(
    items: &[Value],
    span_budget: u64,
) -> Result<Vec<(String, Located)>, JevProviderError> {
    let mut located = Vec::with_capacity(items.len());
    let mut identities = std::collections::HashSet::new();
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let content = item.get("content").and_then(Value::as_str);
        let (id, content) = match (id, content) {
            (Some(id), Some(content)) if !content.trim().is_empty() => (id, content),
            _ => {
                return Err(err(
                    "invalidJevRequest",
                    "scout items mode requires { id, content } per item.",
                    "Provide a nonempty id and nonempty content for every item.",
                ));
            }
        };
        if !identities.insert(id) {
            return Err(err(
                "invalidJevRequest",
                "scout item IDs must be unique.",
                "Give every pre-fetched item a distinct id.",
            ));
        }
        let full_chars = utf16_len(content);
        let bounded = truncate_utf16(content, span_budget as usize);
        let redacted = redact(&bounded);
        let source = item
            .get("source")
            .and_then(Value::as_str)
            .filter(|source| !source.is_empty())
            .unwrap_or(id)
            .to_owned();
        let coverage = if full_chars > 0 {
            round3(utf16_len(&redacted) as f64 / full_chars as f64)
        } else {
            0.0
        };
        located.push((
            id.to_owned(),
            Located {
                spans: vec![Span {
                    source,
                    content: redacted,
                }],
                coverage,
                file_chars: full_chars,
                truncated: full_chars > span_budget as usize,
            },
        ));
    }
    Ok(located)
}

// ---------------------------------------------------------------------------
// Dimensions: port of scout.mjs normalizeDimensions.
// ---------------------------------------------------------------------------

fn dim_from_value(value: &Value, claim: &str) -> Dim {
    let key = value
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let role = value
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("info")
        .to_owned();
    let dim_claim = value
        .get("claim")
        .and_then(Value::as_str)
        .unwrap_or(claim)
        .to_owned();
    let levels = value
        .get("taxonomy")
        .and_then(Value::as_str)
        .and_then(preset_levels)
        .or_else(|| value.get("levels").and_then(Value::as_array).cloned())
        .unwrap_or_else(default_levels);
    Dim {
        key,
        role,
        claim: dim_claim,
        levels,
    }
}

/// Named taxonomy presets — byte-identical to scout.mjs `TAXONOMIES` so the
/// same packet resolves identically on both implementations.
fn preset_levels(name: &str) -> Option<Vec<Value>> {
    match name {
        "implements" => Some(default_levels()),
        "relevance" => Some(vec![
            json!({ "level": "unrelated", "meaning": "does not concern the question" }),
            json!({ "level": "adjacent", "meaning": "touches the same area without bearing on the question" }),
            json!({ "level": "related", "meaning": "bears on the question but does not settle or address it" }),
            json!({ "level": "addresses", "meaning": "directly addresses the question; open this one" }),
        ]),
        _ => None,
    }
}

fn normalize_dimensions(query: &Value, claim: &str) -> Result<Vec<Dim>, JevProviderError> {
    let Some(raw) = query.get("dimensions").and_then(Value::as_array) else {
        // Absent (or non-array) → single default primary dimension.
        let levels = query
            .get("taxonomy")
            .and_then(Value::as_str)
            .and_then(preset_levels)
            .or_else(|| query.get("levels").and_then(Value::as_array).cloned())
            .unwrap_or_else(default_levels);
        return Ok(vec![Dim {
            key: "main".to_owned(),
            role: "primary".to_owned(),
            claim: claim.to_owned(),
            levels,
        }]);
    };
    if raw.is_empty() || raw.len() > 4 {
        return Err(err(
            "invalidJevRequest",
            "scout dimensions must be 1..4.",
            "Provide between one and four dimensions.",
        ));
    }
    let dims: Vec<Dim> = raw
        .iter()
        .map(|value| dim_from_value(value, claim))
        .collect();
    if dims.iter().filter(|dim| dim.role == "primary").count() != 1 {
        return Err(err(
            "invalidJevRequest",
            "scout dimensions require exactly one primary.",
            "Mark exactly one dimension role=primary.",
        ));
    }
    let mut keys: Vec<&str> = dims.iter().map(|dim| dim.key.as_str()).collect();
    keys.sort_unstable();
    let unique = keys.windows(2).all(|pair| pair[0] != pair[1]);
    if !unique {
        return Err(err(
            "invalidJevRequest",
            "scout dimension keys must be unique.",
            "Give each dimension a distinct key.",
        ));
    }
    for dim in &dims {
        if dim.key.is_empty() || dim.levels.len() < 2 {
            return Err(err(
                "invalidJevRequest",
                format!(
                    "dimension {} requires key and >=2 levels.",
                    if dim.key.is_empty() { "?" } else { &dim.key }
                ),
                "Provide a key and at least two ordered levels per dimension.",
            ));
        }
    }
    Ok(dims)
}

// ---------------------------------------------------------------------------
// Request build: port of scout.mjs buildScoutRequest (byte-faithful).
// ---------------------------------------------------------------------------

fn question_id(candidate_index: usize, dim: &Dim, single: bool) -> String {
    if single {
        format!("candidate_{candidate_index}")
    } else {
        format!("candidate_{candidate_index}__{}", dim.key)
    }
}

fn spans_value(loc: &Located) -> Value {
    if loc.spans.is_empty() {
        Value::String(NO_MATCH.to_owned())
    } else {
        Value::Array(
            loc.spans
                .iter()
                .map(|span| json!({ "source": span.source, "content": span.content }))
                .collect(),
        )
    }
}

fn build_request(claim: &str, model: &str, located: &[(String, Located)], dims: &[Dim]) -> Value {
    let single = dims.len() == 1;
    let mut candidates = Map::new();
    let mut questions = Map::new();
    for (candidate_index, (file, loc)) in located.iter().enumerate() {
        candidates.insert(file.clone(), spans_value(loc));
        for dim in dims {
            questions.insert(
                question_id(candidate_index, dim, single),
                json!({
                    "type": "score",
                    "instructions": {
                        "judge": format!("{}: {}", dim.key, dim.claim),
                        "candidate": file,
                        "use_only": format!("state.candidates[\"{file}\"]"),
                        "distinguish": DISTINGUISH,
                    },
                    "criteria": Value::Array(dim.levels.clone()),
                }),
            );
        }
    }
    json!({
        "model": model,
        "state": {
            "task": format!("For each candidate, judge every listed question about: {claim}."),
            "candidates": Value::Object(candidates),
        },
        "questions": Value::Object(questions),
    })
}

// ---------------------------------------------------------------------------
// Policy v2 + veto combiner: port of scout.mjs applyPolicy + runScout combiner.
// ---------------------------------------------------------------------------

/// argmax over a probabilities map, matching JS `Object.entries(probs).reduce`
/// seeded with `[0, -1]`: integer keys iterate ascending, strict-greater wins so
/// ties keep the lowest index; an empty map yields 0.
fn argmax_index(probs: &Map<String, Value>) -> i64 {
    let mut entries: Vec<(Option<i64>, &String, f64)> = probs
        .iter()
        .map(|(key, value)| {
            let numeric = key
                .parse::<i64>()
                .ok()
                .filter(|number| *number >= 0 && &number.to_string() == key);
            (numeric, key, value.as_f64().unwrap_or(f64::NAN))
        })
        .collect();
    entries.sort_by(|a, b| match (a.0, b.0) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    let mut best_index: i64 = 0;
    let mut best_prob: f64 = -1.0;
    for (numeric, _key, prob) in entries {
        if prob > best_prob {
            best_prob = prob;
            best_index = numeric.unwrap_or(i64::MIN);
        }
    }
    best_index
}

fn probs_of(answer: &Value) -> Option<&Map<String, Value>> {
    answer.get("probabilities").and_then(Value::as_object)
}

/// FROZEN policy v2 (see module docs). Top level = last taxonomy entry.
fn apply_policy(
    answer: &Value,
    has_spans: bool,
    levels_len: usize,
) -> (&'static str, &'static str) {
    if !has_spans {
        return ("skip", "no_evidence");
    }
    let top = levels_len.saturating_sub(1) as i64;
    let empty = Map::new();
    let probs = probs_of(answer).unwrap_or(&empty);
    let p_top = probs
        .get(&top.to_string())
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let argmax = argmax_index(probs);
    if argmax == top {
        ("read", "argmax_top")
    } else if p_top <= T_SKIP {
        ("skip", "judged_and_rejected")
    } else {
        ("gray_read", "fail_open")
    }
}

fn level_label(dim: &Dim, answer: &Value, has_spans: bool) -> Value {
    if !has_spans {
        return Value::Null;
    }
    let index = probs_of(answer).map(argmax_index).unwrap_or(-1);
    if index >= 0 && (index as usize) < dim.levels.len() {
        dim.levels[index as usize]
            .get("level")
            .cloned()
            .unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}

// ---------------------------------------------------------------------------
// Shared typed provider-response validation, including Score distributions.
// ---------------------------------------------------------------------------

fn validate_answers(response: &Value, request: &Value) -> Result<(), JevProviderError> {
    octocode_engine::jev::validate_response(request, response).map_err(|error| err(
        "invalidJevResponse", error.message,
        "No scout verdicts are available; read the candidates or inspect provider compatibility.",
    ))
}

// ---------------------------------------------------------------------------
// Orchestration.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Parsed {
    claim: String,
    located: Vec<(String, Located)>,
    dims: Vec<Dim>,
    model: String,
}

fn parse_and_locate(query: &Value, default_model: &str) -> Result<Parsed, JevProviderError> {
    let claim = query
        .get("claim")
        .and_then(Value::as_str)
        .filter(|claim| !claim.is_empty())
        .ok_or_else(|| {
            err(
                "invalidJevRequest",
                "scout input requires claim.",
                "Provide a nonempty capability claim.",
            )
        })?
        .to_owned();

    let source = query.get("source");
    let local = source
        .and_then(|source| source.get("local"))
        .filter(|value| is_object(value));
    let items = source
        .and_then(|source| source.get("items"))
        .and_then(Value::as_array);

    if local.is_some() == items.is_some() {
        return Err(err(
            "invalidJevRequest",
            "scout requires exactly one of source.local (files) or source.items (pre-fetched rows).",
            "Supply either a local file source or an items source, not both or neither.",
        ));
    }

    let dims = normalize_dimensions(query, &claim)?;
    let model = query
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .unwrap_or(default_model)
        .to_owned();

    let located = if let Some(local) = local {
        let candidates = local.get("candidates").and_then(Value::as_array);
        let anchors = local.get("anchors").and_then(Value::as_array);
        let candidates = candidates.ok_or_else(|| {
            err(
                "invalidJevRequest",
                "scout local source requires candidates[].",
                "Provide 2..12 rootDir-relative candidate paths.",
            )
        })?;
        let anchors_raw = anchors
            .filter(|anchors| !anchors.is_empty())
            .ok_or_else(|| {
                err(
                    "invalidJevRequest",
                    "scout local source requires claim and anchors[].",
                    "Provide at least one anchor pattern.",
                )
            })?;
        if candidates.len() < 2 || candidates.len() > 12 {
            return Err(err(
                "invalidJevRequest",
                "scout input requires 2..12 candidates.",
                "Provide between two and twelve candidates.",
            ));
        }
        if dims.len() * candidates.len() > 24 {
            return Err(err(
                "invalidJevRequest",
                "scout allows at most 24 questions (candidates x dimensions).",
                "Reduce candidates or dimensions so their product is <= 24.",
            ));
        }
        let candidate_paths: Vec<&str> = candidates
            .iter()
            .map(|candidate| {
                candidate.as_str().ok_or_else(|| {
                    err(
                        "invalidJevRequest",
                        "scout candidates[] must be strings.",
                        "Provide candidate paths as strings.",
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        if candidate_paths
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != candidate_paths.len()
        {
            return Err(err(
                "invalidJevRequest",
                "scout candidate paths must be unique.",
                "Provide each candidate path once.",
            ));
        }
        let anchor_list: Vec<String> = anchors_raw
            .iter()
            .filter_map(|anchor| anchor.as_str().map(str::to_owned))
            .collect();
        if anchor_list.len() != anchors_raw.len() {
            return Err(err(
                "invalidJevRequest",
                "scout anchors[] must be strings.",
                "Provide anchor patterns as strings.",
            ));
        }
        if anchor_list.len() > MAX_ANCHORS {
            return Err(err(
                "invalidJevRequest",
                format!("scout allows at most {MAX_ANCHORS} anchors."),
                "Reduce the number of anchor patterns.",
            ));
        }
        if let Some(oversized) = anchor_list
            .iter()
            .find(|anchor| anchor.len() > MAX_ANCHOR_BYTES)
        {
            return Err(err(
                "invalidJevRequest",
                format!("scout anchor {oversized:?} exceeds {MAX_ANCHOR_BYTES} bytes."),
                "Shorten the anchor pattern.",
            ));
        }
        let window = local
            .get("window")
            .and_then(Value::as_u64)
            .unwrap_or(WINDOW);
        let span_budget = local
            .get("spanBudget")
            .and_then(Value::as_u64)
            .unwrap_or(SPAN_BUDGET);
        let cwd = std::env::current_dir().map_err(|error| {
            err(
                "invalidJevRequest",
                format!("scout could not resolve the working directory: {error}."),
                "Run the scout from a readable working directory.",
            )
        })?;
        let root_str = local.get("root").and_then(Value::as_str).unwrap_or(".");
        let root_dir = lexical_normalize(&cwd.join(root_str));
        let mut located = Vec::with_capacity(candidates.len());
        for file in candidate_paths {
            let loc = locate_spans(&root_dir, file, &anchor_list, window, span_budget)?;
            located.push((file.to_owned(), loc));
        }
        located
    } else {
        // items mode (source.items is present here by the exactly-one check)
        let items = items.expect("items source present in items mode");
        if items.len() < 2 || items.len() > 12 {
            return Err(err(
                "invalidJevRequest",
                "scout input requires 2..12 candidates.",
                "Provide between two and twelve items.",
            ));
        }
        if dims.len() * items.len() > 24 {
            return Err(err(
                "invalidJevRequest",
                "scout allows at most 24 questions (candidates x dimensions).",
                "Reduce items or dimensions so their product is <= 24.",
            ));
        }
        let span_budget = query
            .get("itemSpanBudget")
            .and_then(Value::as_u64)
            .unwrap_or(SPAN_BUDGET);
        locate_items(items, span_budget)?
    };

    Ok(Parsed {
        claim,
        located,
        dims,
        model,
    })
}

fn build_results(parsed: &Parsed, response: &Value) -> Value {
    let single = parsed.dims.len() == 1;
    let empty_map = Map::new();
    let answers = response
        .get("answers")
        .and_then(Value::as_object)
        .unwrap_or(&empty_map);
    let primary_key = parsed
        .dims
        .iter()
        .find(|dim| dim.role == "primary")
        .map(|dim| dim.key.clone())
        .unwrap_or_default();

    let mut results = Map::new();
    let mut reads: Vec<Value> = Vec::new();
    let mut required_reads: Vec<Value> = Vec::new();
    for (candidate_index, (file, loc)) in parsed.located.iter().enumerate() {
        let has_spans = !loc.spans.is_empty();
        let answer_for = |dim: &Dim| -> Value {
            answers
                .get(&question_id(candidate_index, dim, single))
                .cloned()
                .unwrap_or_else(|| json!({}))
        };
        let primary_dim = parsed
            .dims
            .iter()
            .find(|dim| dim.role == "primary")
            .expect("normalize_dimensions guarantees exactly one primary");
        let primary_answer = answer_for(primary_dim);
        let (initial_action, initial_reason) =
            apply_policy(&primary_answer, has_spans, primary_dim.levels.len());
        let mut action: &str = initial_action;
        let mut reason: String = initial_reason.to_owned();
        if action == "skip" && loc.truncated {
            action = "gray_read";
            reason = "incomplete_excerpt".to_owned();
        }

        let mut dimensions = Map::new();
        for dim in &parsed.dims {
            let answer = answer_for(dim);
            let level = level_label(dim, &answer, has_spans);
            let score = answer.get("score").cloned().unwrap_or(Value::Null);
            let probabilities = answer.get("probabilities").cloned().unwrap_or(Value::Null);
            dimensions.insert(
                dim.key.clone(),
                json!({
                    "role": dim.role,
                    "level": level,
                    "score": score,
                    "probabilities": probabilities,
                }),
            );
            // Deterministic veto combiner: a veto court at its bottom level demotes
            // read -> gray_read (host reads anyway); it never creates a skip.
            if dim.role == "veto"
                && action == "read"
                && has_spans
                && let Some(probs) = probs_of(&answer)
                && argmax_index(probs) == 0
            {
                action = "gray_read";
                reason = format!("vetoed_by_{}", dim.key);
            }
        }

        let primary_entry = dimensions.get(&primary_key).cloned().unwrap_or(Value::Null);
        let level = primary_entry.get("level").cloned().unwrap_or(Value::Null);
        let score = primary_entry.get("score").cloned().unwrap_or(Value::Null);
        let probabilities = primary_entry
            .get("probabilities")
            .cloned()
            .unwrap_or(Value::Null);

        let mut row = Map::new();
        row.insert("action".to_owned(), Value::String(action.to_owned()));
        row.insert("reason".to_owned(), Value::String(reason));
        row.insert("level".to_owned(), level);
        row.insert("score".to_owned(), score);
        row.insert("probabilities".to_owned(), probabilities);
        if !single {
            row.insert("dimensions".to_owned(), Value::Object(dimensions));
        }
        row.insert("coverage".to_owned(), json!(loc.coverage));
        row.insert("truncated".to_owned(), json!(loc.truncated));
        row.insert(
            "anchors".to_owned(),
            Value::Array(
                loc.spans
                    .iter()
                    .map(|span| Value::String(span.source.clone()))
                    .collect(),
            ),
        );
        row.insert("provisional".to_owned(), Value::Bool(true));

        if action == "read" {
            reads.push(Value::String(file.clone()));
        }
        if action != "skip" {
            required_reads.push(Value::String(file.clone()));
        }
        results.insert(file.clone(), Value::Object(row));
    }

    json!({
        "status": "scouted",
        "claim": parsed.claim,
        "model": response.get("model").cloned().unwrap_or(Value::Null),
        "results": Value::Object(results),
        "reads": Value::Array(reads),
        "requiredReads": Value::Array(required_reads),
        "usage": response.get("usage").cloned().unwrap_or(Value::Null),
    })
}

/// Symmetric to `jev_reasoning::execute`: locate → one batched Score request →
/// frozen policy v2. Reuses `jev_reasoning::{post, endpoint}` for HTTP.
pub async fn execute(
    query: &Value,
    key: SecretString,
    base_url: &str,
    default_model: &str,
    budget: RequestBudget,
    retries: u32,
) -> Result<Value, JevProviderError> {
    if key.expose_secret().chars().any(char::is_control) {
        return Err(err(
            "invalidJevConfiguration",
            "OCTOCODE_JEV_KEY contains invalid control characters.",
            "Replace the configured key.",
        ));
    }
    let parsed = parse_and_locate(query, default_model)?;
    let request = build_request(&parsed.claim, &parsed.model, &parsed.located, &parsed.dims);
    let response = post(&request, &key, endpoint(base_url)?, &budget, retries).await?;
    validate_answers(&response, &request)?;
    Ok(build_results(&parsed, &response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Command;

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) {
        let mut file = std::fs::File::create(dir.join(name)).expect("create fixture file");
        file.write_all(bytes).expect("write fixture file");
    }

    fn score_answer(scores: &[(usize, f64)], score: f64) -> Value {
        let mut probs = Map::new();
        for (index, prob) in scores {
            probs.insert(index.to_string(), json!(prob));
        }
        json!({ "score": score, "probabilities": Value::Object(probs) })
    }

    #[test]
    fn truncated_rejections_remain_required_reads_and_labels_use_the_mode() {
        let parsed = parse_and_locate(
            &json!({
                "claim": "implements parsing", "itemSpanBudget": 200,
                "source": { "items": [
                    { "id": "cut", "content": "x".repeat(400) },
                    { "id": "whole", "content": "complete excerpt" }
                ] }
            }),
            "jev-test",
        )
        .unwrap();
        let output = build_results(
            &parsed,
            &json!({ "answers": {
                "candidate_0": score_answer(&[(0, 1.0), (1, 0.0), (2, 0.0), (3, 0.0)], 0.0),
                "candidate_1": score_answer(&[(0, 0.49), (1, 0.0), (2, 0.0), (3, 0.51)], 1.53)
            }}),
        );
        assert_eq!(output["results"]["cut"]["action"], "gray_read");
        assert_eq!(output["results"]["cut"]["truncated"], true);
        assert_eq!(output["results"]["whole"]["level"], "implements");
        assert_eq!(output["requiredReads"], json!(["cut", "whole"]));
    }

    #[test]
    fn malformed_probability_values_cannot_produce_skip_verdicts() {
        let response = json!({"answers": {"a": score_answer(&[(0, 2.0), (1, -1.0)], 0.0)}});
        assert!(
            validate_answers(
                &response,
                &json!({"questions":{"a":{"type":"score","criteria":["low","high"]}}})
            )
            .is_err()
        );
    }

    // ---- locate / merge / anchors / coverage ------------------------------

    #[test]
    fn locate_merges_all_anchor_windows_with_sources_and_coverage() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_file(
            root,
            "a.txt",
            b"x\nhash here\ny\nz\nw\nq\nr\ns\nt\nu\ndigest too\nv",
        );
        let anchors = vec!["hash".to_owned(), "digest".to_owned()];
        let loc = locate_spans(root, "a.txt", &anchors, 1, SPAN_BUDGET).expect("locate");
        assert_eq!(loc.spans.len(), 2);
        assert_eq!(loc.spans[0].source, "a.txt:L1-L3");
        assert_eq!(loc.spans[1].source, "a.txt:L10-L12");
        assert!(loc.coverage > 0.0 && loc.coverage <= 1.0);
    }

    #[test]
    fn locate_single_line_window_matches_reference() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_file(root, "a.mjs", b"const h = createHash(\"sha256\")");
        write_file(root, "b.mjs", b"export const x = 1");
        let anchors = vec!["createHash".to_owned()];
        let a = locate_spans(root, "a.mjs", &anchors, WINDOW, SPAN_BUDGET).expect("locate a");
        assert_eq!(
            a.spans.iter().map(|s| s.source.clone()).collect::<Vec<_>>(),
            vec!["a.mjs:L1-L1".to_owned()]
        );
        let b = locate_spans(root, "b.mjs", &anchors, WINDOW, SPAN_BUDGET).expect("locate b");
        assert!(b.spans.is_empty());
        assert_eq!(b.coverage, 0.0);
    }

    // ---- sandbox fail-closed ---------------------------------------------

    #[test]
    fn sandbox_rejects_absolute_and_traversal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_file(root, "s.txt", b"hash inside");
        let anchors = vec!["x".to_owned()];
        let absolute = locate_spans(root, "/etc/passwd", &anchors, WINDOW, SPAN_BUDGET);
        let message = absolute.expect_err("absolute path must fail").message;
        assert!(message.contains("absolute"), "message: {message}");
        let traversal = locate_spans(root, "../out.txt", &anchors, WINDOW, SPAN_BUDGET);
        let message = traversal.expect_err("traversal must fail").message;
        assert!(
            message.contains("escapes") || message.contains("sandbox"),
            "message: {message}"
        );
    }

    // ---- redaction applied ------------------------------------------------

    #[test]
    fn locate_redacts_secret_tokens() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let secret = format!("ghp_{}", "a".repeat(37));
        write_file(root, "s.txt", format!("token={secret} hash").as_bytes());
        let anchors = vec!["hash".to_owned()];
        let loc = locate_spans(root, "s.txt", &anchors, WINDOW, SPAN_BUDGET).expect("locate");
        assert_eq!(loc.spans.len(), 1);
        assert!(
            !loc.spans[0].content.contains(&secret),
            "secret leaked: {}",
            loc.spans[0].content
        );
    }

    // ---- request shape (single + multi dim) ------------------------------

    #[test]
    fn request_shape_single_dimension() {
        let claim = "does X";
        let dims = vec![Dim {
            key: "main".to_owned(),
            role: "primary".to_owned(),
            claim: claim.to_owned(),
            levels: default_levels(),
        }];
        let located = vec![
            (
                "a.mjs".to_owned(),
                Located {
                    spans: vec![Span {
                        source: "a.mjs:L1-L2".to_owned(),
                        content: "code".to_owned(),
                    }],
                    coverage: 0.5,
                    file_chars: 8,
                    truncated: false,
                },
            ),
            (
                "b.mjs".to_owned(),
                Located {
                    spans: vec![],
                    coverage: 0.0,
                    file_chars: 9,
                    truncated: false,
                },
            ),
        ];
        let request = build_request(claim, "jev-latest", &located, &dims);
        let questions = request["questions"].as_object().expect("questions");
        assert_eq!(questions.len(), 2);
        assert_eq!(request["questions"]["candidate_0"]["type"], "score");
        assert_eq!(
            request["questions"]["candidate_0"]["criteria"]
                .as_array()
                .expect("criteria")
                .len(),
            4
        );
        assert!(request["questions"]["candidate_0"]["instructions"].is_object());
        assert_eq!(
            request["state"]["candidates"]["b.mjs"],
            "no anchor matches in this file"
        );
    }

    #[test]
    fn request_shape_multi_dimension_ids() {
        let claim = "which PR fixes X";
        let dims = vec![
            Dim {
                key: "relevance".to_owned(),
                role: "primary".to_owned(),
                claim: claim.to_owned(),
                levels: vec![
                    json!({"level":"no"}),
                    json!({"level":"maybe"}),
                    json!({"level":"yes"}),
                ],
            },
            Dim {
                key: "is_fix".to_owned(),
                role: "veto".to_owned(),
                claim: claim.to_owned(),
                levels: vec![json!({"level":"not_a_fix"}), json!({"level":"fix"})],
            },
        ];
        let located = vec![
            (
                "a".to_owned(),
                Located {
                    spans: vec![Span {
                        source: "a".to_owned(),
                        content: "fixes X properly".to_owned(),
                    }],
                    coverage: 1.0,
                    file_chars: 16,
                    truncated: false,
                },
            ),
            (
                "b".to_owned(),
                Located {
                    spans: vec![Span {
                        source: "b".to_owned(),
                        content: "docs update".to_owned(),
                    }],
                    coverage: 1.0,
                    file_chars: 11,
                    truncated: false,
                },
            ),
        ];
        let request = build_request(claim, "jev-latest", &located, &dims);
        let mut keys: Vec<String> = request["questions"]
            .as_object()
            .expect("questions")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "candidate_0__is_fix",
                "candidate_0__relevance",
                "candidate_1__is_fix",
                "candidate_1__relevance",
            ]
        );
    }

    #[test]
    fn request_ids_do_not_collide_for_similar_candidate_names() {
        let dims = vec![Dim {
            key: "main".to_owned(),
            role: "primary".to_owned(),
            claim: "implements X".to_owned(),
            levels: default_levels(),
        }];
        let located = vec![
            (
                "a-b".to_owned(),
                Located {
                    spans: vec![Span {
                        source: "a-b".to_owned(),
                        content: "one".to_owned(),
                    }],
                    coverage: 1.0,
                    file_chars: 3,
                    truncated: false,
                },
            ),
            (
                "a/b".to_owned(),
                Located {
                    spans: vec![Span {
                        source: "a/b".to_owned(),
                        content: "two".to_owned(),
                    }],
                    coverage: 1.0,
                    file_chars: 3,
                    truncated: false,
                },
            ),
        ];
        let request = build_request("implements X", "jev-latest", &located, &dims);
        let questions = request["questions"].as_object().expect("questions");
        assert_eq!(questions.len(), 2);
        assert!(questions.contains_key("candidate_0"));
        assert!(questions.contains_key("candidate_1"));
    }

    // ---- policy v2 table --------------------------------------------------

    #[test]
    fn policy_v2_table() {
        // no evidence -> skip
        assert_eq!(apply_policy(&json!({}), false, 4), ("skip", "no_evidence"));
        // argmax top -> read
        assert_eq!(
            apply_policy(
                &score_answer(&[(0, 0.0), (1, 0.0), (2, 0.1), (3, 0.9)], 3.0),
                true,
                4
            ),
            ("read", "argmax_top")
        );
        // P(top) <= 0.25 -> skip
        assert_eq!(
            apply_policy(
                &score_answer(&[(0, 0.0), (1, 0.0), (2, 0.9), (3, 0.1)], 2.0),
                true,
                4
            ),
            ("skip", "judged_and_rejected")
        );
        // middle mass on top -> gray_read (fail-open)
        assert_eq!(
            apply_policy(
                &score_answer(&[(0, 0.0), (1, 0.0), (2, 0.55), (3, 0.45)], 2.4),
                true,
                4
            ),
            ("gray_read", "fail_open")
        );
    }

    // ---- veto demote-only + info reports only ----------------------------

    #[test]
    fn veto_demotes_read_only_and_info_reports_only() {
        let claim = "which PR fixes X";
        let dims = vec![
            Dim {
                key: "relevance".to_owned(),
                role: "primary".to_owned(),
                claim: claim.to_owned(),
                levels: vec![
                    json!({"level":"no"}),
                    json!({"level":"maybe"}),
                    json!({"level":"yes"}),
                ],
            },
            Dim {
                key: "is_fix".to_owned(),
                role: "veto".to_owned(),
                claim: claim.to_owned(),
                levels: vec![json!({"level":"not_a_fix"}), json!({"level":"fix"})],
            },
        ];
        let parsed = Parsed {
            claim: claim.to_owned(),
            model: "jev-latest".to_owned(),
            dims,
            located: vec![(
                "a".to_owned(),
                Located {
                    spans: vec![Span {
                        source: "a".to_owned(),
                        content: "fixes X".to_owned(),
                    }],
                    coverage: 1.0,
                    file_chars: 7,
                    truncated: false,
                },
            )],
        };
        // primary argmax == top(2) -> read; veto argmax == 0 -> demote to gray_read
        let response = json!({
            "model": "jev-1",
            "usage": {},
            "answers": {
                "candidate_0__relevance": score_answer(&[(0, 0.0), (1, 0.1), (2, 0.9)], 2.0),
                "candidate_0__is_fix": score_answer(&[(0, 0.8), (1, 0.2)], 0.2),
            }
        });
        let out = build_results(&parsed, &response);
        assert_eq!(out["results"]["a"]["action"], "gray_read");
        assert_eq!(out["results"]["a"]["reason"], "vetoed_by_is_fix");
        assert!(out["results"]["a"].get("dimensions").is_some());
        assert_eq!(out["reads"].as_array().expect("reads").len(), 0);

        // veto not at bottom -> read preserved
        let response = json!({
            "model": "jev-1",
            "usage": {},
            "answers": {
                "candidate_0__relevance": score_answer(&[(0, 0.0), (1, 0.1), (2, 0.9)], 2.0),
                "candidate_0__is_fix": score_answer(&[(0, 0.2), (1, 0.8)], 0.8),
            }
        });
        let out = build_results(&parsed, &response);
        assert_eq!(out["results"]["a"]["action"], "read");
        assert_eq!(out["results"]["a"]["reason"], "argmax_top");
    }

    // ---- items mode -------------------------------------------------------

    #[test]
    fn items_mode_bounds_and_redacts() {
        let items = vec![
            json!({ "id": "PR#12", "content": "Fix flaky retry test", "source": "octo/repo#12" }),
            json!({ "id": "PR#9", "content": "Update README badges" }),
        ];
        let located = locate_items(&items, SPAN_BUDGET).expect("locate items");
        assert_eq!(located.len(), 2);
        assert_eq!(located[0].0, "PR#12");
        assert_eq!(located[0].1.spans[0].source, "octo/repo#12");
        assert_eq!(located[1].0, "PR#9");
        // full content, no truncation -> coverage 1
        assert_eq!(located[1].1.coverage, 1.0);
        assert_eq!(located[1].1.spans[0].source, "PR#9");
    }

    #[test]
    fn items_mode_requires_id_and_content() {
        let items = vec![json!({ "id": "a" }), json!({ "id": "b", "content": "ok" })];
        let error = locate_items(&items, SPAN_BUDGET).expect_err("missing content must fail");
        assert!(
            error.message.contains("id, content"),
            "message: {}",
            error.message
        );
    }

    // ---- dimensions normalization ----------------------------------------

    #[test]
    fn dimensions_default_is_single_primary() {
        let dims = normalize_dimensions(&json!({}), "claim").expect("default dims");
        assert_eq!(dims.len(), 1);
        assert_eq!(dims[0].role, "primary");
        assert_eq!(dims[0].key, "main");
        assert_eq!(dims[0].levels.len(), 4);
    }

    #[test]
    fn dimensions_require_exactly_one_primary() {
        let query = json!({ "dimensions": [
            { "key": "a", "role": "veto", "levels": [{"level":"no"},{"level":"yes"}] }
        ]});
        let error = normalize_dimensions(&query, "claim").expect_err("no primary");
        assert!(
            error.message.contains("exactly one primary"),
            "message: {}",
            error.message
        );
    }

    #[test]
    fn dimensions_bounds_and_unique_keys() {
        let five: Vec<Value> = (0..5)
            .map(|n| json!({ "key": format!("d{n}"), "role": if n == 0 { "primary" } else { "info" }, "levels": [{"level":"a"},{"level":"b"}] }))
            .collect();
        let error =
            normalize_dimensions(&json!({ "dimensions": five }), "c").expect_err("too many");
        assert!(error.message.contains("1..4"), "message: {}", error.message);

        let dup = json!({ "dimensions": [
            { "key": "x", "role": "primary", "levels": [{"level":"a"},{"level":"b"}] },
            { "key": "x", "role": "info", "levels": [{"level":"a"},{"level":"b"}] }
        ]});
        let error = normalize_dimensions(&dup, "c").expect_err("dup keys");
        assert!(
            error.message.contains("unique"),
            "message: {}",
            error.message
        );
    }

    // ---- top-level validation via parse_and_locate -----------------------

    #[test]
    fn parse_rejects_missing_claim() {
        let query = json!({ "source": { "items": [ {"id":"a","content":"x"}, {"id":"b","content":"y"} ] } });
        let error = parse_and_locate(&query, "jev-latest").expect_err("missing claim");
        assert!(
            error.message.contains("claim"),
            "message: {}",
            error.message
        );
    }

    #[test]
    fn parse_rejects_both_or_neither_source() {
        let both = json!({
            "claim": "x",
            "source": {
                "local": { "candidates": ["a", "b"], "anchors": ["z"] },
                "items": [ {"id":"a","content":"x"}, {"id":"b","content":"y"} ]
            }
        });
        let error = parse_and_locate(&both, "jev-latest").expect_err("both sources");
        assert!(
            error.message.contains("exactly one"),
            "message: {}",
            error.message
        );

        let neither = json!({ "claim": "x", "source": {} });
        let error = parse_and_locate(&neither, "jev-latest").expect_err("neither source");
        assert!(
            error.message.contains("exactly one"),
            "message: {}",
            error.message
        );
    }

    #[test]
    fn parse_rejects_candidate_bounds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let query = json!({
            "claim": "x",
            "source": { "local": { "root": dir.path().to_string_lossy(), "candidates": ["only-one"], "anchors": ["y"] } }
        });
        let error = parse_and_locate(&query, "jev-latest").expect_err("too few candidates");
        assert!(
            error.message.contains("2..12"),
            "message: {}",
            error.message
        );
    }

    #[test]
    fn parse_uses_item_span_budget_and_rejects_duplicate_ids() {
        let query = json!({
            "claim": "x",
            "itemSpanBudget": 200,
            "source": { "items": [
                {"id":"a","content":"a".repeat(400)},
                {"id":"b","content":"b".repeat(400)}
            ] }
        });
        let parsed = parse_and_locate(&query, "jev-latest").expect("valid items");
        assert_eq!(parsed.located[0].1.spans[0].content, "a".repeat(200));
        assert_eq!(parsed.located[0].1.coverage, 0.5);

        let duplicate = json!({
            "claim": "x",
            "source": { "items": [
                {"id":"same","content":"one"},
                {"id":"same","content":"two"}
            ] }
        });
        let error = parse_and_locate(&duplicate, "jev-latest").expect_err("duplicate IDs");
        assert!(
            error.message.contains("unique"),
            "message: {}",
            error.message
        );
    }

    #[test]
    fn parse_rejects_question_budget() {
        let dims: Vec<Value> = (0..4)
            .map(|n| json!({ "key": format!("d{n}"), "role": if n == 0 { "primary" } else { "info" }, "levels": [{"level":"a"},{"level":"b"}] }))
            .collect();
        let items: Vec<Value> = (0..7)
            .map(|n| json!({ "id": format!("i{n}"), "content": "c" }))
            .collect();
        let query = json!({ "claim": "x", "dimensions": dims, "source": { "items": items } });
        let error = parse_and_locate(&query, "jev-latest").expect_err("budget");
        assert!(
            error.message.contains("at most 24 questions"),
            "message: {}",
            error.message
        );
    }

    #[test]
    fn validate_answers_rejects_missing_and_malformed() {
        let request = json!({"questions":{"a":{"type":"score","criteria":["low","high"]}}});
        let good = json!({"model":"jev-test","usage":{"input_tokens":1,"output_tokens":1},"answers":{
            "a":{"type":"score","score":1.0,"probabilities":{"0":0.0,"1":1.0},"confidence":1.0,"legend":{"0":"low","1":"high"}}
        }});
        assert!(validate_answers(&good, &request).is_ok());
        for pointer in [
            "/model",
            "/answers/a/type",
            "/answers/a/score",
            "/answers/a/probabilities",
            "/answers/a/confidence",
            "/answers/a/legend",
        ] {
            let mut malformed = good.clone();
            *malformed.pointer_mut(pointer).unwrap() = Value::Null;
            assert!(validate_answers(&malformed, &request).is_err(), "{pointer}");
        }
        for probabilities in [
            json!({"0":2.0,"1":-1.0}),
            json!({"0":0.1,"1":0.1}),
            json!({"0":0.0,"2":1.0}),
        ] {
            let mut malformed = good.clone();
            malformed["answers"]["a"]["probabilities"] = probabilities;
            assert!(validate_answers(&malformed, &request).is_err());
        }
        let mut extra = good.clone();
        extra["answers"]["extra"] = extra["answers"]["a"].clone();
        assert!(validate_answers(&extra, &request).is_err());
    }

    // -------------------------------------------------------------------
    // EDGE-ENCODING PROPERTY TESTS (release-blocking per RFC risk R2).
    //
    // Each fixture is run through BOTH this native locate and the JS
    // reference `scout.mjs --dry-run` on the SAME bytes; we assert the JS
    // dry-run `candidates` section (span sources + coverage) and its
    // `request.state.candidates` (span content) agree with native output.
    //
    // Truth definition for the divergence documented at the top of this file:
    // span sources / line ranges must match for EVERY encoding; content-length
    // (and coverage, and exact content) equality is asserted only where the
    // UTF-16 (JS) and char-boundary (Rust) truncation points provably coincide
    // — i.e. ASCII/BMP fixtures with no redactable tokens.
    // -------------------------------------------------------------------

    fn scout_script() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../skills/octocode-jev-reasoning-loop/scripts/scout.mjs")
    }

    fn run_js_dry_run(input_path: &Path) -> Value {
        let output = Command::new("node")
            .arg(scout_script())
            .arg("--input")
            .arg(input_path)
            .arg("--dry-run")
            .output()
            .expect("node must be available to run the JS reference scout");
        assert!(
            output.status.success(),
            "scout.mjs --dry-run failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("scout.mjs emits JSON")
    }

    /// Compare native locate to the JS dry-run for a local-mode fixture.
    /// `assert_content` gates the ASCII/BMP-only content-length + coverage checks.
    fn assert_local_parity(
        root: &Path,
        candidates: &[&str],
        anchors: &[&str],
        window: u64,
        span_budget: u64,
        assert_content: bool,
    ) {
        let input = json!({
            "claim": "capability under test",
            "root": root.to_string_lossy(),
            "candidates": candidates,
            "anchors": anchors,
            "window": window,
            "spanBudget": span_budget,
        });
        let input_path = root.join("scout-input.json");
        write_file(root, "scout-input.json", input.to_string().as_bytes());
        let js = run_js_dry_run(&input_path);

        let anchors_owned: Vec<String> = anchors.iter().map(|a| a.to_string()).collect();
        for file in candidates {
            let loc = locate_spans(root, file, &anchors_owned, window, span_budget)
                .expect("native locate");
            let js_sources: Vec<String> = js["candidates"][file]["spans"]
                .as_array()
                .expect("js spans")
                .iter()
                .map(|s| s.as_str().expect("source string").to_owned())
                .collect();
            let native_sources: Vec<String> = loc.spans.iter().map(|s| s.source.clone()).collect();
            assert_eq!(
                native_sources, js_sources,
                "span sources must match for {file}"
            );
            // Every native span content is valid UTF-8 by construction.
            if assert_content {
                let js_coverage = js["candidates"][file]["coverage"].as_f64().expect("cov");
                assert_eq!(round3(loc.coverage), round3(js_coverage), "coverage {file}");
                let js_state = &js["request"]["state"]["candidates"][file];
                if let Some(js_spans) = js_state.as_array() {
                    assert_eq!(js_spans.len(), loc.spans.len(), "span count {file}");
                    for (native, js_span) in loc.spans.iter().zip(js_spans) {
                        let js_content = js_span["content"].as_str().expect("js content");
                        assert_eq!(native.content, js_content, "content {file}");
                        assert_eq!(
                            utf16_len(&native.content),
                            utf16_len(js_content),
                            "content length {file}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn property_crlf_and_over_cap_ascii_match_js() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // CRLF line endings — '\r' stays attached to each line in both impls.
        write_file(
            root,
            "crlf.txt",
            b"alpha\r\nANCHOR line\r\nbeta\r\ngamma\r\ndelta\r\n",
        );
        // Over-cap file: a small spanBudget forces truncation at an ASCII boundary.
        let mut big = String::new();
        for n in 0..60 {
            if n == 30 {
                big.push_str("ANCHOR here\n");
            } else {
                big.push_str("filler line of ascii text\n");
            }
        }
        write_file(root, "big.txt", big.as_bytes());
        assert_local_parity(root, &["crlf.txt", "big.txt"], &["ANCHOR"], 6, 120, true);
    }

    #[test]
    fn property_bom_and_cjk_match_js_sources() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // UTF-8 BOM prefix (U+FEFF) — Node utf8 read does not strip it; both keep it.
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice("head\nANCHOR value\ntail\n".as_bytes());
        write_file(root, "bom.txt", &bom);
        // CJK (BMP, multi-byte) content — 1 UTF-16 unit per char, boundaries align.
        write_file(
            root,
            "cjk.txt",
            "一二三\nANCHOR 四五六七八九十\n甲乙丙\n".as_bytes(),
        );
        // Sources + coverage + content all provably align for BOM/CJK (BMP).
        assert_local_parity(root, &["bom.txt", "cjk.txt"], &["ANCHOR"], 6, 3000, true);
    }

    #[test]
    fn property_emoji_straddle_matches_sources_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        // Astral emoji (surrogate pairs) straddling the spanBudget truncation
        // point: JS may cut mid-surrogate; Rust stops on a char boundary. Only
        // span sources/line ranges are asserted (single span => no cascade).
        let mut content = String::from("ANCHOR ");
        for _ in 0..40 {
            content.push('😀');
        }
        content.push('\n');
        write_file(root, "emoji.txt", content.as_bytes());
        write_file(root, "plain.txt", b"ANCHOR plain ascii tail\n");
        assert_local_parity(root, &["emoji.txt", "plain.txt"], &["ANCHOR"], 6, 25, false);
        // The Rust span content must be valid UTF-8 and non-empty.
        let loc = locate_spans(root, "emoji.txt", &["ANCHOR".to_owned()], 6, 25).expect("locate");
        assert_eq!(loc.spans.len(), 1);
        assert!(
            loc.spans[0]
                .content
                .is_char_boundary(loc.spans[0].content.len())
        );
    }

    #[test]
    fn property_items_mode_matches_js() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let input = json!({
            "claim": "which item addresses the question",
            "items": [
                { "id": "PR#12", "content": "Fix flaky retry test by pinning timers", "source": "octo/repo#12" },
                { "id": "PR#9", "content": "Update README badges only" }
            ],
            "spanBudget": 20,
        });
        write_file(root, "items.json", input.to_string().as_bytes());
        let js = run_js_dry_run(&root.join("items.json"));
        let items = input["items"].as_array().expect("items").clone();
        let located = locate_items(&items, 20).expect("locate items");
        for (id, loc) in &located {
            let js_sources: Vec<String> = js["candidates"][id]["spans"]
                .as_array()
                .expect("js spans")
                .iter()
                .map(|s| s.as_str().expect("source").to_owned())
                .collect();
            assert_eq!(
                vec![loc.spans[0].source.clone()],
                js_sources,
                "sources {id}"
            );
            let js_coverage = js["candidates"][id]["coverage"].as_f64().expect("cov");
            assert_eq!(round3(loc.coverage), round3(js_coverage), "coverage {id}");
            let js_content = js["request"]["state"]["candidates"][id][0]["content"]
                .as_str()
                .expect("js content");
            assert_eq!(loc.spans[0].content, js_content, "content {id}");
        }
    }

    #[test]
    fn property_redaction_fires_in_both_impls() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let secret = format!("ghp_{}", "a".repeat(37));
        let input = json!({
            "claim": "which item fixes the retry bug",
            "items": [
                { "id": "PR#12", "content": format!("Fix retry token={secret} done") },
                { "id": "PR#9", "content": "Docs only change" }
            ],
        });
        write_file(root, "redact.json", input.to_string().as_bytes());
        let js = run_js_dry_run(&root.join("redact.json"));
        // JS redaction fires (its own «redacted-token» marker) and the raw secret
        // is gone from the JS request state.
        let js_state = js["request"]["state"]["candidates"]["PR#12"].to_string();
        assert!(
            js_state.contains("redacted"),
            "js did not redact: {js_state}"
        );
        assert!(!js_state.contains(&secret), "js leaked secret");
        // Native redaction fires with its own marker; raw secret is gone.
        let items = input["items"].as_array().expect("items").clone();
        let located = locate_items(&items, SPAN_BUDGET).expect("locate items");
        assert!(
            !located[0].1.spans[0].content.contains(&secret),
            "native leaked secret: {}",
            located[0].1.spans[0].content
        );
        assert_ne!(
            located[0].1.spans[0].content, "Fix retry token= done",
            "native must transform the secret-bearing content"
        );
    }
}
