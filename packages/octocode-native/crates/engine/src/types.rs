#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisCapabilities {
    pub grammar_capabilities: Vec<GrammarCapability>,
    pub source_decoding: SourceDecodingCapability,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionAdapterCapability {
    pub id: String,
    pub version: String,
    pub operations: Vec<crate::text::analysis::AnalysisOperation>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDecodingCapability {
    pub encodings: Vec<SourceEncodingCapability>,
    pub malformed: &'static str,
    pub correspondence: &'static str,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceEncodingCapability {
    pub encoding: crate::text::analysis::Encoding,
    pub bom: &'static str,
}

/// One parser entry from the canonical grammar registry. Consumers use this
/// runtime inventory for language selection and agent guidance instead of
/// maintaining extension/name tables outside the engine.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrammarCapability {
    pub language: String,
    pub language_id: Option<String>,
    pub selector_aliases: Vec<String>,
    pub extensions: Vec<String>,
    pub structural_search: bool,
    pub signature_outline: bool,
    pub graph_facts: bool,
    /// Fact families the native extractor implements; not a completeness claim.
    pub facts: GrammarFactCapabilities,
    /// Grammar extraction itself does not establish semantic symbol identity.
    pub semantic_binding: bool,
    pub syntax_tree: bool,
    pub region_adapters: Vec<RegionAdapterCapability>,
    /// Admitted test declaration adapters; none of these records execution.
    pub test_frameworks: Vec<String>,
    pub argument_references: GrammarArgumentCapabilities,
    /// Callable/value propagation is not inferred from a direct-call graph.
    pub value_flow: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrammarArgumentCapabilities {
    pub supported: bool,
    pub forms: Vec<crate::graph::GraphArgumentForm>,
    pub unsupported_constructs: Vec<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrammarFactCapabilities {
    pub declarations: bool,
    pub calls: bool,
    pub imports: bool,
    pub exports: bool,
    pub heritage: bool,
}

impl GrammarFactCapabilities {
    pub const fn any(self) -> bool {
        self.declarations || self.calls || self.imports || self.exports || self.heritage
    }
}

mod fs_query;
pub(crate) mod fs_resume;
mod graph_scan;
mod ignore;
mod test_code;
mod text_search;

pub use fs_query::{
    EntryType, FileKind, FileSystemEntry, FileSystemQueryOptions, FileSystemQueryResult, ListsEntry,
};
pub use fs_resume::{WalkCursor, WalkTally};
pub use graph_scan::{GraphFactsScanDiagnostic, GraphFactsScanOptions, GraphLanguageGlob};
pub use ignore::IgnorePolicy;
pub use test_code::{TestCodeExcluded, TestCodeScope, WHOLE_FILE, wholly_left_out};
pub use text_search::{
    BinaryExtensionCount, CapReason, CaseRule, PatternSyntax, SearchContext, SearchOutput,
    SearchSort, SearchedSource, SourceValues, SpanValues, TextSearchFile, TextSearchMatch,
    TextSearchOptions, TextSearchResult, TextSearchStats,
};
