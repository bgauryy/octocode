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
}

mod fs_query;
mod graph_scan;
mod text_search;

pub use fs_query::{
    EntryType, FileKind, FileSystemEntry, FileSystemQueryOptions, FileSystemQueryResult,
};
pub use graph_scan::{
    GraphFactsScanDiagnostic, GraphFactsScanOptions, GraphLanguageGlob, GraphReferenceCount,
};
pub use text_search::{
    BinaryExtensionCount, CapReason, CaseRule, PatternSyntax, SearchContext, SearchOutput,
    SearchSort, SearchedSource, SourceValues, SpanValues, TextSearchFile, TextSearchMatch,
    TextSearchOptions, TextSearchResult, TextSearchStats,
};
