mod digest;
mod runtime;
mod search;
mod store;
mod types;

pub use digest::sha256 as content_digest;
pub use runtime::{
    build_index, index_status, query_index, IndexAccess, IndexBuildOptions, IndexBuildResult,
    IndexQueryOptions, IndexQueryRuntimeResult, IndexRuntimeLimits, IndexStatus,
    IndexStatusOptions,
};
pub use search::{query_documents, IndexQuery, IndexQueryKind, IndexQueryMatch, IndexQueryResult};
pub use store::{GenerationReader, GenerationWriter, IndexError, IndexStore, Result};
pub use types::{
    ContentRecord, FreshnessReport, GenerationManifest, GenerationSpec, GraphFactSidecarRef,
    IndexConfig, RootIdentity, SourceFileIdentity, SourceIdentity, SymbolRecord,
    INDEX_LAYOUT_VERSION,
};

#[cfg(test)]
#[path = "../../tests/index/mod.rs"]
mod tests;
