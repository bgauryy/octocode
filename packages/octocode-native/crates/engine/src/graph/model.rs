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
    /// 1-based `[start, end]` line spans of the syntax errors a recovered
    /// parse skipped (merged); empty for a clean parse.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub error_lines: Vec<[u32; 2]>,
    pub modules: Vec<GraphFactRustModule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_root_unsupported: Option<bool>,
}

/// A source file's size and change times when its content was hashed. A
/// later read compares these before hashing again; equal stamps prove the
/// bytes are unchanged without reading them.
///
/// Git's stat cache carries the same race: a file rewritten within the clock
/// tick of the read keeps its time. [`SourceStamp::settled`] therefore
/// records a stamp only for a file untouched for [`SourceStamp::SETTLE`], so
/// any later write moves its time past the stored one. On Unix the status
/// change time and inode also change on a write that restores the old
/// modification time, or on a replace by rename.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceStamp {
    pub size: u64,
    /// Modification time, nanoseconds since the Unix epoch.
    pub modified_ns: u64,
    /// Status change time in nanoseconds (Unix), else 0.
    pub changed_ns: u64,
    /// Inode number (Unix), else 0.
    pub inode: u64,
}

impl SourceStamp {
    /// How long a file must be untouched before its stamp can stand in for
    /// its content: two clock ticks of the coarsest common file time (FAT).
    pub const SETTLE: std::time::Duration = std::time::Duration::from_secs(2);

    /// The stamp of `meta`, or `None` when the platform reports no
    /// modification time after the epoch.
    #[must_use]
    pub fn of(meta: &std::fs::Metadata) -> Option<Self> {
        let nanos = |time: std::time::SystemTime| {
            time.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok())
        };
        let modified_ns = nanos(meta.modified().ok()?)?;
        #[cfg(unix)]
        let (changed_ns, inode) = {
            use std::os::unix::fs::MetadataExt as _;
            let changed = u64::try_from(meta.ctime())
                .ok()
                .zip(u64::try_from(meta.ctime_nsec()).ok())
                .and_then(|(secs, nanos)| secs.checked_mul(1_000_000_000)?.checked_add(nanos));
            (changed?, meta.ino())
        };
        #[cfg(not(unix))]
        let (changed_ns, inode) = (0, 0);
        Some(Self {
            size: meta.len(),
            modified_ns,
            changed_ns,
            inode,
        })
    }

    /// [`SourceStamp::of`] when the file was last written at least
    /// [`SourceStamp::SETTLE`] before `now`; `None` keeps the content hash as
    /// the only proof (a racily clean or future-dated file).
    #[must_use]
    pub fn settled(meta: &std::fs::Metadata, now: std::time::SystemTime) -> Option<Self> {
        let stamp = Self::of(meta)?;
        let now_ns = now
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok())?;
        let settle = u64::try_from(Self::SETTLE.as_nanos()).unwrap_or(u64::MAX);
        let last_write = stamp.modified_ns.max(stamp.changed_ns);
        (last_write.saturating_add(settle) <= now_ns).then_some(stamp)
    }
}

#[derive(Clone, Debug)]
pub struct GraphFactsTypedEntry {
    pub relative_path: String,
    pub content_digest: String,
    /// The stamp of the bytes `content_digest` hashed, when settled.
    pub source_stamp: Option<SourceStamp>,
    pub facts: GraphFactsDocument,
    pub reference_counts: Vec<crate::types::GraphReferenceCount>,
}

#[derive(Clone, Debug)]
pub struct GraphFactsTypedScanResult {
    pub schema_version: u32,
    pub entries: Vec<GraphFactsTypedEntry>,
    pub skipped: Vec<crate::types::GraphFactsScanDiagnostic>,
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
    UnresolvedTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKind {
    Declares,
    Imports,
    Reexports,
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

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EvidenceSource {
    Ast { extractor: String, relation: String },
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
    pub reasons: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMetadata {
    pub root: String,
    pub facts_schema_version: u32,
    /// Relative path → content digest of every scanned file.
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodeGraphSnapshot {
    pub snapshot: SnapshotMetadata,
    pub nodes: BTreeMap<NodeId, CodeNode>,
    pub edges: BTreeMap<String, CodeEdge>,
    pub evidence: BTreeMap<EvidenceId, Evidence>,
    pub completeness: GraphCompleteness,
}

#[derive(Clone, Debug, Default)]
pub struct CodeGraphBuilder {
    graph: CodeGraphSnapshot,
}

impl CodeGraphBuilder {
    pub fn new(root: impl AsRef<str>, facts_schema_version: u32) -> Self {
        Self {
            graph: CodeGraphSnapshot {
                snapshot: SnapshotMetadata {
                    root: normalize_path(root.as_ref()),
                    facts_schema_version,
                    ..Default::default()
                },
                completeness: GraphCompleteness {
                    scan_complete: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        }
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

    pub fn mark_incomplete(&mut self, reason: impl Into<String>) {
        self.graph.completeness.scan_complete = false;
        self.graph.completeness.reasons.insert(reason.into());
    }

    pub fn finish(self) -> CodeGraphSnapshot {
        self.graph
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

    fn resolve_fact_node(&mut self, file: &str, local_id: &str) -> NodeId {
        let symbol = NodeId::symbol(file, local_id);
        if self.graph.nodes.contains_key(&symbol) {
            return symbol;
        }
        let occurrence = NodeId::occurrence(file, local_id);
        self.graph
            .nodes
            .entry(occurrence.clone())
            .or_insert_with(|| CodeNode {
                display_name: occurrence.0.clone(),
                id: occurrence.clone(),
                kind: NodeKind::UnresolvedTarget,
                file: None,
                range: None,
            });
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
        Ok(())
    }
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
