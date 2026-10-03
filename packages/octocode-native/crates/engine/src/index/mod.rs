mod digest;
mod runtime;
mod search;
mod store;
mod traversal;
mod types;

pub use digest::sha256 as content_digest;
pub use runtime::{
    IndexAccess, IndexBuildOptions, IndexBuildResult, IndexQueryOptions, IndexQueryRuntimeResult,
    IndexRuntimeLimits, IndexStatus, IndexStatusOptions, build_index, index_status, query_index,
};
pub use search::{IndexQuery, IndexQueryKind, IndexQueryMatch, IndexQueryResult, query_documents};
pub use store::{GenerationReader, GenerationWriter, IndexError, IndexStore, Result};
pub use types::{
    ContentRecord, FreshnessReport, GenerationManifest, GenerationSpec, GraphFactSidecarRef,
    INDEX_LAYOUT_VERSION, IndexConfig, RootIdentity, SourceFileIdentity, SourceIdentity,
    SymbolRecord,
};

#[cfg(test)]
#[path = "../../tests/index/mod.rs"]
mod tests;
