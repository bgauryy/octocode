//! Identical repeats served from a kept answer.
//!
//! An incoming or hierarchy answer reads the whole scope: text scans, gate
//! and include scans, test-code verdicts, declaration outlines, and the
//! server's own requests. An identical request asked again of the same
//! server, while every file the scope's fingerprint covers is unchanged,
//! computes the same row, so the first row is kept and replayed.
//!
//! The key holds what decides the row: the query without its `mainGoal` and
//! `reasoning`, the anchor file, the read policy, the scope fingerprint
//! (every family and project file, by stamp), and the server's
//! [answer state](octocode_engine::lsp::client::NativeLspClient::answer_state):
//! the same client, holding the same document text, with no work begun since.
//! Nothing is kept from a request cut short (its deadline passed or it was
//! cancelled), from a server still at work, from a row that pages on, or
//! from a row whose gaps a retry may close (indexing, a failed request).
use super::page::Target;
use super::scope::{Scope, Walked};
use super::{LspSearchQuery, Op, lsp_search_query};
use octocode_engine::lsp::client::NativeLspClient;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::paging::PartialReason;
use serde_json::Value;
use std::sync::Arc;

/// A kept row and the files its server text is about beyond the ones it
/// names ([`super::server_text::is_about`]), which its judgment reads again.
struct Kept {
    row: Value,
    about: Vec<String>,
}

/// Kept rows against the one process budget, by key and read policy.
static ANSWERS: octocode_tools_shared::page_memo::PageMemo<Arc<Kept>> =
    octocode_tools_shared::page_memo::PageMemo::new();

/// Partial reasons a later identical request may not repeat: the server
/// still loading or unwell, or a request that failed. Work cut by the
/// request's deadline or cancellation is never kept ([`AnswerKey::keep`]).
const TRANSIENT: [PartialReason; 10] = [
    PartialReason::LanguageServerIndexing,
    PartialReason::Cancelled,
    PartialReason::ServerUnhealthy,
    PartialReason::ImporterScanFailed,
    PartialReason::ImporterRequestFailed,
    PartialReason::AliasRecoveryFailed,
    PartialReason::CallHierarchyExpansionFailed,
    PartialReason::TypeHierarchyExpansionFailed,
    PartialReason::DefinitionHopFailed,
    PartialReason::DiagnosticsNotPublished,
];

/// The part of an answer's key that holds for the whole request: the
/// server state is read when the row is looked up and when it is kept.
pub(super) struct AnswerKey {
    asked: String,
    policy: String,
}

impl AnswerKey {
    /// Whether `query`'s answer may be kept: a first page of an operation
    /// that reads the scope.
    pub(super) fn wanted(query: &LspSearchQuery) -> bool {
        #[cfg(test)]
        if BYPASS.with(std::cell::Cell::get) {
            return false;
        }
        matches!(
            lsp_search_query::op(query),
            Op::References
                | Op::Callers
                | Op::Callees
                | Op::Implementation
                | Op::Supertypes
                | Op::Subtypes
                | Op::WorkspaceSymbol
        ) && lsp_search_query::snapshot(query).is_none()
            && lsp_search_query::page(query).unwrap_or(1) <= 1
            && lsp_search_query::importer_page(query) <= 1
    }

    /// The key of `query`'s answer on `target` within `scope`, whose walk
    /// `walked` proved its fingerprint; `None` for an answer that is not
    /// kept: a page or importer window past the first, an operation that
    /// reads little beyond its anchor, or a scope with no fingerprint.
    pub(super) fn of(
        query: &LspSearchQuery,
        target: &Target,
        scope: &Scope,
        walked: Option<&Walked>,
        paths: &PathPolicy,
    ) -> Option<Self> {
        if !Self::wanted(query) {
            return None;
        }
        let (fingerprint, _) = walked?.fingerprint.as_ref()?;
        let mut asked = lsp_search_query::to_row(query);
        if let Some(fields) = asked.as_object_mut() {
            fields.remove("mainGoal");
            fields.remove("reasoning");
        }
        let asked = serde_json::json!([
            asked,
            target.path,
            target.root_only,
            target.allow_build_scripts,
            scope.root,
            scope.include,
            fingerprint,
        ]);
        Some(Self {
            asked: octocode_engine::digest::sha256(asked.to_string().as_bytes()),
            policy: paths.identity(),
        })
    }

    /// The full key with the server's state now; `None` while it works.
    fn with_state(&self, client: &NativeLspClient) -> Option<String> {
        Some(format!(
            "lsp-answer:{}:{}",
            self.asked,
            client.answer_state()?
        ))
    }

    /// The kept row of this request, its judged files noted again.
    pub(super) fn kept(&self, client: &NativeLspClient) -> Option<Value> {
        let key = self.with_state(client)?;
        let kept = ANSWERS.get(&key, &self.policy, Arc::clone)?;
        #[cfg(test)]
        HITS.with(|hits| hits.set(hits.get() + 1));
        for file in &kept.about {
            super::server_text::is_about(file.clone());
        }
        Some(kept.row.clone())
    }

    /// Keep `row` for an identical repeat, under the server's state now (the
    /// request's own opens included), unless it pages on, failed, was cut
    /// short, or holds a gap a retry may close.
    pub(super) fn keep(
        self,
        row: &Value,
        client: &NativeLspClient,
        cancel: &dyn CancellationCheck,
    ) {
        let cut = cancel.check().is_err()
            || std::time::Instant::now()
                >= octocode_runtime_base::execution::cancel::deadline_of(cancel);
        if cut || !keeps(row) {
            return;
        }
        let Some(key) = self.with_state(client) else {
            return;
        };
        let about = super::server_text::noted();
        let bytes = row.to_string().len() + about.iter().map(String::len).sum::<usize>();
        ANSWERS.put(
            key,
            self.policy,
            bytes,
            Arc::new(Kept {
                row: row.clone(),
                about,
            }),
        );
    }
}

/// Whether `row` may serve an identical repeat: a settled, single-page
/// answer with no gap a retry may close.
fn keeps(row: &Value) -> bool {
    if row.get("status").and_then(Value::as_str) == Some("error")
        || row.pointer("/pagination/hasMore").and_then(Value::as_bool) == Some(true)
        || row
            .get("next")
            .and_then(Value::as_object)
            .is_some_and(|next| !next.is_empty())
    {
        return false;
    }
    let transient = TRANSIENT.map(|reason| reason.to_string());
    let listed = |reason: &Value| {
        reason
            .as_str()
            .is_some_and(|reason| transient.iter().any(|known| known == reason))
    };
    !row.get("partialReasons")
        .and_then(Value::as_array)
        .is_some_and(|reasons| reasons.iter().any(listed))
        && !row.pointer("/coverage/reason").is_some_and(listed)
}

#[cfg(test)]
thread_local! {
    /// Requests on this thread compute their answer whatever is kept.
    static BYPASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Kept answers served on this thread.
    static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Run `work` with every request on this thread computing its answer.
#[cfg(test)]
pub(super) fn computed<T>(work: impl FnOnce() -> T) -> T {
    BYPASS.with(|bypass| bypass.set(true));
    let done = work();
    BYPASS.with(|bypass| bypass.set(false));
    done
}

/// Kept answers this thread served so far.
#[cfg(test)]
pub(super) fn hits() -> usize {
    HITS.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use super::keeps;
    use serde_json::json;

    #[test]
    fn only_settled_single_page_rows_are_kept() {
        assert!(keeps(
            &json!({"files": [], "partialReasons": ["cfgGatedFiles"]})
        ));
        assert!(!keeps(&json!({"status": "error"})));
        assert!(!keeps(&json!({"pagination": {"hasMore": true}})));
        assert!(!keeps(&json!({"next": {"nextImporterWindow": {}}})));
        assert!(!keeps(
            &json!({"partialReasons": ["languageServerIndexing"]})
        ));
        assert!(!keeps(
            &json!({"coverage": {"reason": "importerRequestFailed"}})
        ));
        assert!(keeps(&json!({"next": {}})));
    }
}
