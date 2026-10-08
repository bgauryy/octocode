//! clasify: judge resources (supplied values or delegated reads) against
//! typed questions. The runtime works on the public input shape everywhere;
//! [`transport`] alone maps it onto the provider's wire form.
pub(crate) mod admission;
pub(crate) mod batch;
pub(crate) mod cache;
mod compact;
pub(crate) mod context;
pub(crate) mod handoff;
pub mod items;
mod locate;
mod output;
pub mod resource;
pub(crate) mod run;
pub(crate) mod stats;
pub(crate) mod transport;

use self::transport::{ClassificationError, check_budget, check_key, endpoint, post};
pub(crate) use crate::contracts::tool_types::ClasifyQuery;
use crate::providers::classification::gate::GateLease;
use crate::{
    providers::RequestBudget,
    tools::id::{ToolId, clasify_policy},
};
use secrecy::SecretString;
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;

/// clasify's output facts: rows are resource-major matrices with their own
/// projection.
pub(crate) struct Output;
impl crate::tools::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Inspect resources, typed questions, and OCTOCODE_CLASSIFICATION_API."
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
    fn resource_major(&self) -> bool {
        true
    }
}

/// Clasify's resolved provider settings. Clasify is its own product: it
/// runs through [`ClasifySettings::call`] and never enters the ordinary
/// results loop.
pub(crate) struct ClasifySettings {
    pub(crate) key: Option<SecretString>,
    pub(crate) base_url: String,
    pub(crate) endpoint_path: String,
    pub(crate) model: String,
    pub(crate) provider: &'static dyn crate::providers::classification::ClassificationProvider,
    pub(crate) timeout: std::time::Duration,
    pub(crate) retries: u32,
    pub(crate) max_concurrency: usize,
}

impl ClasifySettings {
    /// Run validated rows: each parses once into the generated
    /// [`ClasifyQuery`], the tool entry's input.
    pub(crate) fn call(
        &self,
        rows: &[Value],
        rejected_rows: Vec<(usize, Value)>,
        dispatcher: &crate::runtime::domain_dispatch::DomainDispatcher,
        context: &crate::runtime::ExecutionContext,
        record_usage: impl FnOnce(stats::ClassificationUsage),
    ) -> Result<run::Receipts, crate::runtime::ExecutionError> {
        let Some(key) = self.key.as_ref() else {
            return Err(crate::runtime::ExecutionError::WorkerFailed);
        };
        // Contract validation passed, so a row that does not parse means core
        // and native disagree on the contract.
        let queries = rows
            .iter()
            .map(|row| serde_json::from_value::<ClasifyQuery>(row.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| crate::runtime::ExecutionError::WorkerFailed)?;
        run::execute(
            &queries,
            rejected_rows,
            dispatcher,
            context,
            self.timeout,
            run::ProviderConfig {
                key,
                base_url: &self.base_url,
                endpoint_path: &self.endpoint_path,
                model: &self.model,
                provider: self.provider,
                retries: self.retries,
                max_concurrency: self.max_concurrency,
            },
            record_usage,
        )
    }

    /// One minimal yes/no judgment through the same key, endpoint, gate, and
    /// response validation a real call uses, bounded by [`PROBE_TIMEOUT`]
    /// with one retry. `Ok` means the provider answered.
    pub(crate) async fn probe(&self) -> Result<(), ClassificationError> {
        let Some(key) = self.key.clone() else {
            return Err(ClassificationError::new(
                "missingConfiguration",
                "No classification provider key is configured.",
                "Set OCTOCODE_CLASSIFICATION_API.",
            ));
        };
        let gate = crate::providers::classification::gate::lease(
            &run::account_gate_key(&self.base_url, &self.endpoint_path, &key),
            self.max_concurrency,
        );
        let budget = transport::budget(
            std::time::Instant::now() + self.timeout.min(PROBE_TIMEOUT),
            tokio_util::sync::CancellationToken::new(),
        );
        let question = transport::provider_question(
            &json!({"type":"yesno","ask":"Is this state a connectivity check?"}),
        )?;
        judge(
            &json!("octocode connectivity check"),
            &question,
            key,
            &self.base_url,
            &self.endpoint_path,
            &self.model,
            self.provider,
            budget,
            self.retries.min(1),
            &gate,
        )
        .await
        .map(drop)
    }
}

/// Upper bound on [`ClasifySettings::probe`], so a startup check never holds
/// a host's handshake for the full request timeout.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Semantic assessment is nondeterministic and billed per evaluation. Query
/// replay cannot serve a page of the original judgment.
pub(crate) fn reject_response_pagination(
    input: &Value,
) -> Result<(), crate::runtime::RuntimeError> {
    if ["responseOffset", "responseLength", "responseSnapshot"]
        .iter()
        .any(|field| input.get(field).is_some())
    {
        return Err(crate::runtime::RuntimeError::new(
            "unsupportedResponsePagination",
            "clasify response pagination is unsupported: replay would repeat context execution and inference. Use the page-level results and next.clasify continuation instead.",
        ));
    }
    Ok(())
}

fn request_error(message: &str) -> ClassificationError {
    ClassificationError {
        code: "invalidClassificationRequest".into(),
        message: message.into(),
        hints: vec!["Inspect the current clasify query schema.".into()],
        ..Default::default()
    }
}

fn nullable_entry(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::String(_) | Value::Array(_) | Value::Object(_)
    )
}

fn entry(value: &Value) -> bool {
    !value.is_null() && nullable_entry(value)
}

/// Read tools a clasify resource may delegate to (contract `scoutTools`).
pub(crate) fn is_context_tool(tool: ToolId) -> bool {
    clasify_policy::SCOUT_TOOLS.contains(&tool)
}

/// Search tools that accept `candidateEvidence` (contract `candidateSearchTools`).
pub(crate) fn is_candidate_search_tool(tool: ToolId) -> bool {
    clasify_policy::CANDIDATE_SEARCH_TOOLS.contains(&tool)
}

/// File reads that accept `prefilter` (contract `fileReadTools`).
pub(crate) fn is_file_read_tool(tool: ToolId) -> bool {
    clasify_policy::FILE_READ_TOOLS.contains(&tool)
}

/// Contract `candidateEvidence` enum (`search` | `fileChunks`).
pub(crate) use crate::contracts::tool_types::ClasifyQueryResourcesItemVariant1CandidateEvidence as CandidateEvidence;
use crate::contracts::tool_types::{
    ClasifyQueryQuestionsItem as Question, ClasifyQueryResourcesItem as Resource,
    ClasifyQueryResourcesItemVariant1Query as ResourceQuery,
    ClasifyQueryResourcesItemVariant1Tool as ResourceTool,
};

/// A resource's parsed `candidateEvidence`, when present and known.
pub(crate) fn candidate_evidence(resource: &Value) -> Option<CandidateEvidence> {
    resource
        .get("candidateEvidence")
        .and_then(Value::as_str)
        .and_then(|value| value.parse().ok())
}

/// Whether a public question is a `locate` question.
pub(crate) fn is_locate(question: &Value) -> bool {
    question.get("type").and_then(Value::as_str) == Some("locate")
}

/// The read tool a resource names.
fn read_tool(tool: ResourceTool) -> ToolId {
    match tool {
        ResourceTool::GhSearchRepo => ToolId::GhSearchRepo,
        ResourceTool::GhSearchCode => ToolId::GhSearchCode,
        ResourceTool::GhStructure => ToolId::GhStructure,
        ResourceTool::GhGetFileContent => ToolId::GhGetFileContent,
        ResourceTool::GhSearchHistory => ToolId::GhSearchHistory,
        ResourceTool::GhGetHistoryItem => ToolId::GhGetHistoryItem,
        ResourceTool::ArtifactSearch => ToolId::ArtifactSearch,
        ResourceTool::LocalSearch => ToolId::LocalSearch,
        ResourceTool::LocalFetch => ToolId::LocalFetch,
        ResourceTool::StructureSearch => ToolId::StructureSearch,
        ResourceTool::AstSearch => ToolId::AstSearch,
        ResourceTool::AstTopology => ToolId::AstTopology,
        ResourceTool::LspSearch => ToolId::LspSearch,
    }
}

/// Apply `$body` to the `id` every question shape carries.
macro_rules! question_id_field {
    ($question:expr, |$id:ident| $body:expr) => {
        match $question {
            Question::Variant0 { id: $id, .. } => $body,
            Question::Variant1 { id: $id, .. } => $body,
            Question::Variant2 { id: $id, .. } => $body,
            Question::Variant3 { id: $id, .. } => $body,
            Question::Variant4 { id: $id, .. } => $body,
        }
    };
}

/// The target a `locate` question asks for; `None` for every other type.
fn locate_ask(question: &Question) -> Option<&str> {
    match question {
        Question::Variant0 {
            type_: crate::contracts::tool_types::ClasifyQueryQuestionsItemVariant0Type::Locate,
            ask,
            ..
        } => Some(ask.as_str()),
        _ => None,
    }
}

fn asks_locate(question: &Question) -> bool {
    locate_ask(question).is_some()
}

fn question_id(question: &Question) -> Option<&str> {
    question_id_field!(question, |id| id.as_deref().map(String::as_str))
}

fn resource_id(resource: &Resource) -> Option<&str> {
    match resource {
        Resource::Variant0 { id, .. } => id.as_deref().map(String::as_str),
        Resource::Variant1 { id, .. } => id.as_deref().map(String::as_str),
    }
}

/// File-read query fields that already select what to read; without one, a
/// file resource reads the whole file (`fullContent`).
const READ_SELECTORS: [&str; 7] = [
    "fullContent",
    "ranges",
    "block",
    "matchString",
    "offset",
    "length",
    "minify",
];

/// One validated call's matrices as the runtime runs them: a lead's
/// `{queries:[row]}` resource query becomes its row, implied fields are
/// explicit (`fullContent` on a file read with no selector, `fileChunks` on a
/// search resource), and every matrix, resource, and question has
/// an `id`.
/// [`normalize`] over validated rows in the public shape (test fixtures).
#[cfg(test)]
pub(crate) fn normalize_rows(rows: &mut [Value]) {
    let mut queries = rows
        .iter()
        .map(|row| serde_json::from_value::<ClasifyQuery>(row.clone()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("a fixture matrix parses: {error}"));
    normalize(&mut queries);
    for (row, query) in rows.iter_mut().zip(&queries) {
        *row = serde_json::to_value(query).unwrap_or_else(|error| panic!("{error}"));
    }
}

pub(crate) fn normalize(queries: &mut [ClasifyQuery]) {
    for query in queries.iter_mut() {
        for resource in &mut query.resources {
            normalize_resource(resource);
        }
    }
    normalize_ids(queries);
}

fn normalize_resource(resource: &mut Resource) {
    let Resource::Variant1 {
        candidate_evidence,
        query,
        tool,
        ..
    } = resource
    else {
        return;
    };
    unwrap_lead_query(query);
    let tool = read_tool(*tool);
    if is_file_read_tool(tool)
        && !READ_SELECTORS
            .iter()
            .chain(&["queries"])
            .any(|key| query.extra.contains_key(*key))
    {
        query.extra.insert("fullContent".into(), Value::Bool(true));
    }
    // Search candidates are judged on their files by default: snippets scored
    // a 156-file scout's answer 0.69 in 18 calls, file chunks 0.97 in 4
    // (2026-10-08). `candidateEvidence:"search"` keeps snippets.
    if candidate_evidence.is_none() && is_candidate_search_tool(tool) {
        *candidate_evidence = Some(CandidateEvidence::FileChunks);
    }
}

/// A lead query (`{queries:[row]}`) pasted as a resource query reads its one
/// row. Several rows stay as sent: the read rejects them, since a resource
/// is one read.
fn unwrap_lead_query(query: &mut ResourceQuery) {
    let only_rows = query.owner.is_none()
        && query.path.is_none()
        && query.reasoning.is_none()
        && query.ref_.is_none()
        && query.repo.is_none()
        && query.extra.len() == 1;
    let row = match query.extra.get("queries").and_then(Value::as_array) {
        Some(rows) if only_rows && rows.len() == 1 && rows[0].is_object() => rows[0].clone(),
        _ => return,
    };
    if let Ok(row) = serde_json::from_value::<ResourceQuery>(row) {
        *query = row;
    }
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

/// Ids for the rows that lack one, unique within `rows`: `read` gets a row's
/// id and `write` sets a derived one.
fn fill_ids<T>(
    rows: &mut [T],
    prefix: &str,
    read: impl Fn(&T) -> Option<&str>,
    write: impl Fn(&mut T, &str),
) {
    let mut used = rows
        .iter()
        .filter_map(|row| read(row).map(str::to_owned))
        .collect::<HashSet<_>>();
    for (position, row) in rows.iter_mut().enumerate() {
        if read(row).is_none() {
            write(row, &next_unused_id(prefix, position, &mut used));
        }
    }
}

/// Correlation IDs are presentation metadata, not provider input. Derive them
/// after contract validation so callers can omit repetitive bookkeeping while
/// preserving stable keyed output and executable continuations. Derived ids
/// (`matrix-2`, `resource-1-2`) always satisfy the id pattern.
fn normalize_ids(queries: &mut [ClasifyQuery]) {
    fill_ids(
        queries,
        "matrix",
        |query| query.id.as_deref().map(String::as_str),
        |query, id| query.id = id.parse().ok(),
    );
    for query in queries.iter_mut() {
        fill_ids(
            &mut query.resources,
            "resource",
            resource_id,
            |resource, value| match resource {
                Resource::Variant0 { id, .. } => *id = value.parse().ok(),
                Resource::Variant1 { id, .. } => *id = value.parse().ok(),
            },
        );
        fill_ids(
            &mut query.questions,
            "question",
            question_id,
            |question, value| question_id_field!(question, |id| *id = value.parse().ok()),
        );
    }
}

/// Context tools whose continuations move within one document. Search and
/// discovery continuations reach new candidates instead, so clasify captures
/// only the requested page and returns the rest through `next.clasify`.
pub(crate) fn pages_within_resource(tool: ToolId) -> bool {
    matches!(
        tool,
        ToolId::LocalFetch | ToolId::GhGetFileContent | ToolId::GhGetHistoryItem
    )
}

/// Whether a tool resource can yield the contiguous original-source page that
/// `locate` tags: an untransformed file read, or search hydrated with
/// `fileChunks` (a page that is still gapped fails per page at runtime).
/// Supplied values are left to the runtime page check.
fn locate_capable(resource: &Resource) -> bool {
    let Resource::Variant1 {
        candidate_evidence,
        query,
        tool,
        ..
    } = resource
    else {
        return true;
    };
    let tool = read_tool(*tool);
    if is_file_read_tool(tool) {
        return query
            .extra
            .get("minify")
            .and_then(Value::as_str)
            .is_none_or(|minify| minify == "none");
    }
    is_candidate_search_tool(tool) && *candidate_evidence == Some(CandidateEvidence::FileChunks)
}

/// Resolve a matrix's questions once, before capture: each becomes
/// `{id, question}` where `question` is the provider question, or the public
/// `locate` question (its provider questions are built per page). Rejects
/// what the contract schema cannot express: `locate` over resources without
/// contiguous source lines.
///
/// Precondition: `query` passed `contracts::prepare_many_and_validate` and
/// [`admission::check`] (the engine is clasify's only entry and admits every
/// row; clasify never mints cursors) and then [`normalize`]. Shape, brief,
/// id, context, prefilter-tool, and cell-limit rules are therefore not
/// re-checked here.
pub(crate) fn preflight(query: &ClasifyQuery) -> Result<Vec<Value>, ClassificationError> {
    let mut resolved = Vec::with_capacity(query.questions.len());
    for question in &query.questions {
        let provider = if let Some(ask) = locate_ask(question) {
            json!({"type":"locate","ask":ask})
        } else {
            let public = serde_json::to_value(question)
                .map_err(|_| request_error("Questions must be typed question objects."))?;
            transport::provider_question(&public)?
        };
        resolved.push(json!({"id":question_id(question),"question":provider}));
    }
    if query.questions.iter().any(asks_locate) {
        let blocked = query
            .resources
            .iter()
            .filter(|resource| !locate_capable(resource))
            .filter_map(resource_id)
            .collect::<Vec<_>>();
        if !blocked.is_empty() {
            return Err(locate_unsupported(&format!(
                "resources {} cannot supply them",
                blocked.join(", ")
            )));
        }
    }
    Ok(resolved)
}

/// `classificationLocateUnsupported`, raised before capture (a resource that
/// cannot supply source lines) or per page (a captured page that does not):
/// one message stem and one repair, whichever stage found it.
pub(crate) fn locate_unsupported(reason: &str) -> ClassificationError {
    ClassificationError {
        code: "classificationLocateUnsupported".into(),
        message: format!("locate needs contiguous original source lines; {reason}."),
        hints: vec![
            "Locate needs localFetch/ghGetFileContent without minify, or localSearch/ghSearchCode candidateEvidence:\"fileChunks\".".into(),
            "To screen search, structure, AST, LSP, history or package results, ask yesno/choice/score/relevant in a separate matrix.".into(),
        ],
        ..Default::default()
    }
}

/// Build the provider request for one state × provider question cell and
/// serialize it once: the size check and the POST share those bytes.
/// Delegates wire format to the vendor's [`ClassificationProvider::build_request`].
fn prepare(
    state: &Value,
    question: &Value,
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
) -> Result<(Value, bytes::Bytes), ClassificationError> {
    if !entry(state) {
        return Err(request_error(
            "Context value must be a non-empty string, object, or array.",
        ));
    }
    let request = provider.build_request(state, question, model);
    let body = serde_json::to_vec(&request)
        .map_err(|_| request_error("Classification request could not be serialized."))?;
    if body.len() > MAX_REQUEST_BYTES {
        return Err(request_error(
            "Classification request exceeded the 4 MiB limit.",
        ));
    }
    Ok((request, body.into()))
}

/// Judge one state × provider question with a single request.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn judge(
    state: &Value,
    question: &Value,
    key: SecretString,
    base_url: &str,
    endpoint_path: &str,
    model: &str,
    provider: &dyn crate::providers::classification::ClassificationProvider,
    budget: RequestBudget,
    retries: u32,
    gate: &GateLease,
) -> Result<Value, ClassificationError> {
    check_budget(&budget)?;
    let (request, body) = prepare(state, question, model, provider)?;
    check_key(&key)?;
    let (response, provider_calls) = post(
        body,
        &key,
        endpoint(base_url, endpoint_path)?,
        &budget,
        retries,
        gate,
    )
    .await?;
    provider
        .validate_response(&request, &response)
        .map_err(|error| ClassificationError {
            code: error.code().into(),
            provider_calls,
            ..ClassificationError::invalid_response(error.message)
        })?;
    let answer = provider
        .extract_answer(&response)
        .ok_or_else(|| ClassificationError {
            provider_calls,
            ..ClassificationError::invalid_response(
                "Classification provider response is missing the expected answer.",
            )
        })?;
    let mut result = transport::project(
        question,
        answer,
        model,
        response["model"].as_str().unwrap_or(model),
        &response["usage"],
    )
    .map_err(|mut error| {
        error.provider_calls = provider_calls;
        error
    })?;
    result["usage"]["provider_calls"] = json!(provider_calls);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method},
    };

    /// The public yes/no question tests send.
    fn question() -> Value {
        json!({"type":"yesno","ask":"Assess only supplied context"})
    }
    /// The provider question `question()` runs as.
    fn provider_question() -> Value {
        json!({"type":"noul","instructions":"Assess only supplied context"})
    }
    /// One public matrix: `context` is a resource's `value` or
    /// `tool`+`query`(+`candidateEvidence`) fields.
    fn semantic_query(context: Value, question: Value) -> Value {
        let mut resource = context;
        resource["id"] = json!("resource-1");
        let mut question = question;
        question["id"] = json!("relevance.v1");
        json!({
            "id":"decision",
            "reasoning":"Decide whether to inspect the retry branch.",
            "mainGoal":"Searching for retry handling. Need files that decide a retry.",
            "resources":[resource],
            "questions":[question]
        })
    }
    /// Clasify's engine path: contract validation of the public shape,
    /// [`normalize`], then preflight.
    fn admitted(query: &Value) -> Result<Vec<Value>, ClassificationError> {
        let validated =
            crate::contracts::prepare_many_and_validate("clasify", json!({"queries":[query]}))
                .and_then(|validated| super::admission::check(&validated).map(|()| validated))
                .map_err(|error| request_error(&format!("{error:?}")))?;
        let mut typed = validated
            .iter()
            .map(|row| serde_json::from_value::<ClasifyQuery>(row.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| request_error(&error.to_string()))?;
        normalize(&mut typed);
        preflight(&typed[0])
    }

    #[test]
    fn preflight_accepts_a_continuation_that_carries_the_running_best() {
        let mut query = semantic_query(json!({"value":"x"}), question());
        query["carry"] = json!({"t":[{"resourceId":"resource-1","exists":0.9,"line":1,"endLine":8,"probability":0.5}]});
        assert!(admitted(&query).is_ok());
        query["carry"] = json!("not a map");
        assert!(admitted(&query).is_err());
        query.as_object_mut().unwrap().remove("carry");
        query["unexpected"] = json!(1);
        assert!(admitted(&query).is_err());
    }

    #[test]
    fn preflight_rejects_prefilter_outside_file_reads() {
        let search = json!({"tool":"localSearch","query":{"path":"/repo","matchString":"retry"}});
        let mut query = semantic_query(search, question());
        query["resources"][0]["prefilter"] = json!(["retry"]);
        // Contract validation owns the rule (same stage and wording as core).
        let error = admitted(&query).expect_err("search resources cannot prefilter");
        assert!(
            error
                .message
                .contains("prefilter applies only to localFetch or ghGetFileContent"),
            "{}",
            error.message
        );
        let read = json!({"tool":"localFetch","query":{"path":"/repo/a.rs"}});
        let mut query = semantic_query(read, question());
        query["resources"][0]["prefilter"] = json!(["retry"]);
        assert!(admitted(&query).is_ok());
    }

    fn budget() -> RequestBudget {
        super::transport::budget(
            Instant::now() + Duration::from_secs(30),
            tokio_util::sync::CancellationToken::new(),
        )
    }

    fn jev_provider() -> &'static dyn crate::providers::classification::ClassificationProvider {
        &crate::providers::classification::jev::JEV
    }

    fn test_gate() -> GateLease {
        crate::providers::classification::gate::lease("test://clasify-mod", 64)
    }

    /// The provider request for the admitted `{observation:true}` resource
    /// carries only the model, the state and the provider question.
    fn assert_observation_request(
        query: &Value,
        resolved: &[Value],
        provider: &dyn crate::providers::classification::ClassificationProvider,
    ) {
        assert_eq!(
            prepare(
                &query["resources"][0]["value"],
                &resolved[0]["question"],
                "m",
                provider,
            )
            .unwrap()
            .0,
            json!({"model":"m","state":{"observation":true},"questions":{"answer":provider_question()}})
        );
    }

    #[test]
    fn optional_briefs_stay_off_the_expanded_question() {
        let provider = jev_provider();
        let mut query = semantic_query(json!({"value":{"observation":true}}), question());
        let resolved = admitted(&query).expect("briefs accepted");
        assert!(resolved[0]["question"].get("mainGoal").is_none());
        assert!(resolved[0]["question"].get("reasoning").is_none());
        assert_observation_request(&query, &resolved, provider);
        query["carry"] = json!({"t":[{"resourceId":"resource-1","exists":0.9,"line":1,"endLine":8,"probability":0.5}]});
        assert!(admitted(&query).is_ok());
        for field in ["reasoning", "mainGoal"] {
            let mut missing = query.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(admitted(&missing).is_ok(), "{field} is optional");
            // A blank brief is dropped before validation.
            for blank in [json!(""), json!(" \t\n")] {
                let mut dropped = query.clone();
                dropped[field] = blank;
                assert!(admitted(&dropped).is_ok(), "blank {field} is dropped");
            }
            for invalid in [Value::Null, json!(7), json!("x".repeat(501))] {
                let mut bad = query.clone();
                bad[field] = invalid.clone();
                assert!(admitted(&bad).is_err(), "{field} {invalid}");
            }
        }
    }

    #[test]
    fn correlation_ids_are_validated_but_never_sent_to_the_provider() {
        let provider = jev_provider();
        let query = semantic_query(json!({"value":{"observation":true}}), question());
        let resolved = admitted(&query).expect("valid correlation IDs");
        assert_eq!(resolved[0]["id"], query["questions"][0]["id"]);
        assert_observation_request(&query, &resolved, provider);
        let mut null_id = query.clone();
        null_id["questions"][0]["id"] = Value::Null;
        let mut invalid_id = query.clone();
        invalid_id["resources"][0]["id"] = json!("bad id");
        for invalid in [null_id, invalid_id] {
            assert!(admitted(&invalid).is_err());
        }
        let mut missing = query;
        missing["questions"][0]
            .as_object_mut()
            .unwrap()
            .remove("id");
        let derived = admitted(&missing).expect("omitted ids are derived after validation");
        assert_eq!(derived[0]["id"], "question-1");
    }

    #[test]
    fn values_and_read_tool_resources_are_the_only_resources() {
        for value in [json!(["one"]), json!({"value":"one"}), json!("literal")] {
            assert!(admitted(&semantic_query(json!({"value":value}), question())).is_ok());
        }
        for tool in [
            "localFetch",
            "localSearch",
            "structureSearch",
            "astSearch",
            "astTopology",
            "lspSearch",
            "ghSearchRepo",
            "ghSearchCode",
            "ghStructure",
            "ghGetFileContent",
            "ghSearchHistory",
            "ghGetHistoryItem",
            "artifactSearch",
        ] {
            assert!(admitted(&semantic_query(json!({"tool":tool,"query":{}}), question())).is_ok());
        }
        for tool in ["jev", "clasify", "astRewrite", "ghCloneRepo", "unknown"] {
            assert!(
                admitted(&semantic_query(json!({"tool":tool,"query":{}}), question())).is_err()
            );
        }
        for context in [
            json!({"tool":"localSearch","query":{},"candidateEvidence":"search"}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"search"}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"fileChunks"}),
        ] {
            assert!(admitted(&semantic_query(context, question())).is_ok());
        }
        for context in [
            json!({"tool":"localFetch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"localFetch","query":{},"candidateEvidence":"search"}),
            json!({"tool":"ghStructure","query":{},"candidateEvidence":"search"}),
            json!({"tool":"ghSearchRepo","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"unknown"}),
        ] {
            assert!(admitted(&semantic_query(context, question())).is_err());
        }
        for invalid in [
            semantic_query(json!({"value":true}), question()),
            semantic_query(
                json!({"value":null,"tool":"localFetch","query":{}}),
                question(),
            ),
            semantic_query(json!({"value":null}), question()),
        ] {
            assert!(admitted(&invalid).is_err());
        }
    }

    #[test]
    fn locate_is_rejected_before_capture_for_resources_without_source_lines() {
        let locate = json!({"type":"locate","ask":"retry condition"});
        for context in [
            json!({"tool":"localFetch","query":{}}),
            json!({"tool":"localFetch","query":{"minify":"none"}}),
            json!({"tool":"ghGetFileContent","query":{}}),
            json!({"tool":"localSearch","query":{},"candidateEvidence":"fileChunks"}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"fileChunks"}),
            // A search resource under locate reads fileChunks by default.
            json!({"tool":"localSearch","query":{}}),
            json!({"value":"supplied"}),
        ] {
            assert!(admitted(&semantic_query(context, locate.clone())).is_ok());
        }
        for context in [
            json!({"tool":"localFetch","query":{"minify":"standard"}}),
            json!({"tool":"ghGetFileContent","query":{"minify":"symbols"}}),
            json!({"tool":"ghSearchCode","query":{},"candidateEvidence":"search"}),
            json!({"tool":"structureSearch","query":{}}),
            json!({"tool":"astSearch","query":{}}),
            json!({"tool":"astTopology","query":{}}),
            json!({"tool":"lspSearch","query":{}}),
            json!({"tool":"ghSearchRepo","query":{}}),
            json!({"tool":"ghStructure","query":{}}),
            json!({"tool":"ghSearchHistory","query":{}}),
            json!({"tool":"ghGetHistoryItem","query":{}}),
            json!({"tool":"artifactSearch","query":{}}),
        ] {
            let error = admitted(&semantic_query(context.clone(), locate.clone()))
                .expect_err("locate needs source lines");
            assert_eq!(error.code, "classificationLocateUnsupported", "{context}");
            assert!(error.message.contains("resource-1"));
            assert!(admitted(&semantic_query(context, question())).is_ok());
        }
    }

    #[test]
    fn prepare_rejects_empty_state_and_frames_the_provider_question() {
        let provider = jev_provider();
        assert_eq!(
            prepare(&Value::Null, &provider_question(), "m", provider)
                .expect_err("null context is invalid")
                .message,
            "Context value must be a non-empty string, object, or array."
        );
        assert_eq!(
            prepare(&json!({"x":1}), &provider_question(), "m", provider)
                .unwrap()
                .0,
            json!({"model":"m","state":{"x":1},"questions":{"answer":provider_question()}})
        );
        let (request, body) =
            prepare(&json!({"x":1}), &provider_question(), "m", provider).unwrap();
        assert_eq!(body, serde_json::to_vec(&request).unwrap());
    }

    #[tokio::test]
    async fn each_primitive_is_projected_without_provider_extras() {
        let provider = jev_provider();
        for (question, answer) in [
            (provider_question(), json!({"type":"noul","noul":0.8})),
            (
                json!({"type":"choice","instructions":"Pick","criteria":{"a":"First","b":"Second"}}),
                json!({"type":"choice","choice":"a","confidence":0.8,"probabilities":{"a":0.8,"b":0.2}}),
            ),
            (
                json!({"type":"score","instructions":"Rate","criteria":[{"path":"/literal/criterion"},"Good"]}),
                json!({"type":"score","score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":{"path":"/literal/criterion"},"1":"Good"}}),
            ),
        ] {
            let server = MockServer::start().await;
            let state = json!({"context":"HIDDEN_BODY"});
            let mut supplied = answer.clone();
            supplied["content"] = json!("HIDDEN_BODY");
            Mock::given(method("POST")).and(body_json(prepare(&state, &question, "m", provider).unwrap().0))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":"provider-model","answers":{"answer":supplied},"content":"HIDDEN_BODY","usage":{"input_tokens":10,"output_tokens":1,"content":"HIDDEN_BODY"}})))
                .expect(1).mount(&server).await;
            let result = judge(
                &state,
                &question,
                SecretString::from("test-key"),
                &server.uri(),
                "v1/systemone",
                "m",
                provider,
                budget(),
                0,
                &test_gate(),
            )
            .await
            .unwrap();
            // The provider's yes/no answer is published as `yesno`.
            let public = if answer["type"] == "noul" {
                json!({"type":"yesno","yesno":answer["noul"]})
            } else {
                answer.clone()
            };
            assert_eq!(
                result,
                json!({"requestedModel":"m","resolvedModel":"provider-model","answer":public,"usage":{"input_tokens":10,"output_tokens":1,"provider_calls":1}})
            );
            assert!(!result.to_string().contains("HIDDEN_BODY"));
        }
    }

    #[tokio::test]
    async fn invalid_and_cancelled_requests_never_reach_provider() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        let cancelled = budget();
        cancelled.cancellation.cancel();
        assert_eq!(
            judge(
                &json!({"state":true}),
                &provider_question(),
                SecretString::from("test-key"),
                &server.uri(),
                "v1/systemone",
                "m",
                jev_provider(),
                cancelled,
                0,
                &test_gate(),
            )
            .await
            .unwrap_err()
            .code,
            "cancelled"
        );
    }

    #[tokio::test]
    async fn provider_answer_id_or_type_drift_is_rejected() {
        for answer in [
            json!({}),
            json!({"answer":{"type":"choice"}}),
            json!({"wrong":{"type":"noul","noul":0.8}}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":"m","answers":answer,"usage":{"input_tokens":1,"output_tokens":1}}))).expect(1).mount(&server).await;
            assert_eq!(
                judge(
                    &json!({"state":true}),
                    &provider_question(),
                    SecretString::from("test-key"),
                    &server.uri(),
                    "v1/systemone",
                    "m",
                    jev_provider(),
                    budget(),
                    0,
                    &test_gate(),
                )
                .await
                .unwrap_err()
                .code,
                "invalidClassificationResponse"
            );
        }
    }
}
