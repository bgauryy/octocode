use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum GraphAnalysis {
    Dependencies,
    Dependents,
    Path,
    Cycles,
    Reachability,
    DeadCode,
    Drift,
}

impl GraphAnalysis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dependencies => "dependencies",
            Self::Dependents => "dependents",
            Self::Path => "path",
            Self::Cycles => "cycles",
            Self::Reachability => "reachability",
            Self::DeadCode => "deadCode",
            Self::Drift => "drift",
        }
    }
}

fn topology() -> String {
    "topology".into()
}
fn one() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AstGraphQuery {
    #[serde(default = "topology")]
    pub operation: String,
    pub analysis: GraphAnalysis,
    pub path: Option<String>,
    pub file: Option<String>,
    pub target: Option<String>,
    pub depth: Option<u32>,
    pub entrypoints: Option<Vec<String>>,
    pub include_tests: Option<bool>,
    pub exclude_dir: Option<Vec<String>>,
    pub language_globs: Option<BTreeMap<String, Vec<String>>>,
    pub max_files: Option<u32>,
    pub limit: Option<u32>,
    #[serde(default = "one")]
    pub page: u32,
    pub page_size: Option<u32>,
    #[serde(default = "one")]
    pub diagnostic_page: u32,
    pub diagnostic_page_size: Option<u32>,
    pub diagnostic_snapshot: Option<String>,
    pub rust_workspace: Option<String>,
    /// Baseline repository/package root for `analysis:"drift"`; the current
    /// `path` is the head. Only valid for the drift analysis.
    pub baseline: Option<String>,
}

pub(crate) type RawFacts = octocode_engine::graph::GraphFactsDocument;

#[derive(Clone, Debug)]
pub(crate) struct Declaration {
    /// Stable occurrence id from the facts (file + name + position + kind).
    pub id: String,
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub exported: bool,
    /// Public names when they differ from `name`; empty means `[name]`.
    pub exported_as: Vec<String>,
    /// Id of the containing declaration (class for a method).
    pub parent: Option<String>,
}

impl Declaration {
    /// Names this declaration is importable under from its module.
    pub fn public_names(&self) -> &[String] {
        if self.exported_as.is_empty() {
            std::slice::from_ref(&self.name)
        } else {
            &self.exported_as
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Import {
    pub imported_name: String,
    pub line: u32,
    pub target: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Reexport {
    pub local_name: String,
    pub imported_name: String,
    pub target: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Call {
    /// Declaration id of the caller; `None` for module-level code.
    pub caller_id: Option<String>,
    pub callee: String,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct FileFacts {
    pub declarations: Vec<Declaration>,
    pub imports: Vec<Import>,
    pub reexports: Vec<Reexport>,
    pub calls: Vec<Call>,
    /// Value-reference count per declaration id (declaration names, export
    /// clauses and call targets excluded). A missing id was not counted.
    pub reference_counts: BTreeMap<String, u32>,
    /// How the counts were produced: `semantic-references` (scope-resolved
    /// JS/TS symbols) or `syntax-references` (identifier tokens by name).
    pub reference_basis: &'static str,
}
pub(crate) type Node = octocode_engine::graph::FileGraphNode;

#[derive(Clone, Debug, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Diagnostic {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BuiltGraph {
    pub root: std::path::PathBuf,
    pub display_path: String,
    pub facts: BTreeMap<String, FileFacts>,
    pub nodes: BTreeMap<String, Node>,
    pub code_graph: octocode_engine::graph::CodeGraphSnapshot,
    pub files_skipped: u32,
    pub truncated: bool,
    /// Total file-graph edges accepted so far; `add_edge` stops collecting at
    /// the edge cap and flips `edges_capped` + `truncated` once.
    pub edge_count: u32,
    pub edges_capped: bool,
    pub languages: Vec<(String, u32, String)>,
    /// Import tallies: `[resolved, external, unresolvedInternal, unsupported,
    /// nonCode]`; `nonCode` counts relative JSON/style/asset specifiers, which
    /// are not graph edges and are not linking gaps.
    pub imports: [u32; 5],
    pub diagnostics: Vec<Diagnostic>,
    pub namespace_targets: BTreeSet<String>,
    pub star_reexporters: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AstGraphError {
    pub code: String,
    pub message: String,
    pub hints: Vec<String>,
    pub next: Option<Box<serde_json::Value>>,
}
impl AstGraphError {
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hints: vec![],
            next: None,
        }
    }
}
impl std::fmt::Display for AstGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AstGraphError {}

pub type AstGraphResult = Result<serde_json::Value, AstGraphError>;
