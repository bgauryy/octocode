//! Bounded, disposable extraction artifacts. Resolution is always rebuilt.
use super::*;
use crate::cache::{CacheClass, CacheConfig, CacheKey, Disk, JsonCodec};
use octocode_engine::graph::{GraphFactsDocument, GraphFactsTypedEntry};
use serde::{Deserialize, Serialize};
use std::sync::{
    Mutex, MutexGuard, PoisonError,
    atomic::{AtomicBool, Ordering},
};

const ENTRY_BYTES: usize = 1024 * 1024;
const DISK_BYTES: u64 = 128 * 1024 * 1024;
const EXTRACTION_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact<S, F, C> {
    namespace: S,
    relative_path: S,
    content_digest: S,
    facts: F,
    reference_counts: C,
}

type OwnedArtifact = Artifact<String, GraphFactsDocument, Vec<(String, u32)>>;

pub(super) struct FactCache<'a> {
    disk: Option<Disk>,
    namespace: String,
    config: CacheConfig,
    validated: Mutex<BTreeMap<String, String>>,
    written: AtomicBool,
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
}

impl<'a> FactCache<'a> {
    pub(super) fn new(
        scan: &GraphScan,
        root: &Path,
        paths: &'a PathPolicy,
        security: &'a ContentSecurity,
        registry: RegistryView<'_>,
    ) -> Result<Self, ToolError> {
        let namespace = serde_json::to_string(&(
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
        })?;
        let disk = scan
            .fact_cache
            .as_ref()
            .and_then(|p| paths.validate_output(p).ok())
            .map(|p| Disk::new(p.canonical));
        Ok(Self {
            disk,
            namespace,
            config: CacheConfig {
                max_entries: 0,
                max_bytes: ENTRY_BYTES,
                disk_bytes: DISK_BYTES,
            },
            validated: Mutex::new(BTreeMap::new()),
            written: AtomicBool::new(false),
            paths,
            security,
        })
    }

    fn validated(&self) -> MutexGuard<'_, BTreeMap<String, String>> {
        self.validated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn key(&self, relative: &str) -> CacheKey {
        CacheKey {
            namespace: "graph-facts".into(),
            scope: self.namespace.clone(),
            resource: relative.into(),
        }
    }

    pub(super) fn load(&self, relative: &str, digest: &str) -> Option<GraphFactsTypedEntry> {
        self.validated().remove(relative);
        let disk = self.disk.as_ref()?;
        let key = self.key(relative);
        self.paths.validate(disk.file(&key)).ok()?;
        let (artifact, _, _, _) = disk.read::<OwnedArtifact, JsonCodec>(&key, &self.config)?;
        if artifact.namespace != self.namespace
            || artifact.relative_path != relative
            || artifact.content_digest != digest
        {
            return None;
        }
        self.validated().insert(relative.into(), digest.into());
        Some(GraphFactsTypedEntry {
            relative_path: artifact.relative_path,
            content_digest: artifact.content_digest,
            source_stamp: None,
            facts: artifact.facts,
            reference_counts: artifact
                .reference_counts
                .into_iter()
                .map(
                    |(declaration_id, count)| octocode_engine::types::GraphReferenceCount {
                        declaration_id,
                        count,
                    },
                )
                .collect(),
        })
    }

    pub(super) fn store(&self, view: &octocode_engine::fact_scan::GraphFactsView<'_>) {
        let Some(disk) = &self.disk else {
            return;
        };
        // An in-memory extraction hit need not exist on disk. Only skip a
        // digest this cache actually read or persisted during this scan.
        if self
            .validated()
            .get(view.relative_path)
            .is_some_and(|digest| digest == view.content_digest)
        {
            return;
        }
        let artifact = Artifact {
            namespace: self.namespace.as_str(),
            relative_path: view.relative_path,
            content_digest: view.content_digest,
            facts: view.facts,
            reference_counts: view
                .reference_counts
                .iter()
                .map(|count| (count.declaration_id.as_str(), count.count))
                .collect::<Vec<_>>(),
        };
        let Ok(bytes) = serde_json::to_vec(&artifact) else {
            return;
        };
        // Cache omission affects reuse only. Never persist extracted secrets.
        if bytes.len() > ENTRY_BYTES
            || self
                .security
                .sanitize_text(&String::from_utf8_lossy(&bytes), None)
                .has_secrets
        {
            return;
        }
        let key = self.key(view.relative_path);
        if self.paths.validate_output(disk.file(&key)).is_err() {
            return;
        }
        if disk.write_encoded(&key, &bytes, CacheClass::Immutable, &self.config) {
            self.written.store(true, Ordering::Relaxed);
            self.validated()
                .insert(view.relative_path.into(), view.content_digest.into());
        }
    }
}

impl Drop for FactCache<'_> {
    fn drop(&mut self) {
        if self.written.load(Ordering::Relaxed)
            && let Some(disk) = &self.disk
        {
            disk.prune(self.config.disk_bytes);
        }
    }
}

#[cfg(test)]
mod tests;
