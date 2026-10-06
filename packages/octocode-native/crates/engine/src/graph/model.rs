use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::digest::sha256;

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct GraphRange {
    pub start: GraphPosition,
    pub end: GraphPosition,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactDeclaration {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub range: GraphRange,
    pub selection_range: GraphRange,
    pub exported: bool,
    /// Public names when an exported local binding is exported under another
    /// name (`export { foo as bar }`, named default exports).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exported_as: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// 0-based first line of the comment block directly above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_line: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactImport {
    pub id: String,
    pub specifier: String,
    pub line: u32,
    pub import_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_range: Option<GraphRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_range: Option<GraphRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_scope: Option<Vec<String>>,
    /// Where the imported binding is used: the id of the innermost
    /// declaration around each reference, or [`IMPORT_USE_MODULE`] for
    /// module-level code (export clauses included). Absent when the producer
    /// cannot tell (namespace imports, re-exports, trait or test scopes):
    /// consumers must then treat the import as used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_in: Option<Vec<String>>,
}

/// [`GraphFactImport::used_in`] entry for a use outside every declaration.
pub const IMPORT_USE_MODULE: &str = "module";

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactExport {
    pub id: String,
    pub name: String,
    pub line: u32,
    pub export_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactCall {
    pub id: String,
    pub caller: String,
    /// Declaration id of the enclosing caller; absent for module-level code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    pub callee: String,
    pub line: u32,
    pub range: GraphRange,
    pub kind: String,
    /// Syntactic type of a member call's receiver (`x` in `x.m()`) as written,
    /// without references, pointers or generic arguments; absent when the
    /// parser cannot read it from a declaration, constructor or field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub source: String,
    pub line: u32,
    pub resolution: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactRustModule {
    pub name: String,
    pub line: u32,
    pub scope: Vec<String>,
    pub inline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub unsupported: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactCommonJs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub specifier: Option<String>,
    pub line: u32,
    pub kind: String,
    pub binding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphFactsDocument {
    pub kind: String,
    pub schema_version: u32,
    pub source: String,
    pub language: String,
    pub file: String,
    pub declarations: Vec<GraphFactDeclaration>,
    pub imports: Vec<GraphFactImport>,
    pub exports: Vec<GraphFactExport>,
    pub calls: Vec<GraphFactCall>,
    pub common_js: Vec<GraphFactCommonJs>,
    pub edges: Vec<GraphFactEdge>,
    pub diagnostics: Vec<String>,
    pub modules: Vec<GraphFactRustModule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_root_unsupported: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct GraphFactsTypedEntry {
    pub relative_path: String,
    pub content_digest: String,
    pub facts: GraphFactsDocument,
    pub reference_counts: Vec<crate::types::GraphReferenceCount>,
}

#[derive(Clone, Debug)]
pub struct GraphFactsTypedScanResult {
    pub schema_version: u32,
    pub entries: Vec<GraphFactsTypedEntry>,
    pub skipped: Vec<crate::types::GraphFactsScanDiagnostic>,
    pub candidate_paths: Vec<String>,
    pub files_skipped: u32,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn file(path: impl AsRef<str>) -> Self {
        Self(format!("file:{}", normalize_path(path.as_ref())))
    }

    pub fn external(symbol: impl AsRef<str>) -> Self {
        Self(format!("external:{}", symbol.as_ref()))
    }

    /// File-local declaration identity supplied by the syntax producer. This is
    /// not a canonical cross-file binding or a stable identity across edits.
    pub fn symbol(file: &str, local_id: &str) -> Self {
        Self(format!("symbol:{}#{}", normalize_path(file), local_id))
    }

    pub fn occurrence(file: &str, local_id: &str) -> Self {
        Self(format!("occurrence:{}#{}", normalize_path(file), local_id))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(transparent)]
pub struct EvidenceId(pub String);

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    File,
    Symbol,
    Occurrence,
    Module,
    Package,
    ExternalSymbol,
    UnresolvedTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKind {
    Contains,
    Declares,
    Imports,
    Reexports,
    Calls,
    References,
    Defines,
    Implements,
    TypeDefinition,
    Extends,
    DynamicImport,
    Syntactic(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub display_name: String,
    pub file: Option<String>,
    pub range: Option<GraphRange>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerReceipt {
    pub family: String,
    pub version: Option<String>,
    pub configuration_digest: String,
    pub capabilities: BTreeSet<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EvidenceSource {
    Ast {
        extractor: String,
        relation: String,
    },
    Lsp {
        method: String,
        server_family: String,
        server_version: Option<String>,
        configuration_digest: String,
        capabilities: BTreeSet<String>,
        document_version: Option<i64>,
    },
    CargoMetadata,
    PackageMetadata,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub id: EvidenceId,
    pub source: EvidenceSource,
    pub file: Option<String>,
    pub range: Option<GraphRange>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeEdge {
    pub id: String,
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub evidence: BTreeSet<EvidenceId>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphCompleteness {
    pub scan_complete: bool,
    /// Whole-graph semantic completeness. Only set when every semantic
    /// candidate has been enriched by a complete provider scope. A single
    /// successful relation must never imply this flag.
    pub semantic_complete: bool,
    pub skipped_files: u32,
    pub reasons: BTreeSet<String>,
    /// Scope-aware finalization: names of candidate scopes (e.g. `deadCode`,
    /// `impact`) that were completed for their selected candidates. Distinct
    /// from `semantic_complete`, which is whole-graph.
    #[serde(default)]
    pub semantic_scopes_complete: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMetadata {
    pub root: String,
    pub facts_schema_version: u32,
    /// Source snapshot identity (root, facts schema, file content digests).
    /// Not a configured-project cache key: parser choices, manifests, provider
    /// configuration and semantic observations are outside this identity.
    pub generation: String,
    pub digest: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct CodeGraphDiagnostic {
    pub code: String,
    pub message: String,
    pub file: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeGraphSnapshot {
    pub schema_version: u32,
    pub snapshot: SnapshotMetadata,
    pub nodes: BTreeMap<NodeId, CodeNode>,
    pub edges: BTreeMap<String, CodeEdge>,
    pub evidence: BTreeMap<EvidenceId, Evidence>,
    /// Semantic observations, including negative evidence (zero-result or
    /// unavailable queries with provenance). Stored separately from graph
    /// edges so an absence claim is never encoded as a positive relation.
    #[serde(default)]
    pub observations: BTreeMap<String, SemanticObservation>,
    pub completeness: GraphCompleteness,
    pub diagnostics: Vec<CodeGraphDiagnostic>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphBuildMetrics {
    pub files: u64,
    pub nodes: u64,
    pub edges: u64,
    pub evidence: u64,
    pub ast_relations: u64,
    pub semantic_relations: u64,
    pub semantic_observations: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphBuildReceipt {
    pub snapshot_digest: String,
    pub metrics: GraphBuildMetrics,
}

/// The provider operation that produced a semantic observation.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum SemanticOperation {
    Definition,
    References,
    Callers,
    Callees,
    Implementations,
    TypeDefinition,
    Supertypes,
    Subtypes,
}

/// The classification a provider assigned to a semantic candidate. Negative
/// outcomes (`NoResult`, `Unresolved`, `Unavailable`, `Truncated`) never prove
/// absence on their own; only a `complete` provider scope can support that.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum SemanticOutcome {
    /// External semantic evidence corroborates the syntactic candidate.
    Corroborated,
    /// Semantic evidence contradicts the syntactic candidate.
    Contradicted,
    /// The provider completed and returned zero results.
    NoResult,
    /// Identity could not be resolved (ambiguous or not indexed).
    Unresolved,
    /// The capability was unavailable for this provider/configuration.
    Unavailable,
    /// Results were truncated by a budget or provider bound.
    Truncated,
}

/// A source location anchoring a semantic candidate to a definition site.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct SymbolAnchor {
    pub file: String,
    pub range: GraphRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// A recorded semantic observation with provenance, including negative
/// evidence. Stored separately from graph edges.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SemanticObservation {
    pub id: String,
    pub generation: String,
    pub candidate: NodeId,
    pub provider: ServerReceipt,
    pub operation: SemanticOperation,
    pub anchor: SymbolAnchor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_version: Option<i64>,
    pub outcome: SemanticOutcome,
    pub result_count: u64,
    /// Whether the provider scope for this query was complete. Required before
    /// any `NoResult` observation can support an absence claim downstream.
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation_reason: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CodeGraphBuilder {
    graph: CodeGraphSnapshot,
    metrics: GraphBuildMetrics,
}

impl CodeGraphBuilder {
    pub fn new(root: impl AsRef<str>, facts_schema_version: u32) -> Self {
        let root = normalize_path(root.as_ref());
        let generation = generation_digest(&root, facts_schema_version, &BTreeMap::new());
        Self {
            graph: CodeGraphSnapshot {
                schema_version: 1,
                snapshot: SnapshotMetadata {
                    root,
                    facts_schema_version,
                    generation,
                    ..Default::default()
                },
                completeness: GraphCompleteness {
                    scan_complete: true,
                    semantic_complete: false,
                    reasons: BTreeSet::from(["semantic-incomplete".to_owned()]),
                    ..Default::default()
                },
                ..Default::default()
            },
            metrics: GraphBuildMetrics::default(),
        }
    }

    /// Identifies source contents only. Consumers must separately validate the
    /// producer/configuration before reusing semantic evidence across builders.
    pub fn generation(&self) -> String {
        generation_digest(
            &self.graph.snapshot.root,
            self.graph.snapshot.facts_schema_version,
            &self.graph.snapshot.files,
        )
    }

    pub fn add_file(
        &mut self,
        path: impl AsRef<str>,
        digest: impl Into<String>,
    ) -> Result<NodeId, String> {
        let path = normalize_path(path.as_ref());
        self.graph
            .snapshot
            .files
            .insert(path.clone(), digest.into());
        let id = NodeId::file(&path);
        self.graph
            .nodes
            .entry(id.clone())
            .or_insert_with(|| CodeNode {
                id: id.clone(),
                kind: NodeKind::File,
                display_name: path.clone(),
                file: Some(path),
                range: None,
            });
        self.refresh_counts();
        Ok(id)
    }

    pub fn ingest_facts(
        &mut self,
        path: impl AsRef<str>,
        digest: impl Into<String>,
        facts: &GraphFactsDocument,
    ) -> Result<(), String> {
        let path = normalize_path(path.as_ref());
        let file_id = self.add_file(&path, digest)?;
        for declaration in &facts.declarations {
            let id = NodeId::symbol(&path, &declaration.id);
            self.graph.nodes.insert(
                id.clone(),
                CodeNode {
                    id: id.clone(),
                    kind: NodeKind::Symbol,
                    display_name: declaration.name.clone(),
                    file: Some(path.clone()),
                    range: Some(declaration.range.clone()),
                },
            );
            self.add_edge_with_evidence(
                file_id.clone(),
                id,
                EdgeKind::Declares,
                EvidenceSource::Ast {
                    extractor: facts.source.clone(),
                    relation: "declares".to_owned(),
                },
                Some(path.clone()),
                Some(declaration.range.clone()),
            )?;
        }
        for call in &facts.calls {
            let id = NodeId::occurrence(&path, &call.id);
            self.graph.nodes.insert(
                id.clone(),
                CodeNode {
                    id,
                    kind: NodeKind::Occurrence,
                    display_name: call.callee.clone(),
                    file: Some(path.clone()),
                    range: Some(call.range.clone()),
                },
            );
        }
        for edge in &facts.edges {
            let from = self.resolve_fact_node(&path, &edge.from);
            let to = self.resolve_fact_node(&path, &edge.to);
            self.add_edge_with_evidence(
                from,
                to,
                EdgeKind::Syntactic(edge.relation.clone()),
                EvidenceSource::Ast {
                    extractor: facts.source.clone(),
                    relation: edge.relation.clone(),
                },
                Some(path.clone()),
                None,
            )?;
        }
        for diagnostic in &facts.diagnostics {
            self.graph.diagnostics.push(CodeGraphDiagnostic {
                code: "ast.coverage".to_owned(),
                message: diagnostic.clone(),
                file: Some(path.clone()),
            });
        }
        self.refresh_counts();
        Ok(())
    }

    pub fn add_file_relation(
        &mut self,
        from: impl AsRef<str>,
        to: impl AsRef<str>,
        relation: impl AsRef<str>,
        line: u32,
    ) -> Result<(), String> {
        let from_path = normalize_path(from.as_ref());
        let to_path = normalize_path(to.as_ref());
        let from_id = self.ensure_file(&from_path);
        let to_id = self.ensure_file(&to_path);
        let relation = relation.as_ref().to_owned();
        let kind = match relation.as_str() {
            "dynamic-import" => EdgeKind::DynamicImport,
            value if value.contains("reexport") => EdgeKind::Reexports,
            _ => EdgeKind::Imports,
        };
        self.add_edge_with_evidence(
            from_id,
            to_id,
            kind,
            EvidenceSource::Ast {
                extractor: "native-linker".to_owned(),
                relation,
            },
            Some(from_path),
            Some(GraphRange {
                start: GraphPosition {
                    line: line.saturating_sub(1),
                    character: 0,
                },
                end: GraphPosition {
                    line: line.saturating_sub(1),
                    character: 0,
                },
            }),
        )?;
        Ok(())
    }

    pub fn mark_incomplete(&mut self, reason: impl Into<String>, skipped_files: u32) {
        self.graph.completeness.scan_complete = false;
        self.graph.completeness.skipped_files = skipped_files;
        self.graph.completeness.reasons.insert(reason.into());
    }

    pub fn finish(self) -> CodeGraphSnapshot {
        let mut graph = self.finish_without_digest();
        let encoded = serde_json::to_vec(&graph).unwrap_or_default();
        graph.snapshot.digest = sha256(&encoded);
        graph
    }

    /// The finished graph with an empty `snapshot.digest`: for callers that
    /// never read it, skipping the whole-graph serialization and hash (the
    /// dominant cost of finishing a large graph).
    pub fn finish_without_digest(mut self) -> CodeGraphSnapshot {
        self.refresh_generation();
        self.graph.diagnostics.sort();
        self.graph.diagnostics.dedup();
        self.refresh_counts();
        self.graph.snapshot.digest.clear();
        self.graph
    }

    pub fn finish_with_receipt(mut self) -> (CodeGraphSnapshot, GraphBuildReceipt) {
        self.refresh_counts();
        self.refresh_relation_counts();
        let metrics = self.metrics.clone();
        let graph = self.finish();
        let receipt = GraphBuildReceipt {
            snapshot_digest: graph.snapshot.digest.clone(),
            metrics,
        };
        (graph, receipt)
    }

    fn refresh_generation(&mut self) {
        self.graph.snapshot.generation = generation_digest(
            &self.graph.snapshot.root,
            self.graph.snapshot.facts_schema_version,
            &self.graph.snapshot.files,
        );
    }

    fn ensure_file(&mut self, path: &str) -> NodeId {
        let id = NodeId::file(path);
        self.graph
            .nodes
            .entry(id.clone())
            .or_insert_with(|| CodeNode {
                id: id.clone(),
                kind: NodeKind::File,
                display_name: path.to_owned(),
                file: Some(path.to_owned()),
                range: None,
            });
        id
    }

    fn ensure_node(&mut self, id: NodeId) {
        self.graph.nodes.entry(id.clone()).or_insert_with(|| {
            let kind = if id.0.starts_with("file:") {
                NodeKind::File
            } else if id.0.starts_with("external:") {
                NodeKind::ExternalSymbol
            } else {
                NodeKind::UnresolvedTarget
            };
            CodeNode {
                display_name: id.0.clone(),
                id,
                kind,
                file: None,
                range: None,
            }
        });
    }

    fn resolve_fact_node(&mut self, file: &str, local_id: &str) -> NodeId {
        let symbol = NodeId::symbol(file, local_id);
        if self.graph.nodes.contains_key(&symbol) {
            return symbol;
        }
        let occurrence = NodeId::occurrence(file, local_id);
        if self.graph.nodes.contains_key(&occurrence) {
            return occurrence;
        }
        self.ensure_node(occurrence.clone());
        occurrence
    }

    fn add_edge_with_evidence(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
        source: EvidenceSource,
        file: Option<String>,
        range: Option<GraphRange>,
    ) -> Result<(), String> {
        // Borrow every tuple element (serde serializes `&T` identically to `T`),
        // so the evidence digest is byte-identical while avoiding a full
        // `EvidenceSource` clone per edge — `source` is moved into `Evidence` below.
        let evidence_key = serde_json::to_vec(&(&from, &to, &kind, &source, &file, &range))
            .map_err(|error| error.to_string())?;
        let evidence_id = EvidenceId(sha256(&evidence_key));
        self.graph
            .evidence
            .entry(evidence_id.clone())
            .or_insert(Evidence {
                id: evidence_id.clone(),
                source,
                file,
                range,
            });
        let edge_key =
            serde_json::to_vec(&(&from, &to, &kind)).map_err(|error| error.to_string())?;
        let edge_id = sha256(&edge_key);
        self.graph
            .edges
            .entry(edge_id.clone())
            .and_modify(|edge| {
                edge.evidence.insert(evidence_id.clone());
            })
            .or_insert_with(|| CodeEdge {
                id: edge_id,
                from,
                to,
                kind,
                evidence: BTreeSet::from([evidence_id]),
            });
        self.refresh_counts();
        Ok(())
    }

    fn refresh_counts(&mut self) {
        self.metrics.files = self.graph.snapshot.files.len() as u64;
        self.metrics.nodes = self.graph.nodes.len() as u64;
        self.metrics.edges = self.graph.edges.len() as u64;
        self.metrics.evidence = self.graph.evidence.len() as u64;
    }

    fn refresh_relation_counts(&mut self) {
        self.metrics.ast_relations = self
            .graph
            .evidence
            .values()
            .filter(|evidence| matches!(&evidence.source, EvidenceSource::Ast { .. }))
            .count() as u64;
        self.metrics.semantic_observations = self.graph.observations.len() as u64;
        self.metrics.semantic_relations = self
            .graph
            .evidence
            .values()
            .filter(|evidence| matches!(&evidence.source, EvidenceSource::Lsp { .. }))
            .count() as u64;
    }
}

fn generation_digest(root: &str, schema: u32, files: &BTreeMap<String, String>) -> String {
    let encoded = serde_json::to_vec(&(root, schema, files)).unwrap_or_default();
    sha256(&encoded)
}

fn normalize_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.ends_with('/')
        && normalized.len() > 1
        && normalized
            .as_bytes()
            .get(normalized.len() - 2)
            .is_none_or(|byte| *byte != b':')
    {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_normalization_preserves_filesystem_roots() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("C:\\"), "C:/");
        assert_eq!(normalize_path("/workspace/"), "/workspace");
    }

    #[test]
    fn typed_facts_preserve_ranges_modules_edges_and_commonjs() {
        let json = r#"{
          "kind":"graphFacts","schemaVersion":1,"source":"native-ast","language":"rust","file":"lib.rs",
          "declarations":[{"id":"d","name":"run","kind":"function","line":1,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":8}},"selectionRange":{"start":{"line":0,"character":3},"end":{"line":0,"character":6}},"exported":true}],
          "edges":[{"id":"e","from":"d","to":"c","relation":"calls","source":"native-ast","line":1,"resolution":"unresolved"}],
          "calls":[{"id":"c","caller":"d","callee":"work","line":1,"range":{"start":{"line":0,"character":7},"end":{"line":0,"character":11}},"kind":"direct"}],
          "modules":[{"name":"child","line":2,"scope":[],"inline":false,"path":"child.rs","unsupported":false}],
          "commonJs":[{"specifier":"pkg","line":3,"kind":"commonjs-require","binding":"require"}]
        }"#;
        let facts: GraphFactsDocument = serde_json::from_str(json).expect("typed facts");
        assert_eq!(facts.declarations[0].selection_range.start.character, 3);
        assert_eq!(facts.edges[0].relation, "calls");
        assert_eq!(facts.modules[0].path.as_deref(), Some("child.rs"));
        assert_eq!(facts.common_js[0].specifier.as_deref(), Some("pkg"));
        assert_eq!(facts.common_js[0].kind, "commonjs-require");
    }
}
