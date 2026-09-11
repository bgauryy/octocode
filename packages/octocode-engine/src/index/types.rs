use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::{IndexError, Result};

pub const INDEX_LAYOUT_VERSION: u32 = 2;

#[derive(Clone, Debug)]
pub struct IndexConfig {
    pub expected_schema_version: u32,
    pub expected_parser_schema_version: u32,
    pub expected_tool_version: String,
    pub max_generations: usize,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourceIdentity {
    MutableWorkspace,
    ImmutableGit { commit: String, tree: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootIdentity {
    pub root_id: String,
    pub canonical_root: PathBuf,
    pub source: SourceIdentity,
}

impl RootIdentity {
    pub fn mutable(root_id: &str, root: &Path) -> Result<Self> {
        Self::new(root_id, root, SourceIdentity::MutableWorkspace)
    }

    pub fn immutable_git(root_id: &str, root: &Path, commit: &str, tree: &str) -> Result<Self> {
        if commit.is_empty() || tree.is_empty() {
            return Err(IndexError::InvalidSourceIdentity(
                "git commit and tree identities must be non-empty".to_owned(),
            ));
        }
        Self::new(
            root_id,
            root,
            SourceIdentity::ImmutableGit {
                commit: commit.to_owned(),
                tree: tree.to_owned(),
            },
        )
    }

    fn new(root_id: &str, root: &Path, source: SourceIdentity) -> Result<Self> {
        validate_root_id(root_id)?;
        let canonical_root = root.canonicalize().map_err(|source| IndexError::Io {
            operation: "canonicalize source root",
            path: root.to_path_buf(),
            source,
        })?;
        Ok(Self {
            root_id: root_id.to_owned(),
            canonical_root,
            source,
        })
    }
}

pub(crate) fn validate_root_id(root_id: &str) -> Result<()> {
    let valid = !root_id.is_empty()
        && root_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(IndexError::InvalidRootId(root_id.to_owned()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationSpec {
    pub root: RootIdentity,
    pub index_schema_version: u32,
    pub parser_schema_version: u32,
    pub tool_version: String,
    pub exclusions: Vec<String>,
    pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFileIdentity {
    pub size: u64,
    pub modified_nanos: u128,
    pub content_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRecord {
    pub name: String,
    pub kind: String,
    pub start_byte: u64,
    pub end_byte: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentRecord {
    pub path: String,
    pub language: String,
    pub content: String,
    pub identity: SourceFileIdentity,
    pub symbols: Vec<SymbolRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphFactSidecarRef {
    pub digest: String,
    pub schema_version: u32,
    pub payload_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationManifest {
    pub layout_version: u32,
    pub generation: u64,
    pub root: RootIdentity,
    pub index_schema_version: u32,
    pub parser_schema_version: u32,
    pub tool_version: String,
    pub exclusions: Vec<String>,
    pub complete: bool,
    pub document_count: u64,
    pub indexed_source_bytes: u64,
    pub documents_digest: String,
    pub graph_fact_sidecars: Vec<GraphFactSidecarRef>,
    /// SHA-256 of the canonical manifest with this field set to the empty string.
    pub manifest_digest: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FreshnessReport {
    pub checked: usize,
    pub fresh: usize,
    pub generation_complete: bool,
    pub added: Vec<String>,
    pub dirty: Vec<String>,
    pub deleted: Vec<String>,
    pub unverifiable: Vec<String>,
}

impl FreshnessReport {
    #[must_use]
    pub fn fresh(count: usize) -> Self {
        Self {
            checked: count,
            fresh: count,
            generation_complete: true,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn can_prove_absence(&self) -> bool {
        self.generation_complete
            && self.checked == self.fresh
            && self.added.is_empty()
            && self.dirty.is_empty()
            && self.deleted.is_empty()
            && self.unverifiable.is_empty()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GraphFactEnvelope {
    pub layout_version: u32,
    pub schema_version: u32,
    pub payload: Vec<u8>,
}
