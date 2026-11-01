//! The breadth-first walk: one level per step, node and fan-out caps, merged
//! edges, and resume points.

use super::nodes::{NodeKeys, builtin_name, is_builtin_lib_declaration};
use super::{
    Expansion, HierarchyEdge, HierarchySource, HierarchyWalk, MAX_HIERARCHY_FAN_OUT,
    MAX_HIERARCHY_NODES, Resume,
};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::cancellable;
use crate::tools::lsp_search::failure::LspFailure;
use crate::tools::lsp_search::render::into_items;
use futures_util::StreamExt;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Concurrent expansion requests per level.
const LEVEL_CONCURRENCY: usize = 4;

/// Breadth-first walk from `roots` up to `depth` levels (at least one; the
/// walk ends sooner where the hierarchy does). Cancellation is checked before every level and
/// while each level's requests are in flight.
#[cfg(test)]
pub(in crate::tools::lsp_search) async fn walk_hierarchy(
    source: &impl HierarchySource,
    roots: &[Value],
    expansion: Expansion,
    depth: u32,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<HierarchyWalk, LspFailure> {
    Walk::new(paths, roots, expansion, depth)
        .finish(source, cancel)
        .await
}

/// A breadth-first walk in progress, one level per [`Walk::step`]. Roots
/// found after level 1 (verified importer call sites) join level 1 through
/// [`Walk::add_roots`], so the server's own level-1 answer can decide which
/// importers still need verifying.
pub(in crate::tools::lsp_search) struct Walk<'a> {
    walker: Walker<'a>,
    expansion: Expansion,
    /// The nodes the next level expands.
    frontier: Vec<Value>,
    /// The level the next step expands.
    level: u32,
}

impl<'a> Walk<'a> {
    pub(in crate::tools::lsp_search) fn new(
        paths: &'a PathPolicy,
        roots: &[Value],
        expansion: Expansion,
        depth: u32,
    ) -> Self {
        let depth = depth.max(1);
        Self {
            walker: Walker::new(paths, roots, depth),
            expansion,
            frontier: roots.to_vec(),
            level: 1,
        }
    }

    /// Expand `nodes` at `level`; returns the nodes the next level expands.
    async fn expand_level(
        &mut self,
        source: &impl HierarchySource,
        nodes: Vec<Value>,
        level: u32,
        cancel: &dyn CancellationCheck,
    ) -> Result<Vec<Value>, LspFailure> {
        cancel.check().map_err(LspFailure::cancelled)?;
        let expansion = self.expansion;
        let requests = nodes
            .iter()
            .map(|node| source.expand(expansion, node.clone()));
        let responses = cancellable(
            cancel,
            futures_util::stream::iter(requests)
                .buffered(LEVEL_CONCURRENCY)
                .collect::<Vec<_>>(),
        )
        .await?;
        let mut next = Vec::new();
        for (parent, response) in nodes.iter().zip(responses) {
            match response {
                Ok(results) => {
                    self.walker
                        .expand(expansion, level, parent, into_items(results), &mut next);
                }
                Err(error) => self.walker.walk.failures.push(error),
            }
        }
        Ok(next)
    }

    /// Expand the next level; `false` once the walk is complete.
    pub(in crate::tools::lsp_search) async fn step(
        &mut self,
        source: &impl HierarchySource,
        cancel: &dyn CancellationCheck,
    ) -> Result<bool, LspFailure> {
        if self.level > self.walker.depth || self.frontier.is_empty() {
            return Ok(false);
        }
        let nodes = std::mem::take(&mut self.frontier);
        let level = self.level;
        self.frontier = self.expand_level(source, nodes, level, cancel).await?;
        self.level += 1;
        Ok(true)
    }

    /// Expand `roots` not already in the walk at level 1, after level 1 of
    /// the original roots ran; their results join the next level. A level-1
    /// result in a `foreign` file (canonical path) is left out: another
    /// importer window, or the anchor's own answer, lists it.
    pub(in crate::tools::lsp_search) async fn add_roots(
        &mut self,
        source: &impl HierarchySource,
        roots: Vec<Value>,
        foreign: HashSet<String>,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), LspFailure> {
        self.walker.foreign = foreign;
        // A walk with no roots of its own (a later importer page) starts
        // here: the results of these level-1 roots expand at level 2.
        self.level = self.level.max(2);
        let fresh = roots
            .into_iter()
            .filter(|root| {
                let key = self.walker.keys.key(root);
                self.walker.seen.insert(key)
            })
            .collect::<Vec<_>>();
        if fresh.is_empty() {
            return Ok(());
        }
        let next = self.expand_level(source, fresh, 1, cancel).await?;
        self.frontier.extend(next);
        Ok(())
    }

    /// Canonical files of the level-1 results: the files the server's own
    /// answer covers.
    pub(in crate::tools::lsp_search) fn answered_files(&self) -> HashSet<String> {
        self.walker
            .walk
            .edges
            .iter()
            .filter(|edge| edge.level == 1)
            .filter_map(|edge| edge.node.get("uri").and_then(Value::as_str))
            .map(crate::tools::lsp_search::scope::canonical_uri)
            .collect()
    }

    /// Run the remaining levels.
    pub(in crate::tools::lsp_search) async fn finish(
        mut self,
        source: &impl HierarchySource,
        cancel: &dyn CancellationCheck,
    ) -> Result<HierarchyWalk, LspFailure> {
        while self.step(source, cancel).await? {}
        Ok(self.walker.walk)
    }
}

/// The state of one walk: discovered edges, seen nodes, and resume points.
struct Walker<'a> {
    keys: NodeKeys<'a>,
    walk: HierarchyWalk,
    seen: HashSet<String>,
    edge_index: HashMap<(String, String), usize>,
    resumed: HashSet<String>,
    depth: u32,
    /// Canonical files whose level-1 results importer roots leave out.
    foreign: HashSet<String>,
}

impl<'a> Walker<'a> {
    fn new(paths: &'a PathPolicy, roots: &[Value], depth: u32) -> Self {
        let mut keys = NodeKeys::new(paths);
        let seen = roots.iter().map(|root| keys.key(root)).collect();
        Self {
            keys,
            walk: HierarchyWalk::default(),
            seen,
            edge_index: HashMap::new(),
            resumed: HashSet::new(),
            depth,
            foreign: HashSet::new(),
        }
    }

    /// Record `parent` as a resume point, once.
    fn resume(&mut self, key: String, node: Value, depth: u32) {
        if self.resumed.insert(key) {
            self.walk.resumes.push(Resume { node, depth });
        }
    }

    /// Record one parent's `results` at `level`; new nodes to expand next go
    /// to `next`.
    fn expand(
        &mut self,
        expansion: Expansion,
        level: u32,
        parent: &Value,
        mut results: Vec<Value>,
        next: &mut Vec<Value>,
    ) {
        let parent_key = self.keys.key(parent);
        // Server order varies between server instances; the caps below
        // keep a prefix, so the walk orders results itself.
        results.sort_by_cached_key(|result| {
            expansion
                .node_of(result)
                .map(|node| self.keys.order(node))
                .unwrap_or_default()
        });
        // The anchor's results are all listed (the row pages them).
        let fan_out = if level == 1 {
            usize::MAX
        } else {
            MAX_HIERARCHY_FAN_OUT
        };
        if results.len() > fan_out {
            self.walk.fan_out_capped.push(
                parent
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_owned(),
            );
            self.resume(parent_key.clone(), parent.clone(), self.depth - level + 1);
        }
        for result in results.into_iter().take(fan_out) {
            self.record(expansion, level, parent, &parent_key, &result, next);
        }
    }

    fn record(
        &mut self,
        expansion: Expansion,
        level: u32,
        parent: &Value,
        parent_key: &str,
        result: &Value,
        next: &mut Vec<Value>,
    ) {
        let Some(node) = expansion.node_of(result).cloned() else {
            return;
        };
        if is_builtin_lib_declaration(&node) {
            let name = builtin_name(&node);
            if !self.walk.builtin_lib.contains(&name) {
                self.walk.builtin_lib.push(name);
            }
            return;
        }
        if !self.keys.authorized(result) {
            self.walk.out_of_policy += 1;
            return;
        }
        if level == 1
            && !self.foreign.is_empty()
            && node.get("uri").and_then(Value::as_str).is_some_and(|uri| {
                self.foreign
                    .contains(&crate::tools::lsp_search::scope::canonical_uri(uri))
            })
        {
            return;
        }
        let node_key = self.keys.key(&node);
        let sites = result
            .get("fromRanges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let pair = (parent_key.to_owned(), node_key.clone());
        if let Some(&index) = self.edge_index.get(&pair) {
            let edge = &mut self.walk.edges[index];
            for site in sites {
                if !edge.sites.contains(&site) {
                    edge.sites.push(site);
                }
            }
            return;
        }
        let is_new = !self.seen.contains(&node_key);
        let full = self.seen.len() >= MAX_HIERARCHY_NODES;
        if is_new && full && level > 1 {
            self.walk.dropped_edges += 1;
            self.resume(
                parent_key.to_owned(),
                parent.clone(),
                self.depth - level + 1,
            );
            return;
        }
        self.edge_index.insert(pair, self.walk.edges.len());
        self.walk.edges.push(HierarchyEdge {
            node: node.clone(),
            parent: Some(parent.clone()),
            level,
            sites,
        });
        if !is_new {
            return;
        }
        self.seen.insert(node_key.clone());
        if level >= self.depth {
            return;
        }
        if full {
            // An anchor result past the node cap: listed, expanded by its own
            // continuation.
            self.walk.unexpanded_nodes += 1;
            self.resume(node_key, node, self.depth - level);
        } else {
            next.push(node);
        }
    }
}
