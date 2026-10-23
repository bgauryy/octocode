//! Call- and type-hierarchy walks (`callers`, `callees`,
//! `supertypes`, `subtypes`).
//!
//! The walk is breadth-first, one level at a time. Nodes are keyed by
//! `(canonical path, selectionRange start/end)` — never by name or by the
//! whole serialized item, whose opaque `data`/`detail` vary. Every edge
//! carries its `level` and, below level 1, the parent (`via`) it connects
//! to; repeated `(parent, node)` pairs merge into one edge with the union of
//! their call sites. A node reached again (cycle or diamond) keeps its edge
//! but is not expanded twice. Depth, total nodes, and per-node fan-out are
//! capped in code; each level's requests run concurrently under a small
//! bound; per-node failures are collected as data.

use super::failure::LspFailure;
use octocode_engine::lsp::client::NativeLspClient;
use serde_json::Value;

mod bfs;
mod caller_names;
mod compact;
mod importer_recovery;
mod nodes;
mod outcome;
mod public;
mod request;

pub(super) use bfs::Walk;
pub(super) use importer_recovery::ImporterRecovery;
pub(super) use nodes::NodeKeys;
pub(super) use outcome::{mark_partial_expansion, mark_truncation};
pub(super) use public::public_edge;
pub(super) use request::{HierarchyRequest, hierarchy};
#[cfg(test)]
pub(super) use {
    bfs::walk_hierarchy,
    caller_names::{ClassFacts, split_module_callers},
    compact::{call_items_payload, compact_calls},
    nodes::{builtin_name, is_builtin_lib_declaration},
    outcome::expansion_outcome,
    request::class_uses_warning,
};

/// Hard ceiling on `depth` (the schema maximum), enforced here too.
pub(super) const MAX_HIERARCHY_DEPTH: u32 = 20;
/// Distinct nodes one walk expands toward (roots included). Past the cap,
/// edges to new nodes below the anchor are dropped and the anchor's own
/// results stay listed but unexpanded; each such parent carries a resumable
/// continuation.
pub(super) const MAX_HIERARCHY_NODES: usize = 200;
/// Results kept per expanded node below the anchor; a wider node resumes as
/// the anchor of its own continuation, where every result is kept (the
/// anchor's results are paged, never cut).
pub(super) const MAX_HIERARCHY_FAN_OUT: usize = 50;

/// One hierarchy expansion request kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Expansion {
    IncomingCalls,
    OutgoingCalls,
    Supertypes,
    Subtypes,
}

impl Expansion {
    /// The hierarchy node a result leads to, expanded at the next level.
    fn node_of(self, result: &Value) -> Option<&Value> {
        match self {
            Self::IncomingCalls => result.get("from"),
            Self::OutgoingCalls => result.get("to"),
            Self::Supertypes | Self::Subtypes => Some(result),
        }
    }

    fn is_call(self) -> bool {
        matches!(self, Self::IncomingCalls | Self::OutgoingCalls)
    }
}

/// The language-server requests a hierarchy walk issues; a seam so the walk
/// is testable over a fake graph. A failure keeps its typed kind, so the
/// row retries only what asking again can change.
pub(super) trait HierarchySource {
    async fn expand(&self, expansion: Expansion, item: Value) -> Result<Value, LspFailure>;
}

impl HierarchySource for NativeLspClient {
    async fn expand(&self, expansion: Expansion, item: Value) -> Result<Value, LspFailure> {
        match expansion {
            Expansion::IncomingCalls => self.incoming_calls(item).await,
            Expansion::OutgoingCalls => self.outgoing_calls(item).await,
            Expansion::Supertypes => self.type_hierarchy_supertypes(item).await,
            Expansion::Subtypes => self.type_hierarchy_subtypes(item).await,
        }
        .map_err(LspFailure::from)
    }
}

/// One discovered edge. `node` is the raw hierarchy item the edge leads to,
/// `parent` the raw item it was expanded from (`None` only for a
/// reference-derived caller), and `sites` the raw call-site ranges
/// (`fromRanges`).
#[derive(Debug)]
pub(super) struct HierarchyEdge {
    pub(super) node: Value,
    pub(super) parent: Option<Value>,
    pub(super) level: u32,
    pub(super) sites: Vec<Value>,
}

/// Where a node-capped walk can resume: re-anchor on `node` and walk the
/// remaining `depth` levels.
#[derive(Debug)]
pub(super) struct Resume {
    pub(super) node: Value,
    pub(super) depth: u32,
}

#[derive(Debug, Default)]
pub(super) struct HierarchyWalk {
    pub(super) edges: Vec<HierarchyEdge>,
    pub(super) failures: Vec<LspFailure>,
    /// Edges to new nodes dropped by [`MAX_HIERARCHY_NODES`].
    pub(super) dropped_edges: usize,
    /// Every parent whose new children the node cap dropped, in walk order
    /// (each once); each gets an executable continuation.
    pub(super) resumes: Vec<Resume>,
    /// Names of nodes whose results were cut to [`MAX_HIERARCHY_FAN_OUT`];
    /// each is also a resume point.
    pub(super) fan_out_capped: Vec<String>,
    /// Anchor results listed past the node cap without being expanded; each
    /// is also a resume point.
    pub(super) unexpanded_nodes: usize,
    /// Results naming a file outside the read policy (skipped).
    pub(super) out_of_policy: usize,
    /// Results declared in a TypeScript built-in lib file (`lib.*.d.ts`,
    /// e.g. `String.toUpperCase`), kept out of the call graph and listed by
    /// name once each.
    pub(super) builtin_lib: Vec<String>,
}
