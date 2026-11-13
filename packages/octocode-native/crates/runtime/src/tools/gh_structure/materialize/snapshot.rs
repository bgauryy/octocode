//! Content-verified materialization snapshots and their filesystem transaction.
use super::writer::SkippedFile;
use super::{EntryKind, MaterializeSkips, Snapshot, TreeEntry, materialize_root, write_error};
use crate::private_file::{BeneathDir, read_limited};
use octocode_github::{ProviderError, ProviderErrorKind, RequestContext};
use octocode_io::lock::{LockError, LockedFile, lock_within};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const FORMAT: &str = "octocode-materialize/v4";
const MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::tools::gh_structure) struct FileIdentity {
    pub(super) bytes: u64,
    sha256: String,
}

impl FileIdentity {
    pub(super) fn of(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.len() as u64,
            sha256: octocode_engine::digest::sha256(bytes),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    owner: String,
    repo: String,
    commit: String,
    files: BTreeMap<String, FileIdentity>,
    skipped: BTreeMap<String, SkippedFile>,
}

pub(super) struct Store {
    pub(super) root: PathBuf,
    parent: Arc<BeneathDir>,
    manifest_name: OsString,
    manifest: Manifest,
    _lease: LockedFile,
}

impl Store {
    pub(super) async fn open(
        snapshot: &Snapshot<'_>,
        context: &RequestContext,
    ) -> Result<Self, ProviderError> {
        context.budget.check()?;
        for segment in [snapshot.owner, snapshot.repo, snapshot.commit] {
            if !matches!(Path::new(segment).components().next(), Some(Component::Normal(name)) if name == segment)
            {
                return Err(ProviderError::new(
                    ProviderErrorKind::Validation,
                    "Invalid materialize snapshot path segment",
                ));
            }
        }
        let home = snapshot.root.to_path_buf();
        let parent_path = materialize_root(&home)
            .join(snapshot.owner)
            .join(snapshot.repo);
        let parent = Arc::new(directory(&home, &parent_path).await?);
        let lock_name = OsString::from(format!("{}.lock", snapshot.commit));
        let lease = loop {
            context.budget.check()?;
            let parent = parent.clone();
            let name = lock_name.clone();
            let result = tokio::task::spawn_blocking(move || {
                let file = parent.open_lock(&name).map_err(|error| {
                    LockError::Io(io::Error::new(
                        error.kind(),
                        format!("open snapshot lock: {error}"),
                    ))
                })?;
                lock_within(file, Duration::ZERO, Duration::ZERO)
            })
            .await
            .map_err(worker_error)?;
            match result {
                Ok(lease) => break lease,
                Err(LockError::Io(error)) => {
                    return Err(write_error("lock materialize snapshot", &error));
                }
                Err(LockError::Busy) => {
                    context
                        .budget
                        .wait(tokio::time::sleep(Duration::from_millis(20)))
                        .await?;
                }
            }
        };
        let root = parent_path.join(snapshot.commit);
        let manifest_name = OsString::from(format!("{}.manifest.json", snapshot.commit));
        let reader = parent.clone();
        let name = manifest_name.clone();
        let loaded = blocking("read materialize manifest", move || {
            match reader.open_file(&name) {
                Ok(file) => read_limited(file, MANIFEST_BYTES).map(Some),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        })
        .await?;
        let has_manifest = loaded.is_some();
        let manifest = match loaded {
            Some(bytes) => {
                let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|error| {
                    storage_error(format!("Invalid materialize manifest: {error}"))
                })?;
                if manifest.format != FORMAT
                    || manifest.owner != snapshot.owner
                    || manifest.repo != snapshot.repo
                    || manifest.commit != snapshot.commit
                {
                    return Err(storage_error(
                        "Materialize manifest identity does not match its snapshot",
                    ));
                }
                manifest
            }
            None => Manifest {
                format: FORMAT.into(),
                owner: snapshot.owner.into(),
                repo: snapshot.repo.into(),
                commit: snapshot.commit.into(),
                files: BTreeMap::new(),
                skipped: BTreeMap::new(),
            },
        };
        directory(&home, &root).await?;
        if !has_manifest {
            let unchecked = root.clone();
            let nonempty = blocking("inspect unrecorded materialize directory", move || {
                Ok(std::fs::read_dir(unchecked)?.next().is_some())
            })
            .await?;
            if nonempty {
                return Err(storage_error(
                    "Materialize directory has files but no identity manifest; use a fresh output directory",
                ));
            }
        }
        let store = Self {
            root,
            parent,
            manifest_name,
            manifest,
            _lease: lease,
        };
        if !has_manifest {
            store.save().await?;
        }
        Ok(store)
    }

    /// The identity of `relative` when this snapshot already holds it: the
    /// manifest records it and the file on disk still has exactly those
    /// bytes. A repeat materialize keeps that copy instead of reading the
    /// file again, so the snapshot, not a saved provider answer, owns each
    /// body.
    pub(super) async fn kept(&self, home: &Path, relative: &str) -> Option<FileIdentity> {
        let identity = self.manifest.files.get(relative)?.clone();
        let (home, file) = (home.to_path_buf(), self.root.join(relative));
        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let directory = BeneathDir::open(&home, file.parent()?).ok()?;
            let file = directory.open_file(file.file_name()?).ok()?;
            let (bytes, sha256) = octocode_engine::digest::copy_hashed(
                file.take(identity.bytes.saturating_add(1)),
                io::sink(),
            )
            .ok()?;
            (bytes == identity.bytes && sha256 == identity.sha256).then_some(identity)
        })
        .await
        .ok()
        .flatten()
    }

    /// Skips before a saved cursor, restricted to this selection. A snapshot
    /// may also hold outcomes from materializing another path or filter.
    pub(super) fn prior_skips(&self, entries: &[TreeEntry], read_limit: usize) -> MaterializeSkips {
        let mut skips = MaterializeSkips::new(read_limit);
        for entry in entries {
            if let Some(skipped) = self.manifest.skipped.get(&entry.path) {
                skips.restore(&entry.path, skipped);
            }
        }
        skips
    }

    /// A cursor only advances past prior files that this snapshot still
    /// verifies, or an explicitly recorded skip. A new, edited or incomplete
    /// snapshot resumes from its first missing identity instead.
    pub(super) async fn resume_offset(
        &self,
        home: &Path,
        entries: &[TreeEntry],
        requested: usize,
        context: &RequestContext,
    ) -> Result<usize, ProviderError> {
        for (index, entry) in entries[..requested].iter().enumerate() {
            context.budget.check()?;
            if entry.kind == EntryKind::File
                && !self.manifest.skipped.contains_key(&entry.path)
                && self.kept(home, &entry.path).await.is_none()
            {
                return Ok(index);
            }
        }
        Ok(requested)
    }

    pub(super) async fn record(
        &mut self,
        files: Vec<(String, FileIdentity)>,
        skips: &MaterializeSkips,
    ) -> Result<(), ProviderError> {
        let skipped = skips.recorded();
        if !files.is_empty() || !skipped.is_empty() {
            for (path, _) in &files {
                self.manifest.skipped.remove(path);
            }
            self.manifest.files.extend(files);
            self.manifest.skipped.extend(skipped);
            self.save().await?;
        }
        Ok(())
    }

    async fn save(&self) -> Result<(), ProviderError> {
        let bytes =
            serde_json::to_vec(&self.manifest).map_err(|error| storage_error(error.to_string()))?;
        let (parent, name) = (self.parent.clone(), self.manifest_name.clone());
        blocking("write materialize manifest", move || {
            parent.write_atomic(&name, &bytes)
        })
        .await
    }
}

pub(super) async fn write_file(
    home: &Path,
    root: &Path,
    relative: &str,
    bytes: &[u8],
) -> Result<(), ProviderError> {
    let (home, root, relative, bytes) = (
        home.to_path_buf(),
        root.to_path_buf(),
        PathBuf::from(relative),
        bytes.to_vec(),
    );
    blocking("write materialized file", move || {
        let dest = root.join(relative);
        let parent = dest
            .parent()
            .ok_or_else(|| io::Error::other("File has no parent"))?;
        let directory = BeneathDir::create_all(&home, parent)?;
        let name = dest
            .file_name()
            .ok_or_else(|| io::Error::other("File has no name"))?;
        // A file already on disk with exactly these bytes is not rewritten;
        // any other content (edited, truncated, replaced) is restored.
        if holds_exactly(&directory, name, &bytes) {
            return Ok(());
        }
        directory.write_atomic(name, &bytes)?;
        if !directory.still_at(&home, parent)? {
            return Err(io::Error::other("Materialize directory moved during write"));
        }
        Ok(())
    })
    .await
}

/// Whether the file `name` in `directory` is there and holds exactly `bytes`.
fn holds_exactly(directory: &BeneathDir, name: &std::ffi::OsStr, bytes: &[u8]) -> bool {
    use std::io::Read;
    let Ok(mut file) = directory.open_file(name) else {
        return false;
    };
    let mut block = vec![0_u8; bytes.len().clamp(1, 64 * 1024)];
    for expected in bytes.chunks(block.len()) {
        let found = &mut block[..expected.len()];
        if file.read_exact(found).is_err() || found != expected {
            return false;
        }
    }
    // One byte past the expected length proves a longer file without
    // reading all of it. Interrupted reads do not change its identity.
    loop {
        match file.read(&mut block[..1]) {
            Ok(read) => return read == 0,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        }
    }
}

async fn directory(home: &Path, path: &Path) -> Result<BeneathDir, ProviderError> {
    let (home, path) = (home.to_path_buf(), path.to_path_buf());
    blocking("create materialize directory", move || {
        BeneathDir::create_all(&home, &path)
    })
    .await
}

async fn blocking<T: Send + 'static>(
    action: &'static str,
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> Result<T, ProviderError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(worker_error)?
        .map_err(|error| write_error(action, &error))
}

fn worker_error(error: tokio::task::JoinError) -> ProviderError {
    storage_error(format!("Materialize filesystem worker failed: {error}"))
}
fn storage_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Configuration, message)
}
