//! Optional semantic reranking embedded in `ghSearch` and `localSearch`.
//! The capability-gated schema is authored in `@octocodeai/octocode-core`
//! (`semanticRerank.ts`); this module strips those fields before canonical
//! contract validation, then applies them to already-sanitized search entries.
use super::dispatch::DomainResult;
use serde_json::{Map, Value, json};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

const MAX_CANDIDATES: usize = 8;
/// Top score below this marks the ranking as uninformative (`lowSignal`).
const LOW_SIGNAL_SCORE: f64 = 0.4;
const MAX_QUESTIONS: usize = 5;
const MAX_CELLS: usize = 25;
const ID_MAX_CHARS: usize = 64;

#[derive(Clone, Debug)]
pub(super) struct SemanticRerankSpec {
    pub questions: Vec<Value>,
}

#[derive(Debug)]
pub(super) struct SemanticRerankError(pub String);

#[derive(Debug)]
pub(super) struct SemanticRerankJob {
    pub row_index: usize,
    pub matrix: Value,
}

fn semantic_entry_is_valid(value: &Value) -> bool {
    match value {
        Value::String(value) => !value.trim().is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty() && value.len() <= 100,
        _ => false,
    }
}

fn valid_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let count = 1 + chars.clone().count();
    count <= ID_MAX_CHARS
        && first.is_ascii_alphanumeric()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn parse_spec(
    query: &mut Map<String, Value>,
) -> Result<Option<SemanticRerankSpec>, SemanticRerankError> {
    let Some(value) = query.remove("semanticRerank") else {
        return Ok(None);
    };
    let rerank = value.as_object().ok_or_else(|| {
        SemanticRerankError("semanticRerank must be an object with questions.".into())
    })?;
    if rerank.keys().any(|key| key != "questions") {
        return Err(SemanticRerankError(
            "semanticRerank accepts only questions; it reorders files and never removes one."
                .into(),
        ));
    }
    let questions = rerank
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SemanticRerankError(
                "semanticRerank.questions must be an array of one to five questions.".into(),
            )
        })?;
    if questions.is_empty() || questions.len() > MAX_QUESTIONS {
        return Err(SemanticRerankError(
            "semanticRerank.questions must contain one to five questions.".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut parsed = Vec::with_capacity(questions.len());
    for (index, question) in questions.iter().enumerate() {
        let object = question.as_object().ok_or_else(|| {
            SemanticRerankError(format!(
                "semanticRerank.questions[{index}] must be an object."
            ))
        })?;
        if object.len() != 2 || !object.contains_key("id") || !object.contains_key("question") {
            return Err(SemanticRerankError(format!(
                "semanticRerank.questions[{index}] accepts only id and question."
            )));
        }
        let id = object["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or_else(|| {
                SemanticRerankError(format!("semanticRerank.questions[{index}].id is invalid."))
            })?;
        if !seen.insert(id.to_owned()) {
            return Err(SemanticRerankError(format!(
                "Duplicate semantic rerank question id: {id}."
            )));
        }
        if !semantic_entry_is_valid(&object["question"]) {
            return Err(SemanticRerankError(format!(
                "semanticRerank.questions[{index}].question must be a non-empty string, object, or array."
            )));
        }
        parsed.push(question.clone());
    }
    let max_candidates = MAX_CANDIDATES.min(MAX_CELLS / parsed.len());
    match query.get("pageSize").and_then(Value::as_u64) {
        Some(page_size) if page_size as usize > max_candidates => {
            return Err(SemanticRerankError(format!(
                "semanticRerank scores at most {max_candidates} files per page ({MAX_CANDIDATES} max, fewer with more questions: {} question(s) × files ≤ {MAX_CELLS}); set pageSize ≤ {max_candidates} and continue with semanticRerank.next.",
                parsed.len()
            )));
        }
        None => {
            query.insert("pageSize".into(), json!(max_candidates));
        }
        _ => {}
    }
    Ok(Some(SemanticRerankSpec { questions: parsed }))
}

fn validate_mode(
    tool: &str,
    query: &Map<String, Value>,
    enabled: bool,
) -> Result<(), SemanticRerankError> {
    if !enabled {
        return Ok(());
    }
    match tool {
        "ghSearch" => {
            if query.get("operation").and_then(Value::as_str) != Some("code") {
                return Err(SemanticRerankError(
                    "semanticRerank is supported only by ghSearch operation:\"code\".".into(),
                ));
            }
            if query
                .get("match")
                .and_then(Value::as_str)
                .is_some_and(|value| value != "file")
            {
                return Err(SemanticRerankError(
                    "ghSearch semantic reranking requires match:\"file\".".into(),
                ));
            }
            if query.get("concise").and_then(Value::as_bool) == Some(true) {
                return Err(SemanticRerankError(
                    "ghSearch semantic reranking is unavailable for concise output.".into(),
                ));
            }
        }
        "localSearch"
            if query
                .get("resultView")
                .and_then(Value::as_str)
                .is_some_and(|value| value != "paginated") =>
        {
            return Err(SemanticRerankError(
                "localSearch semantic reranking requires resultView:\"paginated\".".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}

fn extract_one(
    tool: &str,
    query: &mut Value,
) -> Result<Option<SemanticRerankSpec>, SemanticRerankError> {
    let object = query
        .as_object_mut()
        .ok_or_else(|| SemanticRerankError("Search query must be an object.".into()))?;
    let spec = parse_spec(object)?;
    validate_mode(tool, object, spec.is_some())?;
    Ok(spec)
}

/// Remove addon-only fields before the canonical core contract validates the
/// ordinary search query. Returned specs retain input order.
pub(super) fn extract(
    tool: &str,
    mut input: Value,
) -> Result<(Value, Vec<Option<SemanticRerankSpec>>), SemanticRerankError> {
    if !matches!(tool, "ghSearch" | "localSearch") {
        return Ok((input, Vec::new()));
    }
    let specs = if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
        queries
            .iter_mut()
            .map(|query| extract_one(tool, query))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![extract_one(tool, &mut input)?]
    };
    Ok((input, specs))
}

pub(super) fn has_requests(specs: &[Option<SemanticRerankSpec>]) -> bool {
    specs.iter().any(Option::is_some)
}

pub(super) fn sanitize_specs(
    specs: &mut [Option<SemanticRerankSpec>],
    security: &crate::security::ContentSecurity,
) -> Result<(), SemanticRerankError> {
    for spec in specs.iter_mut().flatten() {
        let checked = security.validate_input_parameters(&json!({"questions": spec.questions}));
        if !checked.is_valid {
            return Err(SemanticRerankError(format!(
                "Security validation failed for semanticRerank: {}",
                checked.warnings.join("; ")
            )));
        }
        spec.questions = checked
            .sanitized_params
            .get("questions")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                SemanticRerankError("Cannot sanitize semanticRerank.questions.".into())
            })?;
    }
    Ok(())
}

pub(super) fn restore_query(query: &mut Value, spec: &SemanticRerankSpec) {
    let Some(object) = query.as_object_mut() else {
        return;
    };
    object.insert(
        "semanticRerank".into(),
        json!({"questions": spec.questions}),
    );
}

pub(super) fn response_query(queries: &[Value], specs: &[Option<SemanticRerankSpec>]) -> Value {
    let mut restored = queries.to_vec();
    for (query, spec) in restored
        .iter_mut()
        .zip(specs)
        .filter_map(|(query, spec)| spec.as_ref().map(|spec| (query, spec)))
    {
        restore_query(query, spec);
    }
    if restored.len() == 1 {
        restored.pop().unwrap_or(Value::Null)
    } else {
        json!({"queries":restored})
    }
}

fn continuation_supports(tool: &str, query: &Value) -> bool {
    match tool {
        "ghSearch" => {
            query.get("operation").and_then(Value::as_str) == Some("code")
                && query
                    .get("match")
                    .and_then(Value::as_str)
                    .is_none_or(|value| value == "file")
                && query.get("concise").and_then(Value::as_bool) != Some(true)
        }
        "localSearch" => query.get("resultView").and_then(Value::as_str) == Some("paginated"),
        _ => false,
    }
}

/// A later match page of the same files (`matchPage > 1`) or a call that
/// keeps the current page re-scores files already ranked here. Single-page
/// results carry no top-level `pagination`, so the match page is the signal.
fn same_file_page(query: &Value, current_page: Option<&Value>) -> bool {
    query
        .get("matchPage")
        .and_then(Value::as_u64)
        .is_some_and(|page| page > 1)
        || current_page.is_some_and(|page| query.get("page") == Some(page))
}

/// Reranked continuations must reach a new candidate page; same-page
/// continuations (more matches inside the same files) would re-score the
/// files already ranked here.
fn attach_continuation(data: &mut Value, tool: &str, spec: &SemanticRerankSpec) {
    let current_page = data.pointer("/pagination/currentPage").cloned();
    let Some(calls) = data.get_mut("next").and_then(Value::as_object_mut) else {
        return;
    };
    // Move (not copy) reranking-capable calls: the canonical contract cannot
    // carry `semanticRerank`, and keeping both would duplicate the query and
    // its cursor. Same-page calls stay in `data.next`.
    let mut moved = Map::new();
    for (name, call) in std::mem::take(calls) {
        let reranks = call.get("tool").and_then(Value::as_str) == Some(tool)
            && call.get("query").is_some_and(|query| {
                continuation_supports(tool, query) && !same_file_page(query, current_page.as_ref())
            });
        if reranks {
            let mut call = call;
            if let Some(query) = call.get_mut("query") {
                restore_query(query, spec);
            }
            moved.insert(name, call);
        } else {
            calls.insert(name, call);
        }
    }
    if calls.is_empty()
        && let Some(object) = data.as_object_mut()
    {
        object.remove("next");
    }
    if !moved.is_empty() {
        data["semanticRerankContinuation"] = Value::Object(moved);
    }
}

fn take_continuation(data: &mut Value) -> Option<Value> {
    data.as_object_mut()?.remove("semanticRerankContinuation")
}

/// Safety cap on matches per candidate. Keep it above one result page: a
/// five-snippet cap dropped the implementing hunk and demoted the true target
/// from #1 to #4 in a live check, so only routing metadata is trimmed.
const MAX_CANDIDATE_SNIPPETS: usize = 50;
const SNIPPET_FIELDS: [&str; 6] = ["line", "value", "text", "fragment", "snippet", "content"];

/// Provider state for one candidate: identity plus its first snippets, without
/// pagination counters, columns, or other routing metadata.
fn candidate_state(file: &Value) -> Value {
    let Some(entry) = file.as_object() else {
        return file.clone();
    };
    let mut state = Map::new();
    for field in ["owner", "repo", "path"] {
        if let Some(value) = entry.get(field).filter(|value| value.is_string()) {
            state.insert(field.into(), value.clone());
        }
    }
    if let Some(matches) = entry.get("matches").and_then(Value::as_array) {
        let snippets = matches
            .iter()
            .take(MAX_CANDIDATE_SNIPPETS)
            .map(|item| match item.as_object() {
                Some(fields) => Value::Object(
                    fields
                        .iter()
                        .filter(|(key, _)| SNIPPET_FIELDS.contains(&key.as_str()))
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                ),
                None => item.clone(),
            })
            .collect::<Vec<_>>();
        state.insert("matches".into(), Value::Array(snippets));
    }
    if state.is_empty() {
        file.clone()
    } else {
        Value::Object(state)
    }
}

/// Words that make a question *about* callers, tests, docs, or config, where
/// the implementer criteria below would contradict it.
const NON_IMPLEMENTER_TOPICS: [&str; 9] = [
    "test", "doc", "call", "example", "config", "readme", "usage", "import", "comment",
];

/// Noul criteria that separate the implementer from files that merely mention
/// the behavior (callers and tests outranked implementers in held-out evals;
/// the provider's rerank recipe fixes true/false this way). Skipped when the
/// question itself targets callers, tests, docs, or config.
fn implementer_criteria(question: &Value) -> Option<Value> {
    let text = question.as_str()?.to_ascii_lowercase();
    if NON_IMPLEMENTER_TOPICS
        .iter()
        .any(|topic| text.contains(topic))
    {
        return None;
    }
    Some(json!({
        "true": "The path and snippets show code that itself does or defines what the question asks.",
        "false": "The file only calls, imports, tests, configures, documents, or mentions it."
    }))
}

pub(super) fn build_jobs(
    structured: &mut Value,
    tool: &str,
    specs: &[Option<SemanticRerankSpec>],
) -> Vec<SemanticRerankJob> {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let mut jobs = Vec::new();
    for (row_index, (row, spec)) in rows.iter_mut().zip(specs).enumerate() {
        let Some(spec) = spec else { continue };
        // A failed search has nothing to reorder; a "success" sidecar there
        // would read as a completed rerank.
        if row.get("status").and_then(Value::as_str) == Some("error") {
            continue;
        }
        let data = &mut row["data"];
        attach_continuation(data, tool, spec);
        let files = data
            .get("files")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if files.is_empty() {
            let continuation = take_continuation(data);
            data["semanticRerank"] = json!({
                "status":"success","totalCandidates":0,"evaluatedCandidates":0,
                "returnedCandidates":0,"filteredCandidates":0,"candidates":[]
            });
            if let Some(continuation) = continuation {
                data["semanticRerank"]["next"] = continuation;
            }
            continue;
        }
        let resources = files
            .iter()
            .enumerate()
            .map(|(index, file)| {
                json!({
                    "id":format!("candidate-{}", index + 1),
                    "context":{"value":candidate_state(file)}
                })
            })
            .collect::<Vec<_>>();
        let questions = spec
            .questions
            .iter()
            .map(|question| {
                let mut noul = json!({"type":"noul","instructions":question["question"]});
                if let Some(criteria) = implementer_criteria(&question["question"]) {
                    noul["criteria"] = criteria;
                }
                json!({"id":question["id"],"question":noul})
            })
            .collect::<Vec<_>>();
        jobs.push(SemanticRerankJob {
            row_index,
            matrix: json!({
                "id":format!("search-row-{row_index}"),
                "reasoning":"Semantically rerank sanitized search candidates.",
                "resources":resources,
                "questions":questions
            }),
        });
    }
    jobs
}

#[derive(Clone)]
struct QuestionResult {
    question_id: String,
    score: Option<f64>,
    error_code: Option<String>,
}

struct CandidateResult {
    item: Value,
    source_rank: usize,
    score: Option<f64>,
    questions: Vec<QuestionResult>,
}

/// Read the first page of each resource from resource-major clasify output
/// (`resources[].pages[].answers[questionId]`); a page-level error applies to
/// every question.
fn cell_results(data: &Value) -> HashMap<(String, String), QuestionResult> {
    let mut results = HashMap::new();
    for resource in data
        .get("resources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(resource_id) = resource.get("resourceId").and_then(Value::as_str) else {
            continue;
        };
        let Some(page) = resource
            .get("pages")
            .and_then(Value::as_array)
            .and_then(|pages| pages.first())
        else {
            continue;
        };
        let page_error = page.pointer("/error/code").and_then(Value::as_str);
        for (question_id, answer) in page
            .get("answers")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            results.insert(
                (resource_id.into(), question_id.clone()),
                QuestionResult {
                    question_id: question_id.clone(),
                    score: answer.get("noul").and_then(Value::as_f64),
                    error_code: answer
                        .pointer("/error/code")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                },
            );
        }
        if let Some(code) = page_error {
            results.insert(
                (resource_id.into(), String::new()),
                QuestionResult {
                    question_id: String::new(),
                    score: None,
                    error_code: Some(code.into()),
                },
            );
        }
    }
    results
}

/// One compact row per returned file, aligned with `data.files`.
fn candidate_entry(candidate: &CandidateResult) -> Value {
    let mut entry = json!({"sourceRank":candidate.source_rank});
    // Agents read scores without re-aligning to `files[i]`; the path is the
    // cheapest unambiguous key (repo identity stays on the file row).
    if let Some(path) = candidate.item.get("path").filter(|path| path.is_string()) {
        entry["path"] = path.clone();
    }
    if let Some(score) = candidate.score {
        entry["score"] = json!(score);
    }
    if candidate.questions.len() > 1 {
        let scores = candidate
            .questions
            .iter()
            .filter_map(|question| Some((question.question_id.clone(), json!(question.score?))))
            .collect::<Map<_, _>>();
        if !scores.is_empty() {
            entry["scores"] = Value::Object(scores);
        }
    }
    let errors = candidate
        .questions
        .iter()
        .filter(|question| question.score.is_none())
        .map(|question| {
            let code = question
                .error_code
                .clone()
                .unwrap_or_else(|| "classificationMissingAnswer".into());
            (question.question_id.clone(), json!(code))
        })
        .collect::<Map<_, _>>();
    if !errors.is_empty() {
        entry["errors"] = Value::Object(errors);
    }
    entry
}

fn apply_row(row: &mut Value, spec: &SemanticRerankSpec, assessment: &DomainResult) {
    let data = &mut row["data"];
    let continuation = take_continuation(data);
    let Some(files) = data.get("files").and_then(Value::as_array).cloned() else {
        return;
    };
    let cells = cell_results(&assessment.data);
    let mut candidates = files
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let resource_id = format!("candidate-{}", index + 1);
            let questions = spec
                .questions
                .iter()
                .map(|question| {
                    let question_id = question["id"].as_str().unwrap_or_default().to_owned();
                    let page_error = cells
                        .get(&(resource_id.clone(), String::new()))
                        .and_then(|page| page.error_code.clone());
                    cells
                        .get(&(resource_id.clone(), question_id.clone()))
                        .cloned()
                        .unwrap_or(QuestionResult {
                            question_id,
                            score: None,
                            error_code: page_error,
                        })
                })
                .collect::<Vec<_>>();
            let scores = questions
                .iter()
                .filter_map(|question| question.score)
                .collect::<Vec<_>>();
            // Rounded like clasify answers: 0.27999999999999997 carries no signal.
            let score = (!scores.is_empty()).then(|| {
                (scores.iter().sum::<f64>() / scores.len() as f64 * 1000.0).round() / 1000.0
            });
            CandidateResult {
                item,
                source_rank: index + 1,
                score,
                questions,
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| match (left.score, right.score) {
        (Some(left_score), Some(right_score)) => right_score
            .partial_cmp(&left_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.source_rank.cmp(&right.source_rank)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.source_rank.cmp(&right.source_rank),
    });
    let total = candidates.len();
    let evaluated = candidates
        .iter()
        .filter(|candidate| candidate.score.is_some())
        .count();
    let answered = candidates
        .iter()
        .flat_map(|candidate| &candidate.questions)
        .filter(|question| question.score.is_some())
        .count();
    let status = if answered == 0 {
        "error"
    } else if answered == total * spec.questions.len() {
        "success"
    } else {
        "partial"
    };
    data["semanticRerank"] = json!({
        "status":status,"totalCandidates":total,"evaluatedCandidates":evaluated,
        "candidates":candidates.iter().map(candidate_entry).collect::<Vec<_>>()
    });
    // Every score below 0.4 means no snippet shows the asked behavior: the
    // order is noise (evals: all ≤0.27, target shuffled within 0.1).
    if candidates
        .iter()
        .filter_map(|candidate| candidate.score)
        .fold(None, |best: Option<f64>, score| {
            Some(best.map_or(score, |b| b.max(score)))
        })
        .is_some_and(|best| best < LOW_SIGNAL_SCORE)
    {
        data["semanticRerank"]["lowSignal"] = json!(true);
    }
    // Reordering only: every returned file stays.
    data["files"] = Value::Array(
        candidates
            .into_iter()
            .map(|candidate| candidate.item)
            .collect(),
    );
    for field in ["model", "usage"] {
        if let Some(value) = assessment.data.get(field) {
            data["semanticRerank"][field] = value.clone();
        }
    }
    if let Some(continuation) = continuation {
        data["semanticRerank"]["next"] = continuation;
    }
}

pub(super) fn apply_assessments(
    structured: &mut Value,
    specs: &[Option<SemanticRerankSpec>],
    jobs: &[SemanticRerankJob],
    assessments: &[DomainResult],
) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for (job, assessment) in jobs.iter().zip(assessments) {
        let Some(spec) = specs.get(job.row_index).and_then(Option::as_ref) else {
            continue;
        };
        if let Some(row) = rows.get_mut(job.row_index) {
            apply_row(row, spec, assessment);
        }
    }
}

/// A failed assessment leaves files in source order; the sidecar reports the
/// failure once instead of repeating it per candidate.
pub(super) fn apply_failure(structured: &mut Value, jobs: &[SemanticRerankJob]) {
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for job in jobs {
        let Some(row) = rows.get_mut(job.row_index) else {
            continue;
        };
        let total = row
            .pointer("/data/files")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let continuation = take_continuation(&mut row["data"]);
        row["data"]["semanticRerank"] = json!({
            "status":"error","totalCandidates":total,"evaluatedCandidates":0,
            "error":{"code":"classificationProviderError","message":"Semantic reranking failed; source order was preserved."}
        });
        if let Some(continuation) = continuation {
            row["data"]["semanticRerank"]["next"] = continuation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::dispatch;

    #[test]
    fn extracts_multiple_questions_and_applies_the_cell_bound() {
        let input = json!({"operation":"code","reasoning":"rank","semanticRerank":{"questions":[
            {"id":"a","question":"Relevant?"},{"id":"b","question":"Runtime?"},
            {"id":"c","question":"Direct?"},{"id":"d","question":"Useful?"}
        ]}});
        let (clean, specs) = extract("ghSearch", input).unwrap();
        assert!(clean.get("semanticRerank").is_none());
        assert_eq!(clean["pageSize"], 6);
        assert_eq!(specs[0].as_ref().unwrap().questions.len(), 4);
    }

    #[test]
    fn rejects_unsupported_modes_and_incomplete_rerank_objects() {
        assert!(extract("ghSearch", json!({"operation":"repositories","semanticRerank":{"questions":[{"id":"q","question":"Relevant?"}]}})).is_err());
        assert!(extract("localSearch", json!({"semanticRerank":{"minScore":0.5}})).is_err());
        assert!(
            extract(
                "localSearch",
                json!({"resultView":"files","semanticRerank":{"questions":[{"id":"q","question":"Relevant?"}]}})
            )
            .is_err()
        );
    }

    #[test]
    fn restores_only_continuations_that_support_semantic_reranking() {
        let spec = SemanticRerankSpec {
            questions: vec![json!({"id":"q","question":"Relevant?"})],
        };
        let mut unsupported =
            json!({"next":{"viewStructure":{"tool":"ghSearch","query":{"operation":"tree"}}}});
        attach_continuation(&mut unsupported, "ghSearch", &spec);
        assert!(unsupported.get("semanticRerankContinuation").is_none());
        assert!(
            unsupported
                .pointer("/next/viewStructure/query/semanticRerank")
                .is_none()
        );

        let mut supported = json!({"pagination":{"currentPage":1},"next":{
            "nextPage":{"tool":"localSearch","query":{"resultView":"paginated","page":2}},
            "nextMatchPage":{"tool":"localSearch","query":{"resultView":"paginated","page":1,"matchPage":2}}
        }});
        attach_continuation(&mut supported, "localSearch", &spec);
        assert!(
            supported
                .pointer("/semanticRerankContinuation/nextMatchPage")
                .is_none(),
            "same-page continuations must not re-rank the same files"
        );
        assert_eq!(
            supported.pointer(
                "/semanticRerankContinuation/nextPage/query/semanticRerank/questions/0/id"
            ),
            Some(&json!("q"))
        );
        assert!(
            supported.pointer("/next/nextPage").is_none(),
            "the reranked call moves out of data.next instead of duplicating it"
        );
        assert!(supported.pointer("/next/nextMatchPage").is_some());

        let mut only = json!({"next":{"nextPage":{"tool":"localSearch","query":{"resultView":"paginated","page":2}}}});
        attach_continuation(&mut only, "localSearch", &spec);
        assert!(
            only.get("next").is_none(),
            "an emptied data.next is removed"
        );
    }

    #[test]
    fn candidate_state_keeps_identity_and_first_snippets_only() {
        let matches = (1..=MAX_CANDIDATE_SNIPPETS + 2)
            .map(|line| json!({"line":line,"column":3,"value":format!("hit {line}")}))
            .collect::<Vec<_>>();
        let state = candidate_state(&json!({"owner":"o","repo":"r","path":"a.rs",
            "totalMatchRows":52,"pagination":{"hasMore":true},"matches":matches}));
        assert_eq!(state["path"], "a.rs");
        assert!(state.get("pagination").is_none() && state.get("totalMatchRows").is_none());
        assert_eq!(
            state["matches"].as_array().unwrap().len(),
            MAX_CANDIDATE_SNIPPETS
        );
        assert_eq!(state["matches"][0], json!({"line":1,"value":"hit 1"}));
        assert_eq!(candidate_state(&json!("raw")), json!("raw"));
    }

    #[test]
    fn ranks_by_mean_and_keeps_every_file_including_failed_ones() {
        let spec = SemanticRerankSpec {
            questions: vec![
                json!({"id":"a","question":"Relevant?"}),
                json!({"id":"b","question":"Runtime?"}),
            ],
        };
        let mut row = json!({"data":{"files":[
            {"owner":"o","repo":"r","path":"first","matches":["BODY"]},
            {"path":"second"},{"path":"unknown"}
        ]}});
        let assessment = dispatch::value_result(json!({"resources":[
            {"resourceId":"candidate-1","pages":[{"answers":{"a":{"noul":0.25},"b":{"noul":0.75}}}]},
            {"resourceId":"candidate-2","pages":[{"answers":{"a":{"noul":1.0},"b":{"noul":0.5}}}]},
            {"resourceId":"candidate-3","pages":[{"answers":{
                "a":{"error":{"code":"provider","message":"down"}},
                "b":{"error":{"code":"provider","message":"down"}}}}]}
        ]}));
        apply_row(&mut row, &spec, &assessment);
        assert_eq!(
            row["data"]["files"],
            json!([{"path":"second"},
                {"owner":"o","repo":"r","path":"first","matches":["BODY"]},
                {"path":"unknown"}]),
            "reordered by mean score; no file removed"
        );
        let sidecar = &row["data"]["semanticRerank"];
        assert_eq!(sidecar["status"], "partial");
        assert_eq!(sidecar["totalCandidates"], 3);
        assert_eq!(sidecar["evaluatedCandidates"], 2);
        assert_eq!(
            sidecar["candidates"],
            json!([
                {"sourceRank":2,"path":"second","score":0.75,"scores":{"a":1.0,"b":0.5}},
                {"sourceRank":1,"path":"first","score":0.5,"scores":{"a":0.25,"b":0.75}},
                {"sourceRank":3,"path":"unknown","errors":{"a":"provider","b":"provider"}}
            ])
        );
        assert!(sidecar.get("excluded").is_none() && sidecar.get("minScore").is_none());
        assert!(!sidecar.to_string().contains("BODY"));
    }

    #[test]
    fn reordering_never_removes_a_low_scoring_file() {
        let spec = SemanticRerankSpec {
            questions: vec![json!({"id":"q","question":"Relevant?"})],
        };
        let mut row = json!({"data":{"files":[{"path":"low"},{"path":"best"}]}});
        let assessment = dispatch::value_result(json!({"resources":[
            {"resourceId":"candidate-1","pages":[{"answers":{"q":{"noul":0.02}}}]},
            {"resourceId":"candidate-2","pages":[{"answers":{"q":{"noul":0.64}}}]}
        ]}));
        apply_row(&mut row, &spec, &assessment);
        assert_eq!(
            row["data"]["files"],
            json!([{"path":"best"},{"path":"low"}])
        );
        assert_eq!(
            row["data"]["semanticRerank"]["candidates"],
            json!([{"sourceRank":2,"path":"best","score":0.64},{"sourceRank":1,"path":"low","score":0.02}])
        );
        assert_eq!(row["data"]["semanticRerank"]["status"], "success");
    }

    #[test]
    fn failed_search_rows_get_no_rerank_sidecar() {
        let spec = SemanticRerankSpec {
            questions: vec![json!({"id":"q","question":"Relevant?"})],
        };
        let mut structured =
            json!({"results":[{"status":"error","data":{"error":"rate limited"}}]});
        assert!(build_jobs(&mut structured, "ghSearch", &[Some(spec)]).is_empty());
        assert!(
            structured
                .pointer("/results/0/data/semanticRerank")
                .is_none()
        );
    }

    #[test]
    fn implementer_criteria_apply_only_to_behavior_questions() {
        assert!(implementer_criteria(&json!("Does this file implement retry backoff?")).is_some());
        for question in [
            "Does this file test retry backoff?",
            "Is this the README for the retry module?",
            "Does this file call retry()?",
        ] {
            assert!(
                implementer_criteria(&json!(question)).is_none(),
                "{question}"
            );
        }
        assert!(implementer_criteria(&json!({"rubric":"x"})).is_none());
    }

    #[test]
    fn uniformly_low_scores_flag_the_ranking_as_low_signal() {
        let spec = SemanticRerankSpec {
            questions: vec![json!({"id":"q","question":"Relevant?"})],
        };
        let mut row = json!({"data":{"files":[{"path":"a"},{"path":"b"}]}});
        let assessment = dispatch::value_result(json!({"resources":[
            {"resourceId":"candidate-1","pages":[{"answers":{"q":{"noul":0.2}}}]},
            {"resourceId":"candidate-2","pages":[{"answers":{"q":{"noul":0.27}}}]}
        ]}));
        apply_row(&mut row, &spec, &assessment);
        assert_eq!(row["data"]["semanticRerank"]["lowSignal"], true);
        let mut strong = json!({"data":{"files":[{"path":"a"}]}});
        let assessment = dispatch::value_result(json!({"resources":[
            {"resourceId":"candidate-1","pages":[{"answers":{"q":{"noul":0.9}}}]}
        ]}));
        apply_row(&mut strong, &spec, &assessment);
        assert!(strong["data"]["semanticRerank"].get("lowSignal").is_none());
    }

    #[test]
    fn total_provider_failure_preserves_files_and_reports_once() {
        let mut structured =
            json!({"results":[{"data":{"files":[{"path":"first"},{"path":"second"}]}}]});
        apply_failure(
            &mut structured,
            &[SemanticRerankJob {
                row_index: 0,
                matrix: Value::Null,
            }],
        );
        assert_eq!(
            structured.pointer("/results/0/data/files"),
            Some(&json!([{"path":"first"},{"path":"second"}]))
        );
        let sidecar = &structured["results"][0]["data"]["semanticRerank"];
        assert_eq!(sidecar["status"], "error");
        assert_eq!(sidecar["totalCandidates"], 2);
        assert!(sidecar.get("candidates").is_none());
        assert_eq!(sidecar["error"]["code"], "classificationProviderError");
    }
}
