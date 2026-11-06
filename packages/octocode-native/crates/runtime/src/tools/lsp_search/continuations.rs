//! Provider evidence retained only for explicitly emitted LSP continuations.
//!
//! Answers are retained as the output gate leaves them
//! ([`ContentSecurity::seal_output`]): a later page renders from them and
//! passes the same gate, a fixed point, so its rows equal a whole pass while
//! no credential a server returned (hover text, symbol details) reaches the
//! store. Data a later page sends back to the server (hierarchy items,
//! opaque `data`, URIs) must stay byte-exact: where the gate rewrites it
//! (a JDT handle spells `key="L…"`), the store keeps the gated bytes and
//! the server's own bytes stay in this process only ([`Held`]), so a page
//! read after a restart restarts instead of sending the server a rewritten
//! item. No continuation is refused for it.
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
    /// JSON pointers (into `answers`) of the protocol values the gate
    /// rewrote: `answers` holds them gated; their server bytes are [`Held`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    held: Vec<String>,
}

/// The server's bytes of the protocol values a [`Selected`] holds gated,
/// by pointer, in this process only: they go back to the server and are
/// never shown, and no store writes them.
type Held = Vec<(String, Value)>;

pub(crate) struct ContinuationAnswers {
    store: Store<Selected>,
    /// Always in memory, whatever the storage mode.
    held: Store<Held>,
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
            store: Store::new(config.clone(), disk),
            held: Store::new(config, None),
            policy,
        }
    }
    pub(crate) fn clear_memory(&self) {
        self.store.clear_memory();
        self.held.clear_memory();
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
        let answers = if saved.held.is_empty() {
            saved.answers.clone()
        } else {
            let held = self.held.get(&self.key(snapshot)).ok_or_else(|| failure(ErrorCode::ResponseSnapshotExpired,
                "The selected LSP provider answer is no longer retained in this process. Restart the original request without its cursor."))?.value;
            restore(&saved.answers, &held)?
        };
        Ok((
            saved.query.clone(),
            ResponseScope {
                generation: saved.revision.clone(),
                answers: Some(Arc::new(Mutex::new(answers))),
                ..Default::default()
            },
        ))
    }
    pub(super) fn retain(
        &self,
        row: &mut Value,
        scope: &ResponseScope,
        deadline: std::time::Instant,
    ) -> Result<(), LspFailure> {
        if row.get("status").and_then(Value::as_str) == Some("error") {
            return Ok(());
        }
        let Some(answers) = &scope.answers else {
            return Ok(());
        };
        in_output(row, |output| {
            let paths = continuation_paths(output)?;
            if paths.is_empty() {
                return Ok(());
            }
            let sealed = sealed(
                &answers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            )?;
            for path in paths {
                if let Some(call) = output.pointer_mut(&path) {
                    self.retain_call(call, &scope.generation, &sealed, deadline)?;
                }
            }
            Ok(())
        })
    }
    fn retain_call(
        &self,
        value: &mut Value,
        revision: &str,
        (answers, held): &(ProviderAnswers, Held),
        deadline: std::time::Instant,
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
                    held: held.iter().map(|(pointer, _)| pointer.clone()).collect(),
                };
                // The held bytes are part of what the snapshot names: two
                // answers that gate alike but differ in them are two.
                let named = if held.is_empty() {
                    serde_json::to_value(&selected)
                } else {
                    serde_json::to_value((&selected, held))
                };
                let snapshot = crate::digest::json_sha256(&named.map_err(|_| encode_failure())?);
                let bytes = octocode_engine::json_size::bytes(&selected).unwrap_or(usize::MAX);
                let mut retained = true;
                if !held.is_empty() {
                    let held_bytes = octocode_engine::json_size::bytes(held).unwrap_or(usize::MAX);
                    retained = self
                        .held
                        .put_until(
                            deadline,
                            self.key(&snapshot),
                            held.clone(),
                            held_bytes,
                            crate::cache::CONTINUATION_RETENTION,
                        )
                        .1;
                }
                retained = retained
                    && self
                        .store
                        .put_until(
                            deadline,
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
                paths.push(crate::json_pointer::of(path));
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
    in_output(row, |output| Ok(!continuation_paths(output)?.is_empty()))
}

/// `visit` of `row` as the one result of a tool output (the `results`
/// envelope the output schema describes); `row` is back in place after,
/// whatever `visit` returns.
fn in_output<T>(row: &mut Value, visit: impl FnOnce(&mut Value) -> T) -> T {
    let mut output = serde_json::json!({"results": [{"index": 0, "data": std::mem::take(row)}]});
    let visited = visit(&mut output);
    *row = output["results"][0]["data"].take();
    visited
}

fn encode_failure() -> LspFailure {
    failure(
        ErrorCode::RequestFailed,
        "Could not encode selected LSP evidence.",
    )
}

/// `answers` through the output gate, with the server's bytes of each
/// protocol value the gate rewrote (see [`protocol_rewrites`]). A gate that
/// changed the answer's shape (a key) leaves nothing to restore by pointer:
/// no continuation is issued.
fn sealed(answers: &ProviderAnswers) -> Result<(ProviderAnswers, Held), LspFailure> {
    let raw = serde_json::to_value(answers).map_err(|_| encode_failure())?;
    let clean = ContentSecurity::new()
        .seal_output(raw.clone(), false, || Ok::<_, std::convert::Infallible>(()))
        .unwrap_or_else(|never| match never {})
        .into_value();
    if raw == clean {
        return Ok((answers.clone(), Held::new()));
    }
    let mut held = Held::new();
    if !protocol_rewrites(&raw, &clean, false, &mut String::new(), &mut held) {
        return Err(failure(
            ErrorCode::ContentSecurityLimit,
            "The output gate changed the shape of the language server's answer, so it cannot be retained and no continuation was issued. Request a narrower answer (a smaller line window, another anchor or operation) that fits one page.",
        ));
    }
    let clean = serde_json::from_value(clean).map_err(|_| encode_failure())?;
    Ok((clean, held))
}

/// `answers` with each held pointer set back to the server's bytes.
fn restore(answers: &ProviderAnswers, held: &Held) -> Result<ProviderAnswers, LspFailure> {
    let mut value = serde_json::to_value(answers).map_err(|_| encode_failure())?;
    for (pointer, raw) in held {
        *value.pointer_mut(pointer).ok_or_else(encode_failure)? = raw.clone();
    }
    serde_json::from_value(value).map_err(|_| encode_failure())
}

/// Collect into `held`, by JSON pointer, the server's bytes of every value
/// the gate rewrote inside a URI, opaque `data`, or a hierarchy item (an
/// object with `uri` and `selectionRange`, which incoming/outgoing and
/// super/subtype requests send back whole). `false` when `clean` differs
/// from `raw` in shape: an object key or an array length.
fn protocol_rewrites(
    raw: &Value,
    clean: &Value,
    protocol: bool,
    pointer: &mut String,
    held: &mut Held,
) -> bool {
    match (raw, clean) {
        (Value::Object(raw), Value::Object(clean)) => {
            if raw.len() != clean.len() || raw.keys().any(|key| !clean.contains_key(key)) {
                return false;
            }
            let item = protocol || (raw.contains_key("uri") && raw.contains_key("selectionRange"));
            raw.iter().all(|(key, value)| {
                let protocol = item || matches!(key.as_str(), "data" | "uri" | "targetUri");
                let at = pointer.len();
                pointer.push('/');
                pointer.push_str(&crate::json_pointer::escape(key));
                let same_shape = protocol_rewrites(value, &clean[key], protocol, pointer, held);
                pointer.truncate(at);
                same_shape
            })
        }
        (Value::Array(raw), Value::Array(clean)) => {
            raw.len() == clean.len()
                && raw
                    .iter()
                    .zip(clean)
                    .enumerate()
                    .all(|(index, (raw, clean))| {
                        let at = pointer.len();
                        pointer.push('/');
                        pointer.push_str(&index.to_string());
                        let same_shape = protocol_rewrites(raw, clean, protocol, pointer, held);
                        pointer.truncate(at);
                        same_shape
                    })
        }
        (raw, clean) => {
            if protocol && raw != clean {
                held.push((pointer.clone(), raw.clone()));
            }
            true
        }
    }
}

#[cfg(test)]
mod tests;
