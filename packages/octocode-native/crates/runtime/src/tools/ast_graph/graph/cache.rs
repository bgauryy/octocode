//! The checkpoint's extraction artifact and the namespace that scopes it.
use super::*;
use octocode_engine::graph::{GraphFactsDocument, GraphFactsTypedEntry};
use serde::{Deserialize, Serialize};

const EXTRACTION_VERSION: u32 = 1;

pub(super) fn namespace(
    scan: &GraphScan,
    root: &Path,
    registry: RegistryView<'_>,
) -> Result<String, ToolError> {
    serde_json::to_string(&(
        root,
        &scan.language_globs,
        octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION,
        env!("CARGO_PKG_VERSION"),
        EXTRACTION_VERSION,
        super::registry_identity(registry)?,
    ))
    .map_err(|error| {
        ToolError::new(
            ErrorCode::ExecutionFailed,
            format!("Could not encode graph facts cache namespace: {error}"),
        )
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact<S, F, C> {
    pub(super) namespace: S,
    pub(super) relative_path: S,
    pub(super) content_digest: S,
    pub(super) facts: F,
    pub(super) reference_counts: C,
}

pub(super) type OwnedArtifact = Artifact<String, GraphFactsDocument, Vec<(String, u32)>>;

impl OwnedArtifact {
    pub(super) fn into_entry(self) -> GraphFactsTypedEntry {
        GraphFactsTypedEntry {
            relative_path: self.relative_path,
            content_digest: self.content_digest,
            source_stamp: None,
            facts: self.facts,
            reference_counts: self
                .reference_counts
                .into_iter()
                .map(
                    |(declaration_id, count)| octocode_engine::graph::GraphReferenceCount {
                        declaration_id,
                        count,
                    },
                )
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests;
