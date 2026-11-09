//! Call and type hierarchies from the editor: roots prepared at the anchor,
//! then the shared breadth-first walk over the bridge's incoming, outgoing,
//! supertypes and subtypes requests.
use super::Fault;
use super::client::{Bridge, BridgeError};
use super::ops::Question;
use crate::tools::lsp_search::failure::LspFailure;
use crate::tools::lsp_search::render::{decode_uri_path, into_items};
use crate::tools::lsp_search::source::item_uri_is_authorized;
use crate::tools::lsp_search::walk::{
    Expansion, HierarchySource, NodeKeys, Walk, call_items_payload, mark_partial_expansion,
    mark_truncation, public_edge, scope_callers,
};
use crate::tools::lsp_search::{Op, cancellable, lsp_search_query};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Mutex;

/// The bridge method that prepares the items an expansion walks from.
fn prepare_method(expansion: Expansion) -> &'static str {
    if expansion.is_call() {
        "prepareCallHierarchy"
    } else {
        "prepareTypeHierarchy"
    }
}

fn expand_method(expansion: Expansion) -> &'static str {
    match expansion {
        Expansion::IncomingCalls => "incomingCalls",
        Expansion::OutgoingCalls => "outgoingCalls",
        Expansion::Supertypes => "supertypes",
        Expansion::Subtypes => "subtypes",
    }
}

/// The walk's requests, answered by the bridge. A failure is kept as the
/// walk's data (as a server's is) and its reason is remembered, so a walk
/// that found nothing can name why the rung did not answer.
struct Expander<'a> {
    bridge: &'a Bridge,
    first_failure: Mutex<Option<String>>,
}

impl Expander<'_> {
    fn fail(&self, error: BridgeError) -> LspFailure {
        let reason = error.reason();
        self.first_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert_with(|| reason.clone());
        LspFailure::unestablished(reason)
    }

    /// `item` prepared again at its own position: the editor forgot the
    /// handle it carried. The same symbol is the one of the same name, else
    /// the first.
    async fn prepared_again(
        &self,
        expansion: Expansion,
        item: &Value,
    ) -> Result<Value, BridgeError> {
        let place = item
            .pointer("/selectionRange/start")
            .or_else(|| item.pointer("/range/start"));
        let (Some(uri), Some(place)) = (item.get("uri").and_then(Value::as_str), place) else {
            return Err(BridgeError::Transport(
                "the item names no position to prepare again".to_owned(),
            ));
        };
        let path = decode_uri_path(uri).map_err(BridgeError::Transport)?;
        let prepared = self
            .bridge
            .ask(
                prepare_method(expansion),
                json!({ "path": path, "line": place["line"], "character": place["character"] }),
            )
            .await?;
        let mut candidates = into_items(prepared);
        let same = candidates
            .iter()
            .position(|candidate| candidate.get("name") == item.get("name"))
            .unwrap_or(0);
        if same >= candidates.len() {
            return Err(BridgeError::Status {
                code: 404,
                reason: "the item could not be prepared again".to_owned(),
            });
        }
        Ok(candidates.swap_remove(same))
    }
}

impl HierarchySource for Expander<'_> {
    async fn expand(&self, expansion: Expansion, item: Value) -> Result<Value, LspFailure> {
        let method = expand_method(expansion);
        let asked = self.bridge.ask(method, json!({ "item": item })).await;
        let answer = match asked {
            // Once: a handle that is stale again after preparing is a failure.
            Err(error) if error.is_stale_handle() => {
                match self.prepared_again(expansion, &item).await {
                    Ok(fresh) => self.bridge.ask(method, json!({ "item": fresh })).await,
                    Err(error) => Err(error),
                }
            }
            other => other,
        };
        answer.map_err(|error| self.fail(error))
    }
}

/// The page of a hierarchy operation.
pub(super) async fn run(question: &Question<'_, '_>) -> Result<Value, Fault> {
    let query = question.query;
    let (expansion, paths) = (
        match lsp_search_query::op(query) {
            Op::Callers => Expansion::IncomingCalls,
            Op::Callees => Expansion::OutgoingCalls,
            Op::Supertypes => Expansion::Supertypes,
            _ => Expansion::Subtypes,
        },
        question.sources.policy(),
    );
    let prepared = cancellable(
        question.cancel,
        question
            .bridge
            .ask(prepare_method(expansion), question.anchor()),
    )
    .await?
    .map_err(Fault::from)?;
    let mut keys = NodeKeys::new(paths);
    let mut seen = HashSet::new();
    let roots = into_items(prepared)
        .into_iter()
        .filter(|root| item_uri_is_authorized(root, paths))
        .filter(|root| seen.insert(keys.key(root)))
        .collect::<Vec<_>>();
    let expander = Expander {
        bridge: question.bridge,
        first_failure: Mutex::new(None),
    };
    let depth = lsp_search_query::depth(query).unwrap_or(1);
    let walk = Walk::new(paths, &roots, expansion, depth)
        .finish(&expander, question.cancel)
        .await?;
    // A failure with nothing found is not an absence: the rung did not answer.
    if walk.edges.is_empty()
        && let Some(reason) = expander
            .first_failure
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        return Err(Fault::Fallback(reason));
    }
    let mut items = walk
        .edges
        .iter()
        .map(|edge| public_edge(expansion, edge, None))
        .collect::<Vec<_>>();
    let excluded = scope_callers(query, paths, &mut items);
    let mut row = call_items_payload(query, &items, None, question.budget, paths);
    crate::tools::lsp_search::test_scope::disclose(&mut row, excluded);
    mark_partial_expansion(&mut row, query, &walk.failures, paths);
    mark_truncation(&mut row, query, &[(expansion, &walk)], paths);
    Ok(row)
}
