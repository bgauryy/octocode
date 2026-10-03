use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub use crate::contracts::tool_types::{AstTopologyQuery, AstTopologyQueryRustWorkspace};

/// The `analysis` discriminant of an [`AstTopologyQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// Binds `$field` from whichever analysis variant `$query` is.
macro_rules! every_analysis {
    ($query:expr, $field:ident => $value:expr) => {
        match $query {
            AstTopologyQuery::DeadCode { $field, .. }
            | AstTopologyQuery::Cycles { $field, .. }
            | AstTopologyQuery::Dependencies { $field, .. }
            | AstTopologyQuery::Dependents { $field, .. }
            | AstTopologyQuery::Path { $field, .. }
            | AstTopologyQuery::Reachability { $field, .. }
            | AstTopologyQuery::Drift { $field, .. } => $value,
        }
    };
}

fn u32_of(value: std::num::NonZeroU64) -> u32 {
    u32::try_from(value.get()).unwrap_or(u32::MAX)
}

/// Analysis-independent views over the generated wire query, in the graph
/// engine's `u32` units.
impl AstTopologyQuery {
    pub fn analysis(&self) -> GraphAnalysis {
        match self {
            Self::DeadCode { .. } => GraphAnalysis::DeadCode,
            Self::Cycles { .. } => GraphAnalysis::Cycles,
            Self::Dependencies { .. } => GraphAnalysis::Dependencies,
            Self::Dependents { .. } => GraphAnalysis::Dependents,
            Self::Path { .. } => GraphAnalysis::Path,
            Self::Reachability { .. } => GraphAnalysis::Reachability,
            Self::Drift { .. } => GraphAnalysis::Drift,
        }
    }
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Cycles { path, .. } | Self::Drift { path, .. } => Some(path),
            Self::DeadCode { path, .. }
            | Self::Dependencies { path, .. }
            | Self::Dependents { path, .. }
            | Self::Path { path, .. }
            | Self::Reachability { path, .. } => path.as_deref(),
        }
    }
    /// Points the scan at another root (the drift baseline snapshot).
    pub fn set_path(&mut self, root: String) {
        match self {
            Self::Cycles { path, .. } | Self::Drift { path, .. } => *path = root,
            Self::DeadCode { path, .. }
            | Self::Dependencies { path, .. }
            | Self::Dependents { path, .. }
            | Self::Path { path, .. }
            | Self::Reachability { path, .. } => *path = Some(root),
        }
    }
    pub fn file(&self) -> Option<&str> {
        match self {
            Self::Dependencies { file, .. }
            | Self::Dependents { file, .. }
            | Self::Path { file, .. } => Some(file),
            _ => None,
        }
    }
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Path { target, .. } => Some(target),
            _ => None,
        }
    }
    pub fn baseline(&self) -> Option<&str> {
        match self {
            Self::Drift { baseline, .. } => Some(baseline),
            _ => None,
        }
    }
    pub fn depth(&self) -> Option<u32> {
        match self {
            Self::Dependencies { depth, .. } | Self::Dependents { depth, .. } => {
                Some(u32_of(*depth))
            }
            _ => None,
        }
    }
    pub fn entrypoints(&self) -> Option<&Vec<String>> {
        match self {
            Self::DeadCode { entrypoints, .. } | Self::Reachability { entrypoints, .. } => {
                Some(entrypoints)
            }
            _ => None,
        }
    }
    pub fn include_tests(&self) -> Option<bool> {
        match self {
            Self::DeadCode { include_tests, .. } | Self::Reachability { include_tests, .. } => {
                Some(*include_tests)
            }
            _ => None,
        }
    }
    pub fn default_excludes(&self) -> bool {
        use crate::policy::prune::DefaultsFlag;
        every_analysis!(self, default_excludes => default_excludes.defaults())
    }
    pub fn exclude_dir(&self) -> Option<&[String]> {
        every_analysis!(self, exclude_dir => (!exclude_dir.is_empty()).then_some(exclude_dir.as_slice()))
    }
    /// Language globs in a deterministic (sorted) order.
    pub fn language_globs(&self) -> Option<BTreeMap<String, Vec<String>>> {
        every_analysis!(self, language_globs => (!language_globs.is_empty()).then(|| {
            language_globs
                .iter()
                .map(|(language, globs)| (language.clone(), globs.clone()))
                .collect()
        }))
    }
    pub fn max_files(&self) -> Option<u32> {
        every_analysis!(self, max_files => max_files.map(u32_of))
    }
    pub fn limit(&self) -> Option<u32> {
        every_analysis!(self, limit => limit.map(u32_of))
    }
    pub fn page(&self) -> u32 {
        every_analysis!(self, page => u32_of(*page))
    }
    pub fn page_size(&self) -> u32 {
        every_analysis!(self, page_size => u32_of(*page_size))
    }
    pub fn diagnostic_page(&self) -> u32 {
        every_analysis!(self, diagnostic_page => diagnostic_page.map_or(1, u32_of))
    }
    /// Coverage diagnostic rows are returned only when the caller asks for a
    /// diagnostic page (`next.nextDiagnostics`); the default carries counts.
    pub fn diagnostic_rows_requested(&self) -> bool {
        every_analysis!(self, diagnostic_page => diagnostic_page.is_some())
    }
    pub fn diagnostic_page_size(&self) -> u32 {
        every_analysis!(self, diagnostic_page_size => u32_of(*diagnostic_page_size))
    }
    pub fn diagnostic_snapshot(&self) -> Option<&str> {
        every_analysis!(self, diagnostic_snapshot => diagnostic_snapshot.as_deref().map(String::as_str))
    }
    pub fn rust_workspace(&self) -> Option<AstTopologyQueryRustWorkspace> {
        every_analysis!(self, rust_workspace => *rust_workspace)
    }
}

pub(crate) type RawFacts = octocode_engine::graph::GraphFactsDocument;

#[derive(Clone, Debug)]
pub(crate) struct Declaration {
    /// Stable occurrence id from the facts (file + name + position + kind).
    pub id: String,
    pub name: String,
    pub kind: String,
    pub line: u32,
    /// Last line of the declaration body, in `line`'s numbering.
    pub end_line: u32,
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
    /// Binding name inside the importing file (`import { a as b }` → `b`).
    pub local_name: Option<String>,
    pub specifier: String,
    pub line: u32,
    pub target: Option<String>,
    /// Unlinked because it names a third-party/standard package, not because
    /// an internal path failed to resolve.
    pub external: bool,
    /// Declaration ids (or `IMPORT_USE_MODULE`) whose code uses the binding;
    /// `None` when the producer could not tell, so the import counts as used.
    pub used_in: Option<Vec<String>>,
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
    pub line: u32,
    /// Engine call kind (`call`, `new`, `renders` for JSX, `decorates`, …).
    pub kind: String,
    /// Syntactic receiver type of a member call (`Store` for `s.save()`),
    /// from the engine fact; `None` when the parser could not read it.
    pub receiver_type: Option<String>,
    /// File that the module prefix of a Rust qualified callee names
    /// (`crate::portable::sanitize` → `src/portable.rs`); `None` when the
    /// callee is unqualified or the prefix did not resolve.
    pub target: Option<String>,
}
/// A declaration's base type (`class A extends Base`, `impl Display for A`),
/// as written at the declaration site.
#[derive(Clone, Debug)]
pub(crate) struct Heritage {
    /// Declaration id of the derived type.
    pub decl_id: String,
    /// `extends` or `implements`.
    pub relation: String,
    /// Base type name as written (`Base`, `ns.Base`, `std::fmt::Display`).
    pub target: String,
    pub line: u32,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct FileFacts {
    pub declarations: Vec<Declaration>,
    pub imports: Vec<Import>,
    pub reexports: Vec<Reexport>,
    /// Line of each linked re-export statement (named or `*`), by target.
    pub reexport_lines: Vec<(String, u32)>,
    pub calls: Vec<Call>,
    /// Base-type relations of this file's declarations.
    pub heritage: Vec<Heritage>,
    /// Value-reference count per declaration id (declaration names, export
    /// clauses and call targets excluded). A missing id was not counted.
    pub reference_counts: BTreeMap<String, u32>,
    /// How the counts were produced: `semantic-references` (scope-resolved
    /// JS/TS symbols) or `syntax-references` (identifier tokens by name).
    pub reference_basis: &'static str,
    pub language: String,
    /// Source content digest (`octocode_engine::index::content_digest`).
    pub digest: String,
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
