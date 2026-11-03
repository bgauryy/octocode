//! Provider evidence retained only for explicitly emitted LSP continuations.
//!
//! Answers are retained as the output gate leaves them
//! ([`ContentSecurity::seal_output`]): a later page renders from them and
//! passes the same gate, a fixed point, so its rows equal a whole pass while
//! no credential a server returned (hover text, symbol details) reaches the
//! store. Data a later page sends back to the server (hierarchy items,
//! opaque `data`, URIs) must stay byte-exact; when the gate would rewrite it,
//! no continuation is issued.
use super::{LspFailure, lsp_search_query};
use crate::cache::{CacheConfig, CacheKey, Store};
use crate::security::ContentSecurity;
use octocode_contract_types::{ErrorCode, LspSearchQuery};
use octocode_engine::lsp::client::{ProviderAnswers, ResponseScope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize, Deserialize)]
struct Selected {
    query: LspSearchQuery,
    revision: String,
    answers: ProviderAnswers,
}

pub(crate) struct ContinuationAnswers {
    store: Store<Selected>,
    policy: String,
}

fn failure(code: ErrorCode, message: &str) -> LspFailure {
    LspFailure {
        code,
        message: message.into(),
        retryable: false,
        lexical_fallback: false,
    }
}
fn pinned(query: &LspSearchQuery) -> bool {
    // Snapshot is contract-private progress, shared by location, importer,
    // and hierarchy-candidate continuations. Public fresh rows carry none.
    lsp_search_query::snapshot(query).is_some()
}
fn with_snapshot(query: &LspSearchQuery, snapshot: &str) -> Result<LspSearchQuery, LspFailure> {
    let mut row = lsp_search_query::to_row(query);
    row["snapshot"] = Value::String(snapshot.into());
    serde_json::from_value(row).map_err(|_| {
        failure(
            ErrorCode::RequestFailed,
            "The saved LSP continuation could not be represented.",
        )
    })
}

impl ContinuationAnswers {
    pub(crate) fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        Self {
            store: Store::new(config, disk),
            policy,
        }
    }
    pub(crate) fn clear_memory(&self) {
        self.store.clear_memory();
    }
    fn key(&self, snapshot: &str) -> CacheKey {
        CacheKey {
            namespace: "lsp-selected-answers-v1".into(),
            scope: self.policy.clone(),
            resource: snapshot.into(),
        }
    }
    pub(super) fn open(
        &self,
        query: LspSearchQuery,
    ) -> Result<(LspSearchQuery, ResponseScope), LspFailure> {
        if !pinned(&query) {
            return Ok((query, ResponseScope::default()));
        }
        let snapshot = lsp_search_query::snapshot(&query).unwrap_or_default();
        let saved = self.store.get(&self.key(snapshot)).ok_or_else(|| failure(ErrorCode::ResponseSnapshotExpired,
            "The selected LSP provider answer is no longer retained. Restart the original request without its cursor."))?.value;
        let expected = with_snapshot(&saved.query, snapshot)?;
        if lsp_search_query::to_row(&expected) != lsp_search_query::to_row(&query) {
            return Err(failure(
                ErrorCode::InvalidInput,
                "The LSP continuation does not match its issued query.",
            ));
        }
        Ok((
            saved.query.clone(),
            ResponseScope {
                generation: saved.revision.clone(),
                answers: Some(Arc::new(Mutex::new(saved.answers.clone()))),
                ..Default::default()
            },
        ))
    }
    pub(super) fn retain(&self, row: &mut Value, scope: &ResponseScope) -> Result<(), LspFailure> {
        if row.get("status").and_then(Value::as_str) == Some("error") {
            return Ok(());
        }
        let Some(answers) = &scope.answers else {
            return Ok(());
        };
        let mut output = serde_json::json!({"results": [{"index":0, "data":std::mem::take(row)}]});
        let paths = continuation_paths(&output)?;
        if !paths.is_empty() {
            let answers = sealed(
                &answers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            )?;
            for path in paths {
                if let Some(call) = output.pointer_mut(&path) {
                    self.retain_call(call, &scope.generation, &answers)?;
                }
            }
        }
        *row = output["results"][0]["data"].take();
        Ok(())
    }
    fn retain_call(
        &self,
        value: &mut Value,
        revision: &str,
        answers: &ProviderAnswers,
    ) -> Result<(), LspFailure> {
        if value.get("tool").and_then(Value::as_str) == Some(crate::id::ToolId::LspSearch.as_str())
            && let Some(queries) = value
                .pointer_mut("/input/queries")
                .and_then(Value::as_array_mut)
        {
            for query in queries {
                let parsed =
                    serde_json::from_value::<LspSearchQuery>(query.clone()).map_err(|_| {
                        failure(
                            ErrorCode::RequestFailed,
                            "A declared LSP continuation has an invalid saved query.",
                        )
                    })?;
                if !pinned(&parsed) {
                    continue;
                }
                let selected = Selected {
                    query: parsed,
                    revision: revision.into(),
                    answers: answers.clone(),
                };
                let snapshot =
                    crate::digest::json_sha256(&serde_json::to_value(&selected).map_err(|_| {
                        failure(
                            ErrorCode::RequestFailed,
                            "Could not encode selected LSP evidence.",
                        )
                    })?);
                let bytes = octocode_engine::json_size::bytes(&selected).unwrap_or(usize::MAX);
                let retained = self
                    .store
                    .put_confirmed(
                        self.key(&snapshot),
                        selected,
                        bytes,
                        crate::cache::CONTINUATION_RETENTION,
                    )
                    .1;
                if !retained {
                    return Err(failure(
                        ErrorCode::RequestFailed,
                        "The selected LSP evidence could not be retained; no continuation was issued. Restart the original request.",
                    ));
                }
                query["snapshot"] = Value::String(snapshot);
            }
            return Ok(());
        }
        Ok(())
    }
}

/// JSON pointers to the lspSearch continuations `output` (a `results`
/// envelope) declares.
fn continuation_paths(output: &Value) -> Result<Vec<String>, LspFailure> {
    let mut paths = Vec::new();
    crate::response::continuations::visit_calls(
        crate::id::ToolId::LspSearch,
        output,
        &mut |call, path| {
            if call["kind"] == "continue" && call["tool"] == crate::id::ToolId::LspSearch.as_str() {
                paths.push(
                    path.iter()
                        .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
                        .collect::<String>(),
                );
            }
        },
    )
    .map_err(|_| {
        failure(
            ErrorCode::RequestFailed,
            "Could not inspect declared LSP continuations.",
        )
    })?;
    Ok(paths)
}

/// Whether `row` declares an lspSearch continuation, which
/// [`ContinuationAnswers::retain`] pins to the page's revision.
pub(super) fn declares_continuation(row: &mut Value) -> Result<bool, LspFailure> {
    let mut output = serde_json::json!({"results": [{"index":0, "data":std::mem::take(row)}]});
    let declared = continuation_paths(&output);
    *row = output["results"][0]["data"].take();
    Ok(!declared?.is_empty())
}

/// `answers` through the output gate, or a refusal when the gate would
/// rewrite protocol data a later page sends back to the server.
fn sealed(answers: &ProviderAnswers) -> Result<ProviderAnswers, LspFailure> {
    let encode_failure = || {
        failure(
            ErrorCode::RequestFailed,
            "Could not encode selected LSP evidence.",
        )
    };
    let raw = serde_json::to_value(answers).map_err(|_| encode_failure())?;
    let clean = ContentSecurity::new()
        .seal_output(raw.clone(), false, || Ok::<_, std::convert::Infallible>(()))
        .unwrap_or_else(|never| match never {})
        .into_value();
    if raw == clean {
        return Ok(answers.clone());
    }
    if rewrites_protocol(&raw, &clean, false) {
        return Err(failure(
            ErrorCode::ContentSecurityLimit,
            "The language server returned a credential inside protocol data that later pages send back to it, so its answer cannot be retained and no continuation was issued. Request a narrower answer (a smaller line window, another anchor or operation) that fits one page.",
        ));
    }
    serde_json::from_value(clean).map_err(|_| encode_failure())
}

/// Whether `clean` differs from `raw` in an object key, a URI, opaque
/// `data`, or a hierarchy item (an object with `uri` and `selectionRange`,
/// which incoming/outgoing and super/subtype requests send back whole).
fn rewrites_protocol(raw: &Value, clean: &Value, protocol: bool) -> bool {
    match (raw, clean) {
        (Value::Object(raw), Value::Object(clean)) => {
            if raw.len() != clean.len() || raw.keys().any(|key| !clean.contains_key(key)) {
                return true;
            }
            let item = protocol || (raw.contains_key("uri") && raw.contains_key("selectionRange"));
            raw.iter().any(|(key, value)| {
                let protocol = item || matches!(key.as_str(), "data" | "uri" | "targetUri");
                rewrites_protocol(value, &clean[key], protocol)
            })
        }
        (Value::Array(raw), Value::Array(clean)) => {
            raw.len() != clean.len()
                || raw
                    .iter()
                    .zip(clean)
                    .any(|(raw, clean)| rewrites_protocol(raw, clean, protocol))
        }
        (raw, clean) => protocol && raw != clean,
    }
}

#[cfg(test)]
mod tests;
