use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::index::content_digest;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
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
}

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
    pub callee: String,
    pub line: u32,
    pub range: GraphRange,
    pub kind: String,
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

impl GraphFactsDocument {
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }
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
    pub semantic_complete: bool,
    pub skipped_files: u32,
    pub reasons: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMetadata {
    pub root: String,
    pub facts_schema_version: u32,
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
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphBuildReceipt {
    pub snapshot_digest: String,
    pub metrics: GraphBuildMetrics,
}

#[derive(Clone, Debug)]
pub struct SemanticRelationInput {
    pub generation: String,
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub method: String,
    pub server: ServerReceipt,
    pub document_version: Option<i64>,
    pub range: Option<GraphRange>,
}

#[derive(Clone, Debug, Default)]
pub struct CodeGraphBuilder {
    graph: CodeGraphSnapshot,
    metrics: GraphBuildMetrics,
    semantic_started: bool,
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
            semantic_started: false,
        }
    }

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
        if self.semantic_started {
            return Err("source files cannot change after semantic enrichment begins".to_owned());
        }
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
        if self.semantic_started {
            return Err(
                "syntax relations cannot change after semantic enrichment begins".to_owned(),
            );
        }
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

    pub fn add_semantic_relation(&mut self, input: SemanticRelationInput) -> Result<(), String> {
        let generation = self.generation();
        if input.generation != generation {
            return Err("semantic evidence generation does not match graph snapshot".to_owned());
        }
        self.graph.snapshot.generation = generation;
        self.semantic_started = true;
        self.ensure_node(input.from.clone());
        self.ensure_node(input.to.clone());
        let source = EvidenceSource::Lsp {
            method: input.method,
            server_family: input.server.family,
            server_version: input.server.version,
            configuration_digest: input.server.configuration_digest,
            capabilities: input.server.capabilities,
            document_version: input.document_version,
        };
        self.add_edge_with_evidence(input.from, input.to, input.kind, source, None, input.range)?;
        Ok(())
    }

    pub fn mark_semantic_complete(&mut self) {
        self.graph.completeness.semantic_complete = true;
        self.graph
            .completeness
            .reasons
            .remove("semantic-incomplete");
    }

    pub fn mark_semantic_incomplete(&mut self, reason: impl Into<String>) {
        self.graph.completeness.semantic_complete = false;
        self.graph.completeness.reasons.insert(reason.into());
    }

    pub fn mark_incomplete(&mut self, reason: impl Into<String>, skipped_files: u32) {
        self.graph.completeness.scan_complete = false;
        self.graph.completeness.skipped_files = skipped_files;
        self.graph.completeness.reasons.insert(reason.into());
    }

    pub fn finish(mut self) -> CodeGraphSnapshot {
        self.refresh_generation();
        self.graph.diagnostics.sort();
        self.graph.diagnostics.dedup();
        self.refresh_counts();
        self.graph.snapshot.digest.clear();
        let encoded = serde_json::to_vec(&self.graph).unwrap_or_default();
        self.graph.snapshot.digest = content_digest(&encoded);
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
        let evidence_key = serde_json::to_vec(&(&from, &to, &kind, source.clone(), &file, &range))
            .map_err(|error| error.to_string())?;
        let evidence_id = EvidenceId(content_digest(&evidence_key));
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
        let edge_id = content_digest(&edge_key);
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
    content_digest(&encoded)
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

    fn semantic(generation: String) -> SemanticRelationInput {
        SemanticRelationInput {
            generation,
            from: NodeId::file("src/main.rs"),
            to: NodeId::file("src/lib.rs"),
            kind: EdgeKind::Imports,
            method: "textDocument/definition".to_owned(),
            server: ServerReceipt {
                family: "rust-analyzer".to_owned(),
                version: Some("test".to_owned()),
                configuration_digest: "config".to_owned(),
                capabilities: ["referencesProvider".to_owned()].into_iter().collect(),
            },
            document_version: Some(7),
            range: None,
        }
    }

    #[test]
    fn path_normalization_preserves_filesystem_roots() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("C:\\"), "C:/");
        assert_eq!(normalize_path("/workspace/"), "/workspace");
    }

    #[test]
    fn graph_snapshot_merges_independent_ast_and_lsp_evidence_deterministically() {
        let mut builder = CodeGraphBuilder::new("/workspace", 1);
        builder.add_file("src/main.rs", "aaa").expect("main file");
        builder.add_file("src/lib.rs", "bbb").expect("lib file");
        builder
            .add_file_relation("src/main.rs", "src/lib.rs", "rust-use", 1)
            .expect("AST relation");
        builder
            .add_semantic_relation(semantic(builder.generation()))
            .expect("LSP relation");
        let first = builder.finish();

        let mut rebuilt = CodeGraphBuilder::new("/workspace", 1);
        rebuilt.add_file("src/lib.rs", "bbb").expect("lib file");
        rebuilt.add_file("src/main.rs", "aaa").expect("main file");
        rebuilt
            .add_file_relation("src/main.rs", "src/lib.rs", "rust-use", 1)
            .expect("AST relation");
        rebuilt
            .add_semantic_relation(semantic(rebuilt.generation()))
            .expect("LSP relation");
        let second = rebuilt.finish();

        assert_eq!(first.snapshot.digest, second.snapshot.digest);
        assert_eq!(first.evidence.len(), 2);
        assert_eq!(first.edges.len(), 1);
        assert_eq!(first.edges.values().next().expect("edge").evidence.len(), 2);
        let ast_evidence = first
            .evidence
            .values()
            .find(|evidence| matches!(evidence.source, EvidenceSource::Ast { .. }))
            .expect("AST evidence");
        assert_eq!(ast_evidence.range.as_ref().expect("range").start.line, 0);
    }

    #[test]
    fn contradictory_semantic_evidence_remains_separate_and_inspectable() {
        let mut builder = CodeGraphBuilder::new("/workspace", 1);
        builder.add_file("src/main.rs", "aaa").expect("main file");
        builder.add_file("src/lib.rs", "bbb").expect("lib file");
        builder
            .add_file_relation("src/main.rs", "src/lib.rs", "rust-use", 1)
            .expect("syntax relation");
        let mut relation = semantic(builder.generation());
        relation.to = NodeId::external("crate:other");
        builder
            .add_semantic_relation(relation)
            .expect("semantic relation");
        builder.mark_semantic_incomplete("selective-budget");
        let snapshot = builder.finish();

        assert_eq!(snapshot.edges.len(), 2);
        assert_eq!(snapshot.evidence.len(), 2);
        assert!(!snapshot.completeness.semantic_complete);
        assert!(snapshot.completeness.reasons.contains("selective-budget"));
    }

    #[test]
    fn semantic_unavailability_is_explicit_in_snapshot_completeness() {
        let mut builder = CodeGraphBuilder::new("/workspace", 1);
        builder.add_file("src/main.rs", "aaa").expect("main file");
        builder.mark_semantic_incomplete("definitionProvider unavailable");
        let snapshot = builder.finish();

        assert!(!snapshot.completeness.semantic_complete);
        assert!(snapshot
            .completeness
            .reasons
            .contains("definitionProvider unavailable"));
    }

    #[test]
    fn stale_semantic_generation_is_rejected() {
        let mut builder = CodeGraphBuilder::new("/workspace", 1);
        builder.add_file("src/main.rs", "aaa").expect("main file");
        let mut relation = semantic("stale".to_owned());
        relation.to = NodeId::external("crate:item");
        let error = builder
            .add_semantic_relation(relation)
            .expect_err("stale evidence must fail");
        assert!(error.contains("generation"));
    }

    #[test]
    fn source_mutation_is_rejected_after_semantic_enrichment_begins() {
        let mut builder = CodeGraphBuilder::new("/workspace", 1);
        builder.add_file("src/main.rs", "aaa").expect("main file");
        builder.add_file("src/lib.rs", "bbb").expect("lib file");
        builder
            .add_semantic_relation(semantic(builder.generation()))
            .expect("semantic relation");
        assert!(builder
            .add_file("src/late.rs", "ccc")
            .expect_err("late source mutation")
            .contains("after semantic enrichment"));
        assert!(builder
            .add_file_relation("src/main.rs", "src/late.rs", "rust-use", 1)
            .expect_err("late syntax mutation")
            .contains("after semantic enrichment"));
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
        let facts = GraphFactsDocument::from_json(json).expect("typed facts");
        assert_eq!(facts.declarations[0].selection_range.start.character, 3);
        assert_eq!(facts.edges[0].relation, "calls");
        assert_eq!(facts.modules[0].path.as_deref(), Some("child.rs"));
        assert_eq!(facts.common_js[0].specifier.as_deref(), Some("pkg"));
        assert_eq!(facts.common_js[0].kind, "commonjs-require");
    }
}
