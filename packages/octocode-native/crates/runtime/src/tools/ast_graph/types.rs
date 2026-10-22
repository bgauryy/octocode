use std::collections::{BTreeMap, BTreeSet};

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
    pub language: String,
    /// Source content digest (`octocode_engine::digest::sha256`).
    pub digest: String,
    /// Size and change times of the bytes `digest` hashed, when settled.
    pub stamp: Option<octocode_engine::stamp::SourceStamp>,
    /// What the scanned source says about the file (roles, entry markers).
    pub marks: super::store::classify::SourceMarks,
}
pub(crate) type Node = octocode_engine::graph::FileGraphNode;

/// What a build diagnostic reports. Variants are in the order of their
/// codes, so diagnostics sort as their stored text does.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum DiagnosticCode {
    FactsSchemaUnsupported,
    GraphEdgeCap,
    ParseRecovery,
    ScanSkip,
    /// A bare import names a scanned file by path, but nothing links it.
    UnlinkedPathImport,
    UnresolvedInternal,
    UnsupportedLinking,
}

impl DiagnosticCode {
    /// The code a snapshot stores and `graph query diagnostics` prints.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FactsSchemaUnsupported => "facts-schema-unsupported",
            Self::GraphEdgeCap => "graph-edge-cap",
            Self::ParseRecovery => "parse-recovery",
            Self::ScanSkip => "scan-skip",
            Self::UnlinkedPathImport => "unlinked-path-import",
            Self::UnresolvedInternal => "unresolved-internal",
            Self::UnsupportedLinking => "unsupported-linking",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Diagnostic {
    pub file: String,
    pub line: Option<u32>,
    pub code: DiagnosticCode,
    pub message: String,
    /// The scanned file the diagnostic is about, when that is not `file`
    /// (the file an unlinked path import names).
    pub target: Option<String>,
}

impl Diagnostic {
    pub(crate) fn new(
        file: impl Into<String>,
        line: Option<u32>,
        code: DiagnosticCode,
        message: String,
    ) -> Self {
        Self {
            file: file.into(),
            line,
            code,
            message,
            target: None,
        }
    }
}

/// How one import statement linked: the index of its tally in
/// [`BuiltGraph::imports`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImportOutcome {
    Resolved,
    External,
    UnresolvedInternal,
    Unsupported,
    /// A relative JSON/style/asset specifier: no graph edge, no linking gap.
    NonCode,
}

/// The linked graph of one scan: per-file facts and the file graph. The
/// build's intermediate model, never stored or queried itself: ingest
/// projects it into the persisted [`super::store::tables`].
#[derive(Clone, Debug, Default)]
pub(crate) struct BuiltGraph {
    pub root: std::path::PathBuf,
    pub facts: BTreeMap<String, FileFacts>,
    pub nodes: BTreeMap<String, Node>,
    pub files_skipped: u32,
    /// The file scan stopped at `maxFiles`.
    pub truncated: bool,
    /// Total file-graph edges accepted so far; `add_edge` stops collecting at
    /// the edge cap and sets `edges_capped` once (more files cannot lift it).
    pub edge_count: u32,
    pub edges_capped: bool,
    pub languages: Vec<(String, u32, String)>,
    /// Import tallies indexed by [`ImportOutcome`].
    pub imports: [u32; 5],
    /// Statements already tallied in `imports` for the file named by
    /// `counted_file`, so a statement that binds several names counts once.
    pub(super) counted_file: String,
    pub(super) counted: BTreeSet<(usize, u32, String)>,
    pub diagnostics: Vec<Diagnostic>,
    pub namespace_targets: BTreeSet<String>,
    pub star_reexporters: BTreeMap<String, Vec<String>>,
}

impl BuiltGraph {
    /// Tallies one import statement under `outcome`.
    /// The unit is the statement, like the diagnostics: a statement binding
    /// three names adds one, and one with several outcomes adds one per
    /// outcome. Files are visited one at a time.
    pub(super) fn count_import(
        &mut self,
        outcome: ImportOutcome,
        file: &str,
        line: u32,
        spec: &str,
    ) {
        let outcome = outcome as usize;
        if self.counted_file != file {
            self.counted_file = file.to_owned();
            self.counted.clear();
        }
        if self.counted.insert((outcome, line, spec.to_owned())) {
            self.imports[outcome] += 1;
        }
    }
}
