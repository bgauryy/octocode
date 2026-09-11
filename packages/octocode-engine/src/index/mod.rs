mod digest;
mod store;
mod types;

pub use digest::sha256 as content_digest;
pub use store::{GenerationReader, GenerationWriter, IndexError, IndexStore, Result};
pub use types::{
    ContentRecord, FreshnessReport, GenerationManifest, GenerationSpec, GraphFactSidecarRef,
    IndexConfig, RootIdentity, SourceFileIdentity, SourceIdentity, SymbolRecord,
    INDEX_LAYOUT_VERSION,
};

#[cfg(test)]
#[path = "../../tests/index/mod.rs"]
mod tests;
