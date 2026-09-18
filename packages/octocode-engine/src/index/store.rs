use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
#[cfg(any(windows, all(unix, not(target_os = "linux"))))]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::digest::sha256;
use super::types::{
    validate_root_id, ContentRecord, FreshnessReport, GenerationManifest, GenerationSpec,
    GraphFactEnvelope, GraphFactSidecarRef, IndexConfig, RootIdentity, SourceFileIdentity,
    SymbolRecord, INDEX_LAYOUT_VERSION,
};

pub type Result<T> = std::result::Result<T, IndexError>;

#[derive(Debug)]
pub enum IndexError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    Serialization(serde_json::Error),
    InvalidRootId(String),
    InvalidSourceIdentity(String),
    InvalidRelativePath(PathBuf),
    PathEscapesRoot {
        path: PathBuf,
        root: PathBuf,
    },
    SymlinkSource {
        path: PathBuf,
    },
    NonFileSource {
        path: PathBuf,
    },
    RootIdentityMismatch {
        expected: PathBuf,
        actual: PathBuf,
    },
    RootIdMismatch {
        expected: String,
        actual: String,
    },
    SourceIdentityMismatch {
        expected: Box<RootIdentity>,
        actual: Box<RootIdentity>,
    },
    SchemaMismatch {
        expected: u32,
        actual: u32,
    },
    ParserSchemaMismatch {
        expected: u32,
        actual: u32,
    },
    ToolVersionMismatch {
        expected: String,
        actual: String,
    },
    GenerationMismatch {
        expected: u64,
        actual: u64,
    },
    LayoutMismatch {
        expected: u32,
        actual: u32,
    },
    CorruptActivePointer {
        path: PathBuf,
        value: String,
    },
    CorruptGeneration {
        generation: u64,
        detail: String,
    },
    WriterLocked {
        path: PathBuf,
    },
    NoActiveGeneration,
    InvalidConfig(String),
    DuplicateDocument(String),
    QuotaExceeded {
        limit: u64,
        required: u64,
    },
    SidecarDigestMismatch {
        expected: String,
        actual: String,
    },
}

impl fmt::Display for IndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "{operation} failed for {}: {source}",
                path.display()
            ),
            Self::Serialization(source) => {
                write!(formatter, "index serialization failed: {source}")
            }
            Self::InvalidRootId(root_id) => write!(formatter, "invalid index root id: {root_id}"),
            Self::InvalidSourceIdentity(detail) => {
                write!(formatter, "invalid source identity: {detail}")
            }
            Self::InvalidRelativePath(path) => {
                write!(
                    formatter,
                    "index source path must be relative: {}",
                    path.display()
                )
            }
            Self::PathEscapesRoot { path, root } => write!(
                formatter,
                "index source {} escapes root {}",
                path.display(),
                root.display()
            ),
            Self::SymlinkSource { path } => {
                write!(
                    formatter,
                    "refusing to index symlink source: {}",
                    path.display()
                )
            }
            Self::NonFileSource { path } => {
                write!(formatter, "index source is not a file: {}", path.display())
            }
            Self::RootIdentityMismatch { expected, actual } => write!(
                formatter,
                "source root mismatch: expected {}, got {}",
                expected.display(),
                actual.display()
            ),
            Self::RootIdMismatch { expected, actual } => {
                write!(
                    formatter,
                    "index root id mismatch: expected {expected}, got {actual}"
                )
            }
            Self::SourceIdentityMismatch { .. } => write!(formatter, "source identity mismatch"),
            Self::SchemaMismatch { expected, actual } => write!(
                formatter,
                "index schema mismatch: expected {expected}, got {actual}; rebuild required"
            ),
            Self::ParserSchemaMismatch { expected, actual } => write!(
                formatter,
                "parser schema mismatch: expected {expected}, got {actual}; rebuild required"
            ),
            Self::ToolVersionMismatch { expected, actual } => write!(
                formatter,
                "index tool version mismatch: expected {expected}, got {actual}; rebuild required"
            ),
            Self::GenerationMismatch { expected, actual } => write!(
                formatter,
                "index generation mismatch: expected {expected}, got {actual}; restart pagination"
            ),
            Self::LayoutMismatch { expected, actual } => write!(
                formatter,
                "index layout mismatch: expected {expected}, got {actual}; rebuild required"
            ),
            Self::CorruptActivePointer { path, value } => write!(
                formatter,
                "corrupt active generation pointer {}: {value:?}",
                path.display()
            ),
            Self::CorruptGeneration { generation, detail } => {
                write!(formatter, "corrupt index generation {generation}: {detail}")
            }
            Self::WriterLocked { path } => {
                write!(formatter, "index writer lock is held: {}", path.display())
            }
            Self::NoActiveGeneration => write!(formatter, "index has no active generation"),
            Self::InvalidConfig(detail) => write!(formatter, "invalid index config: {detail}"),
            Self::DuplicateDocument(path) => write!(formatter, "duplicate index document: {path}"),
            Self::QuotaExceeded { limit, required } => write!(
                formatter,
                "index generation requires {required} bytes, exceeding quota {limit}"
            ),
            Self::SidecarDigestMismatch { expected, actual } => write!(
                formatter,
                "graph-fact sidecar digest mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for IndexError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Serialization(source) => Some(source),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for IndexError {
    fn from(source: serde_json::Error) -> Self {
        Self::Serialization(source)
    }
}

#[derive(Clone, Debug)]
pub struct IndexStore {
    directory: PathBuf,
    root_id: String,
    config: IndexConfig,
}

impl IndexStore {
    pub fn open(home: PathBuf, root_id: &str, config: IndexConfig) -> Result<Self> {
        validate_root_id(root_id)?;
        if config.max_generations == 0 {
            return Err(IndexError::InvalidConfig(
                "max_generations must be at least one".to_owned(),
            ));
        }
        if config.max_bytes == 0 {
            return Err(IndexError::InvalidConfig(
                "max_bytes must be at least one".to_owned(),
            ));
        }
        if config.expected_tool_version.is_empty() {
            return Err(IndexError::InvalidConfig(
                "expected_tool_version must be non-empty".to_owned(),
            ));
        }
        let directory = home
            .join("index")
            .join(format!("v{INDEX_LAYOUT_VERSION}"))
            .join(root_id);
        create_dir_all(&directory)?;
        create_dir_all(&directory.join("generations"))?;
        create_dir_all(&directory.join("sidecars"))?;
        create_dir_all(&directory.join("staging"))?;
        Ok(Self {
            directory,
            root_id: root_id.to_owned(),
            config,
        })
    }

    #[must_use]
    pub fn active_pointer_path(&self) -> PathBuf {
        self.directory.join("ACTIVE")
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn begin_generation(&self, spec: GenerationSpec) -> Result<GenerationWriter> {
        if spec.root.root_id != self.root_id {
            return Err(IndexError::RootIdMismatch {
                expected: self.root_id.clone(),
                actual: spec.root.root_id.clone(),
            });
        }
        if spec.index_schema_version != self.config.expected_schema_version {
            return Err(IndexError::SchemaMismatch {
                expected: self.config.expected_schema_version,
                actual: spec.index_schema_version,
            });
        }
        if spec.parser_schema_version != self.config.expected_parser_schema_version {
            return Err(IndexError::ParserSchemaMismatch {
                expected: self.config.expected_parser_schema_version,
                actual: spec.parser_schema_version,
            });
        }
        if spec.tool_version != self.config.expected_tool_version {
            return Err(IndexError::ToolVersionMismatch {
                expected: self.config.expected_tool_version.clone(),
                actual: spec.tool_version,
            });
        }
        let lock = WriterLock::acquire(self.directory.join("WRITE.lock"))?;
        let generation = self
            .list_generations()?
            .into_iter()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| IndexError::InvalidConfig("generation counter overflow".to_owned()))?;
        let staging_directory = self.directory.join("staging").join(format!(
            "generation-{generation:020}-{}-{}",
            std::process::id(),
            unique_nonce()
        ));
        create_dir(&staging_directory)?;
        Ok(GenerationWriter {
            store: self.clone(),
            spec,
            generation,
            documents: BTreeMap::new(),
            sidecars: BTreeMap::new(),
            staging_directory,
            committed: false,
            _lock: lock,
        })
    }

    pub fn open_active(&self, expected_root: &RootIdentity) -> Result<GenerationReader> {
        let generation = self.read_active_generation()?;
        let generation_directory = self.generation_directory(generation);
        let manifest: GenerationManifest = read_json(&generation_directory.join("manifest.json"))?;
        validate_manifest_integrity(&manifest)?;
        if manifest.generation != generation {
            return Err(IndexError::CorruptGeneration {
                generation,
                detail: "manifest generation does not match active pointer".to_owned(),
            });
        }
        if manifest.layout_version != INDEX_LAYOUT_VERSION {
            return Err(IndexError::LayoutMismatch {
                expected: INDEX_LAYOUT_VERSION,
                actual: manifest.layout_version,
            });
        }
        if manifest.index_schema_version != self.config.expected_schema_version {
            return Err(IndexError::SchemaMismatch {
                expected: self.config.expected_schema_version,
                actual: manifest.index_schema_version,
            });
        }
        if manifest.parser_schema_version != self.config.expected_parser_schema_version {
            return Err(IndexError::ParserSchemaMismatch {
                expected: self.config.expected_parser_schema_version,
                actual: manifest.parser_schema_version,
            });
        }
        if manifest.tool_version != self.config.expected_tool_version {
            return Err(IndexError::ToolVersionMismatch {
                expected: self.config.expected_tool_version.clone(),
                actual: manifest.tool_version,
            });
        }
        if manifest.root != *expected_root {
            return Err(IndexError::SourceIdentityMismatch {
                expected: Box::new(expected_root.clone()),
                actual: Box::new(manifest.root),
            });
        }
        let documents_path = generation_directory.join("documents.json");
        let document_bytes = read_bytes(&documents_path)?;
        let actual_documents_digest = sha256(&document_bytes);
        if actual_documents_digest != manifest.documents_digest {
            return Err(IndexError::CorruptGeneration {
                generation,
                detail: format!(
                    "document payload digest mismatch: expected {}, got {actual_documents_digest}",
                    manifest.documents_digest
                ),
            });
        }
        let documents: Vec<ContentRecord> = serde_json::from_slice(&document_bytes)?;
        if documents.len() as u64 != manifest.document_count {
            return Err(IndexError::CorruptGeneration {
                generation,
                detail: "document count does not match manifest".to_owned(),
            });
        }
        let mut sidecars = BTreeMap::new();
        for reference in &manifest.graph_fact_sidecars {
            let payload = read_and_validate_sidecar(&self.directory, generation, reference)?;
            sidecars.insert(reference.digest.clone(), payload);
        }
        Ok(GenerationReader {
            manifest,
            documents,
            sidecars,
        })
    }

    pub fn list_generations(&self) -> Result<Vec<u64>> {
        let generations_path = self.directory.join("generations");
        let entries = fs::read_dir(&generations_path).map_err(|source| IndexError::Io {
            operation: "list index generations",
            path: generations_path.clone(),
            source,
        })?;
        let mut generations = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| IndexError::Io {
                operation: "read index generation entry",
                path: generations_path.clone(),
                source,
            })?;
            if !entry
                .file_type()
                .map_err(|source| IndexError::Io {
                    operation: "read index generation type",
                    path: entry.path(),
                    source,
                })?
                .is_dir()
            {
                continue;
            }
            if let Some(generation) = parse_generation_name(&entry.file_name().to_string_lossy()) {
                generations.push(generation);
            }
        }
        generations.sort_unstable();
        Ok(generations)
    }

    fn read_active_generation(&self) -> Result<u64> {
        let pointer = self.active_pointer_path();
        let value = match fs::read_to_string(&pointer) {
            Ok(value) => value,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(IndexError::NoActiveGeneration)
            }
            Err(source) => {
                return Err(IndexError::Io {
                    operation: "read active generation pointer",
                    path: pointer,
                    source,
                })
            }
        };
        value
            .trim()
            .parse::<u64>()
            .map_err(|_| IndexError::CorruptActivePointer {
                path: pointer,
                value,
            })
    }

    fn generation_directory(&self, generation: u64) -> PathBuf {
        self.directory
            .join("generations")
            .join(format!("generation-{generation:020}"))
    }

    fn prune_generations(&self, active: u64) -> Result<()> {
        #[cfg(test)]
        {
            let failure_marker = self.directory.join(".test-fail-next-prune");
            if failure_marker.exists() {
                fs::remove_file(&failure_marker).map_err(|source| IndexError::Io {
                    operation: "remove prune failure marker",
                    path: failure_marker.clone(),
                    source,
                })?;
                return Err(IndexError::Io {
                    operation: "prune index generation",
                    path: self.directory.join("generations"),
                    source: std::io::Error::other("injected prune failure"),
                });
            }
        }
        let mut generations = self.list_generations()?;
        while generations.len() > self.config.max_generations {
            let Some(candidate) = generations.iter().copied().find(|value| *value != active) else {
                break;
            };
            let path = self.generation_directory(candidate);
            fs::remove_dir_all(&path).map_err(|source| IndexError::Io {
                operation: "prune index generation",
                path,
                source,
            })?;
            generations.retain(|value| *value != candidate);
        }
        Ok(())
    }

    fn garbage_collect_sidecars(&self) -> Result<()> {
        let mut referenced = BTreeSet::new();
        for generation in self.list_generations()? {
            let manifest: GenerationManifest =
                read_json(&self.generation_directory(generation).join("manifest.json"))?;
            validate_manifest_integrity(&manifest)?;
            referenced.extend(
                manifest
                    .graph_fact_sidecars
                    .iter()
                    .map(|reference| reference.digest.clone()),
            );
        }
        let sidecars = self.directory.join("sidecars");
        let mut removed = false;
        for entry in fs::read_dir(&sidecars).map_err(|source| IndexError::Io {
            operation: "list graph-fact sidecars",
            path: sidecars.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| IndexError::Io {
                operation: "read graph-fact sidecar entry",
                path: sidecars.clone(),
                source,
            })?;
            let file_type = entry.file_type().map_err(|source| IndexError::Io {
                operation: "inspect graph-fact sidecar entry",
                path: entry.path(),
                source,
            })?;
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(digest) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
                continue;
            };
            if !is_sha256_hex(digest) {
                continue;
            }
            if referenced.contains(digest) {
                continue;
            }
            fs::remove_file(entry.path()).map_err(|source| IndexError::Io {
                operation: "remove unreferenced graph-fact sidecar",
                path: entry.path(),
                source,
            })?;
            removed = true;
        }
        if removed {
            sync_directory(&sidecars)?;
        }
        Ok(())
    }

    fn projected_size_after_commit(
        &self,
        new_generation: u64,
        generation_bytes: u64,
        new_sidecars: &[GraphFactSidecarRef],
    ) -> Result<u64> {
        let mut generations = self.list_generations()?;
        generations.push(new_generation);
        generations.sort_unstable();
        let prune_count = generations
            .len()
            .saturating_sub(self.config.max_generations);
        let pruned = generations
            .iter()
            .copied()
            .take(prune_count)
            .collect::<BTreeSet<_>>();
        let retained = generations
            .iter()
            .copied()
            .filter(|generation| *generation != new_generation && !pruned.contains(generation))
            .collect::<Vec<_>>();

        let current = directory_size(&self.directory)?;
        let pruned_generation_bytes = pruned
            .iter()
            .filter(|generation| **generation != new_generation)
            .map(|generation| directory_size(&self.generation_directory(*generation)))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .fold(0_u64, u64::saturating_add);

        let mut retained_sidecars = new_sidecars
            .iter()
            .map(|reference| reference.digest.clone())
            .collect::<BTreeSet<_>>();
        let mut retained_manifests_valid = true;
        for generation in retained {
            let manifest = read_json::<GenerationManifest>(
                &self.generation_directory(generation).join("manifest.json"),
            );
            match manifest {
                Ok(manifest) if validate_manifest_integrity(&manifest).is_ok() => {
                    retained_sidecars.extend(
                        manifest
                            .graph_fact_sidecars
                            .into_iter()
                            .map(|reference| reference.digest),
                    );
                }
                _ => retained_manifests_valid = false,
            }
        }
        let reclaimable_sidecar_bytes = if retained_manifests_valid {
            sidecar_bytes_not_in(&self.directory.join("sidecars"), &retained_sidecars)?
        } else {
            0
        };
        Ok(current
            .saturating_add(generation_bytes)
            .saturating_sub(pruned_generation_bytes)
            .saturating_sub(reclaimable_sidecar_bytes))
    }

    #[cfg(test)]
    pub fn fail_next_prune_for_test(&self) {
        fs::write(self.directory.join(".test-fail-next-prune"), b"fail")
            .expect("write prune failure marker");
    }

    #[cfg(test)]
    pub fn resign_manifest_for_test(&self, generation: u64) -> Result<()> {
        let path = self.generation_directory(generation).join("manifest.json");
        let mut manifest: GenerationManifest = read_json(&path)?;
        manifest.manifest_digest = manifest_digest(&manifest)?;
        write_atomic(&path, &serde_json::to_vec(&manifest)?)
    }
}

pub(super) struct CapturedSource {
    pub content: String,
    identity: SourceFileIdentity,
}

impl CapturedSource {
    pub(super) fn new(content: String, metadata: &fs::Metadata) -> Self {
        let identity = SourceFileIdentity {
            size: content.len() as u64,
            modified_nanos: modified_nanos(metadata),
            content_digest: sha256(content.as_bytes()),
        };
        Self { content, identity }
    }
}

/// Capture at most one sentinel byte beyond the caller's byte budget.
pub(super) fn read_source_bytes(
    root: &Path,
    relative_path: &Path,
    max_bytes: u64,
) -> Result<(Vec<u8>, fs::Metadata)> {
    let path = secure_source_path(root, relative_path)?;
    let file = File::open(&path).map_err(|source| IndexError::Io {
        operation: "open index source",
        path: path.clone(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| IndexError::Io {
        operation: "read index source metadata",
        path: path.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(IndexError::NonFileSource { path });
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| IndexError::Io {
            operation: "read index source",
            path,
            source,
        })?;
    Ok((bytes, metadata))
}

pub struct GenerationWriter {
    store: IndexStore,
    spec: GenerationSpec,
    generation: u64,
    documents: BTreeMap<String, ContentRecord>,
    sidecars: BTreeMap<String, (GraphFactSidecarRef, Vec<u8>)>,
    staging_directory: PathBuf,
    committed: bool,
    _lock: WriterLock,
}

impl GenerationWriter {
    pub fn add_file(
        &mut self,
        root: &Path,
        relative_path: impl AsRef<Path>,
        language: &str,
        symbols: Vec<SymbolRecord>,
    ) -> Result<()> {
        let normalized = self.validate_source(root, relative_path.as_ref())?;
        let (bytes, metadata) = read_source_bytes(root, Path::new(&normalized), u64::MAX)?;
        let content = String::from_utf8(bytes).map_err(|source| {
            IndexError::InvalidSourceIdentity(format!("source {normalized} is not UTF-8: {source}"))
        })?;
        self.insert_source(
            normalized,
            language,
            CapturedSource::new(content, &metadata),
            symbols,
        );
        Ok(())
    }

    pub(super) fn add_captured_file(
        &mut self,
        root: &Path,
        relative_path: impl AsRef<Path>,
        language: &str,
        source: CapturedSource,
        symbols: Vec<SymbolRecord>,
    ) -> Result<()> {
        let normalized = self.validate_source(root, relative_path.as_ref())?;
        self.insert_source(normalized, language, source, symbols);
        Ok(())
    }

    fn validate_source(&self, root: &Path, relative_path: &Path) -> Result<String> {
        let canonical_root = root.canonicalize().map_err(|source| IndexError::Io {
            operation: "canonicalize index source root",
            path: root.to_path_buf(),
            source,
        })?;
        if canonical_root != self.spec.root.canonical_root {
            return Err(IndexError::RootIdentityMismatch {
                expected: self.spec.root.canonical_root.clone(),
                actual: canonical_root,
            });
        }
        let normalized = normalize_relative_path(relative_path)?;
        if self.documents.contains_key(&normalized) {
            return Err(IndexError::DuplicateDocument(normalized));
        }
        secure_source_path(root, Path::new(&normalized))?;
        Ok(normalized)
    }

    fn insert_source(
        &mut self,
        normalized: String,
        language: &str,
        source: CapturedSource,
        symbols: Vec<SymbolRecord>,
    ) {
        self.documents.insert(
            normalized.clone(),
            ContentRecord {
                path: normalized,
                language: language.to_owned(),
                content: source.content,
                identity: source.identity,
                symbols,
            },
        );
    }

    pub fn add_graph_fact_sidecar(
        &mut self,
        schema_version: u32,
        payload: &[u8],
    ) -> Result<GraphFactSidecarRef> {
        let envelope = GraphFactEnvelope {
            layout_version: INDEX_LAYOUT_VERSION,
            schema_version,
            payload: payload.to_vec(),
        };
        let encoded = serde_json::to_vec(&envelope)?;
        let digest = sha256(&encoded);
        let reference = GraphFactSidecarRef {
            digest: digest.clone(),
            schema_version,
            payload_bytes: payload.len() as u64,
        };
        self.sidecars
            .entry(digest)
            .or_insert_with(|| (reference.clone(), encoded));
        Ok(reference)
    }

    pub fn commit(mut self) -> Result<GenerationManifest> {
        let documents = self.documents.values().cloned().collect::<Vec<_>>();
        let sidecar_refs = self
            .sidecars
            .values()
            .map(|(reference, _)| reference.clone())
            .collect::<Vec<_>>();
        let document_bytes = serde_json::to_vec(&documents)?;
        let mut manifest = GenerationManifest {
            layout_version: INDEX_LAYOUT_VERSION,
            generation: self.generation,
            root: self.spec.root.clone(),
            index_schema_version: self.spec.index_schema_version,
            parser_schema_version: self.spec.parser_schema_version,
            tool_version: self.spec.tool_version.clone(),
            exclusions: self.spec.exclusions.clone(),
            complete: self.spec.complete,
            document_count: documents.len() as u64,
            indexed_source_bytes: documents
                .iter()
                .map(|document| document.identity.size)
                .sum(),
            documents_digest: sha256(&document_bytes),
            graph_fact_sidecars: sidecar_refs,
            manifest_digest: String::new(),
        };
        manifest.manifest_digest = manifest_digest(&manifest)?;
        let manifest_bytes = serde_json::to_vec(&manifest)?;
        let sidecar_bytes = self
            .sidecars
            .iter()
            .filter(|(digest, _)| {
                !self
                    .store
                    .directory
                    .join("sidecars")
                    .join(format!("{digest}.json"))
                    .exists()
            })
            .map(|(_, (_, bytes))| bytes.len() as u64)
            .sum::<u64>();
        let generation_bytes = (document_bytes.len() as u64)
            .saturating_add(manifest_bytes.len() as u64)
            .saturating_add(sidecar_bytes);
        let required = self.store.projected_size_after_commit(
            self.generation,
            generation_bytes,
            &manifest.graph_fact_sidecars,
        )?;
        if required > self.store.config.max_bytes {
            return Err(IndexError::QuotaExceeded {
                limit: self.store.config.max_bytes,
                required,
            });
        }

        write_atomic(
            &self.staging_directory.join("documents.json"),
            &document_bytes,
        )?;
        write_atomic(
            &self.staging_directory.join("manifest.json"),
            &manifest_bytes,
        )?;
        for (digest, (_, bytes)) in &self.sidecars {
            let path = self
                .store
                .directory
                .join("sidecars")
                .join(format!("{digest}.json"));
            if !path.exists() {
                write_atomic(&path, bytes)?;
            }
        }
        sync_directory(&self.staging_directory)?;

        let generation_directory = self.store.generation_directory(self.generation);
        fs::rename(&self.staging_directory, &generation_directory).map_err(|source| {
            IndexError::Io {
                operation: "promote staged index generation",
                path: generation_directory.clone(),
                source,
            }
        })?;
        sync_directory(&self.store.directory.join("generations"))?;
        write_atomic(
            &self.store.active_pointer_path(),
            format!("{}\n", self.generation).as_bytes(),
        )?;
        self.committed = true;
        // ACTIVE is the durable commit boundary. Retention is best-effort
        // housekeeping: reporting failure after activation would invite an unsafe retry.
        let _ = self.store.prune_generations(self.generation);
        let _ = self.store.garbage_collect_sidecars();
        Ok(manifest)
    }
}

impl Drop for GenerationWriter {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.staging_directory);
        }
    }
}

#[derive(Clone, Debug)]
pub struct GenerationReader {
    manifest: GenerationManifest,
    documents: Vec<ContentRecord>,
    sidecars: BTreeMap<String, Vec<u8>>,
}

impl GenerationReader {
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.manifest.generation
    }

    #[must_use]
    pub fn manifest(&self) -> &GenerationManifest {
        &self.manifest
    }

    #[must_use]
    pub fn documents(&self) -> &[ContentRecord] {
        &self.documents
    }

    #[must_use]
    pub fn verify_strict(&self, root: &Path) -> FreshnessReport {
        self.verify_strict_bounded(root, usize::MAX, usize::MAX)
    }

    #[must_use]
    pub fn verify_strict_bounded(
        &self,
        root: &Path,
        max_entries: usize,
        max_depth: usize,
    ) -> FreshnessReport {
        let canonical_root = match root.canonicalize() {
            Ok(value) if value == self.manifest.root.canonical_root => value,
            _ => {
                return FreshnessReport {
                    checked: self.documents.len(),
                    unverifiable: self
                        .documents
                        .iter()
                        .map(|document| document.path.clone())
                        .collect(),
                    ..FreshnessReport::default()
                }
            }
        };
        let mut report = FreshnessReport {
            checked: self.documents.len(),
            generation_complete: self.manifest.complete,
            traversal_complete: true,
            ..FreshnessReport::default()
        };
        for (index, document) in self.documents.iter().enumerate() {
            if index >= max_entries {
                report.traversal_complete = false;
                report.unverifiable.push(document.path.clone());
                continue;
            }
            report.scanned_entries += 1;
            let path = canonical_root.join(&document.path);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                    report.deleted.push(document.path.clone());
                    continue;
                }
                Err(_) => {
                    report.unverifiable.push(document.path.clone());
                    continue;
                }
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                report.unverifiable.push(document.path.clone());
                continue;
            }
            match fs::read(&path) {
                Ok(bytes) if sha256(&bytes) == document.identity.content_digest => {
                    report.fresh += 1;
                }
                Ok(_) => report.dirty.push(document.path.clone()),
                Err(_) => report.unverifiable.push(document.path.clone()),
            }
        }
        let indexed_paths = self
            .documents
            .iter()
            .map(|document| document.path.as_str())
            .collect::<BTreeSet<_>>();
        let mut current_paths = Vec::new();
        let mut traversal_entries = 0_usize;
        let traversal_complete = collect_current_paths_bounded(
            &canonical_root,
            &self.manifest.exclusions,
            max_entries,
            max_depth,
            &mut current_paths,
            &mut report.unverifiable,
            &mut traversal_entries,
        );
        report.scanned_entries = report.scanned_entries.saturating_add(traversal_entries);
        report.traversal_complete &= traversal_complete;
        for current_path in current_paths {
            if !indexed_paths.contains(current_path.as_str()) {
                report.added.push(current_path);
            }
        }
        report.added.sort();
        report.unverifiable.sort();
        report.unverifiable.dedup();
        report
    }

    pub fn read_graph_fact_sidecar(&self, reference: &GraphFactSidecarRef) -> Result<Vec<u8>> {
        if !self.manifest.graph_fact_sidecars.contains(reference) {
            return Err(IndexError::CorruptGeneration {
                generation: self.generation(),
                detail: "graph-fact sidecar reference does not match manifest".to_owned(),
            });
        }
        self.sidecars
            .get(&reference.digest)
            .cloned()
            .ok_or_else(|| IndexError::CorruptGeneration {
                generation: self.generation(),
                detail: "validated graph-fact sidecar is unavailable".to_owned(),
            })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WriterLockRecord {
    pid: u32,
    process_identity: Option<String>,
    nonce: String,
}

struct WriterLock {
    file: File,
}

impl WriterLock {
    fn acquire(path: PathBuf) -> Result<Self> {
        let record = WriterLockRecord {
            pid: std::process::id(),
            process_identity: process_identity(std::process::id()),
            nonce: format!("{}-{}", std::process::id(), unique_nonce()),
        };
        let mut record_bytes = serde_json::to_vec(&record)?;
        record_bytes.push(b'\n');
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| IndexError::Io {
                operation: "open index writer lock",
                path: path.clone(),
                source,
            })?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(IndexError::WriterLocked { path });
            }
            Err(std::fs::TryLockError::Error(source)) => {
                return Err(IndexError::Io {
                    operation: "acquire index writer lock",
                    path,
                    source,
                });
            }
        }
        // A crashed writer leaves bytes but not an advisory lock. Replace its
        // diagnostic ownership record only after the OS grants exclusivity.
        file.set_len(0).map_err(|source| IndexError::Io {
            operation: "truncate index writer lock",
            path: path.clone(),
            source,
        })?;
        file.write_all(&record_bytes)
            .map_err(|source| IndexError::Io {
                operation: "write index writer lock",
                path: path.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| IndexError::Io {
            operation: "sync index writer lock",
            path: path.clone(),
            source,
        })?;
        if let Some(parent) = path.parent() {
            sync_directory(parent)?;
        }
        Ok(Self { file })
    }
}

impl Drop for WriterLock {
    fn drop(&mut self) {
        // Drop cannot report cleanup errors. The OS still releases the
        // advisory lock when the file handle closes.
        let _ = self.file.unlock();
    }
}

#[cfg(target_os = "linux")]
fn process_identity(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_name = stat.rsplit_once(')')?.1;
    let start_ticks = after_name.split_whitespace().nth(19)?;
    Some(format!("linux-start:{start_ticks}"))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_identity(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let started = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!started.is_empty()).then(|| format!("unix-start:{started}"))
}

#[cfg(windows)]
fn process_identity(pid: u32) -> Option<String> {
    let command =
        format!("(Get-Process -Id {pid} -ErrorAction Stop).StartTime.ToUniversalTime().Ticks");
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let started = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!started.is_empty()).then(|| format!("windows-start:{started}"))
}

#[cfg(not(any(unix, windows)))]
fn process_identity(pid: u32) -> Option<String> {
    (pid == std::process::id()).then(|| format!("current-process:{pid}"))
}

fn secure_source_path(root: &Path, relative_path: &Path) -> Result<PathBuf> {
    let canonical_root = root.canonicalize().map_err(|source| IndexError::Io {
        operation: "canonicalize index source root",
        path: root.to_path_buf(),
        source,
    })?;
    let mut cursor = canonical_root.clone();
    for component in relative_path.components() {
        let Component::Normal(part) = component else {
            return Err(IndexError::InvalidRelativePath(relative_path.to_path_buf()));
        };
        cursor.push(part);
        let metadata = fs::symlink_metadata(&cursor).map_err(|source| IndexError::Io {
            operation: "inspect index source path",
            path: cursor.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(IndexError::SymlinkSource { path: cursor });
        }
    }
    let canonical_path = cursor.canonicalize().map_err(|source| IndexError::Io {
        operation: "canonicalize index source path",
        path: cursor.clone(),
        source,
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(IndexError::PathEscapesRoot {
            path: canonical_path,
            root: canonical_root,
        });
    }
    let metadata = fs::metadata(&canonical_path).map_err(|source| IndexError::Io {
        operation: "read index source metadata",
        path: canonical_path.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(IndexError::NonFileSource {
            path: canonical_path,
        });
    }
    Ok(canonical_path)
}

fn normalize_relative_path(path: &Path) -> Result<String> {
    if path.is_absolute() || path.components().next().is_none() {
        return Err(IndexError::InvalidRelativePath(path.to_path_buf()));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => return Err(IndexError::InvalidRelativePath(path.to_path_buf())),
        }
    }
    Ok(parts.join("/"))
}

fn modified_nanos(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

fn unique_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn parse_generation_name(name: &str) -> Option<u64> {
    name.strip_prefix("generation-")?.parse().ok()
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir(path).map_err(|source| IndexError::Io {
        operation: "create index directory",
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| IndexError::Io {
        operation: "create index directory tree",
        path: path.to_path_buf(),
        source,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(IndexError::from)
}

fn manifest_digest(manifest: &GenerationManifest) -> Result<String> {
    let mut canonical = manifest.clone();
    canonical.manifest_digest.clear();
    Ok(sha256(&serde_json::to_vec(&canonical)?))
}

fn validate_manifest_integrity(manifest: &GenerationManifest) -> Result<()> {
    let actual = manifest_digest(manifest)?;
    if actual != manifest.manifest_digest {
        return Err(IndexError::CorruptGeneration {
            generation: manifest.generation,
            detail: format!(
                "manifest digest mismatch: expected {}, got {actual}",
                manifest.manifest_digest
            ),
        });
    }
    Ok(())
}

fn read_and_validate_sidecar(
    store_directory: &Path,
    generation: u64,
    reference: &GraphFactSidecarRef,
) -> Result<Vec<u8>> {
    let path = store_directory
        .join("sidecars")
        .join(format!("{}.json", reference.digest));
    let bytes = fs::read(&path).map_err(|source| IndexError::Io {
        operation: "read graph-fact sidecar",
        path: path.clone(),
        source,
    })?;
    let actual = sha256(&bytes);
    if actual != reference.digest {
        return Err(IndexError::SidecarDigestMismatch {
            expected: reference.digest.clone(),
            actual,
        });
    }
    let envelope: GraphFactEnvelope = serde_json::from_slice(&bytes)?;
    if envelope.layout_version != INDEX_LAYOUT_VERSION {
        return Err(IndexError::LayoutMismatch {
            expected: INDEX_LAYOUT_VERSION,
            actual: envelope.layout_version,
        });
    }
    if envelope.schema_version != reference.schema_version {
        return Err(IndexError::CorruptGeneration {
            generation,
            detail: "graph-fact sidecar schema does not match manifest".to_owned(),
        });
    }
    if envelope.payload.len() as u64 != reference.payload_bytes {
        return Err(IndexError::CorruptGeneration {
            generation,
            detail: format!(
                "graph-fact sidecar payload length mismatch: expected {}, got {}",
                reference.payload_bytes,
                envelope.payload.len()
            ),
        });
    }
    Ok(envelope.payload)
}

fn sidecar_bytes_not_in(directory: &Path, retained: &BTreeSet<String>) -> Result<u64> {
    let entries = fs::read_dir(directory).map_err(|source| IndexError::Io {
        operation: "list graph-fact sidecars for quota",
        path: directory.to_path_buf(),
        source,
    })?;
    let mut bytes = 0_u64;
    for entry in entries {
        let entry = entry.map_err(|source| IndexError::Io {
            operation: "read graph-fact sidecar quota entry",
            path: directory.to_path_buf(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| IndexError::Io {
            operation: "inspect graph-fact sidecar quota entry",
            path: entry.path(),
            source,
        })?;
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(digest) = name.to_str().and_then(|name| name.strip_suffix(".json")) else {
            continue;
        };
        if !is_sha256_hex(digest) {
            continue;
        }
        if !retained.contains(digest) {
            bytes = bytes.saturating_add(
                entry
                    .metadata()
                    .map_err(|source| IndexError::Io {
                        operation: "read graph-fact sidecar quota metadata",
                        path: entry.path(),
                        source,
                    })?
                    .len(),
            );
        }
    }
    Ok(bytes)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path).map_err(|source| IndexError::Io {
        operation: "open index data",
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| IndexError::Io {
            operation: "read index data",
            path: path.to_path_buf(),
            source,
        })?;
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        IndexError::InvalidConfig(format!("atomic path has no parent: {}", path.display()))
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            IndexError::InvalidConfig(format!("atomic path has no UTF-8 name: {}", path.display()))
        })?;
    let temporary = parent.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        unique_nonce()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|source| IndexError::Io {
            operation: "create atomic index file",
            path: temporary.clone(),
            source,
        })?;
    file.write_all(bytes).map_err(|source| IndexError::Io {
        operation: "write atomic index file",
        path: temporary.clone(),
        source,
    })?;
    file.sync_all().map_err(|source| IndexError::Io {
        operation: "sync atomic index file",
        path: temporary.clone(),
        source,
    })?;
    drop(file);
    fs::rename(&temporary, path).map_err(|source| IndexError::Io {
        operation: "promote atomic index file",
        path: path.to_path_buf(),
        source,
    })?;
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<()> {
    let directory = File::open(path).map_err(|source| IndexError::Io {
        operation: "open index directory for sync",
        path: path.to_path_buf(),
        source,
    })?;
    directory.sync_all().map_err(|source| IndexError::Io {
        operation: "sync index directory",
        path: path.to_path_buf(),
        source,
    })
}

fn directory_size(path: &Path) -> Result<u64> {
    let entries = fs::read_dir(path).map_err(|source| IndexError::Io {
        operation: "read index directory for quota",
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = 0_u64;
    for entry in entries {
        let entry = entry.map_err(|source| IndexError::Io {
            operation: "read index quota entry",
            path: path.to_path_buf(),
            source,
        })?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path).map_err(|source| IndexError::Io {
            operation: "read index quota metadata",
            path: entry_path.clone(),
            source,
        })?;
        bytes = if metadata.is_dir() {
            bytes.saturating_add(directory_size(&entry_path)?)
        } else {
            bytes.saturating_add(metadata.len())
        };
    }
    Ok(bytes)
}

fn collect_current_paths_bounded(
    root: &Path,
    exclusions: &[String],
    max_entries: usize,
    max_depth: usize,
    files: &mut Vec<String>,
    unverifiable: &mut Vec<String>,
    scanned_entries: &mut usize,
) -> bool {
    let mut pending = std::collections::VecDeque::from([(root.to_path_buf(), 0_usize)]);
    let mut scanned = 0_usize;
    let mut complete = true;
    while let Some((directory, depth)) = pending.pop_front() {
        let remaining = max_entries.saturating_sub(scanned);
        if remaining == 0 {
            return false;
        }
        let mut entries = Vec::new();
        let read = match fs::read_dir(&directory) {
            Ok(read) => read,
            Err(_) => {
                unverifiable.push(relative_display(root, &directory));
                complete = false;
                continue;
            }
        };
        for entry in read {
            match entry {
                Ok(entry) => entries.push(entry.path()),
                Err(_) => {
                    unverifiable.push(relative_display(root, &directory));
                    complete = false;
                }
            }
            if entries.len() > remaining {
                return false;
            }
        }
        entries.sort();
        scanned = scanned.saturating_add(entries.len());
        *scanned_entries = scanned;
        for path in entries {
            let relative = relative_display(root, &path);
            if path_is_excluded(&relative, exclusions) {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    unverifiable.push(relative);
                    complete = false;
                    continue;
                }
            };
            if metadata.file_type().is_symlink() {
                unverifiable.push(relative);
                complete = false;
            } else if metadata.is_dir() {
                if depth >= max_depth {
                    unverifiable.push(relative);
                    complete = false;
                } else {
                    pending.push_back((path, depth + 1));
                }
            } else if metadata.is_file() {
                files.push(relative);
            } else {
                unverifiable.push(relative);
                complete = false;
            }
        }
    }
    files.sort();
    complete
}

fn relative_display(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let displayed = relative.to_string_lossy().replace('\\', "/");
    if displayed.is_empty() {
        ".".to_owned()
    } else {
        displayed
    }
}

fn path_is_excluded(relative: &str, exclusions: &[String]) -> bool {
    exclusions.iter().any(|exclusion| {
        let normalized = exclusion.trim_matches('/');
        !normalized.is_empty()
            && (relative == normalized
                || relative.starts_with(&format!("{normalized}/"))
                || (!normalized.contains('/')
                    && relative.split('/').any(|part| part == normalized)))
    })
}
