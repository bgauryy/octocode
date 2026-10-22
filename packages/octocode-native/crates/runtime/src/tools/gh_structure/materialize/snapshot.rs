//! Content-verified materialization snapshots and their filesystem transaction.
use super::{MATERIALIZE_FILE_BYTES, Snapshot, TreeEntry, materialize_root, write_error};
use crate::private_file::{BeneathDir, open_beneath, read_limited};
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

const FORMAT: &str = "octocode-materialize/v3";
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
}

pub(super) struct Store {
    pub(super) root: PathBuf,
    home: PathBuf,
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
                    "Materialize directory has files but no identity manifest; use a fresh cache directory",
                ));
            }
        }
        let store = Self {
            root,
            home,
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

    pub(super) async fn reusable(&self, entry: &TreeEntry) -> bool {
        let Some(expected) = self
            .manifest
            .files
            .get(&entry.path)
            .filter(|identity| Some(identity.bytes) == entry.size)
            .cloned()
        else {
            return false;
        };
        if !crate::tools::gh_shared::is_repo_relative(&entry.path) {
            return false;
        }
        let (home, path) = (self.home.clone(), self.root.join(&entry.path));
        tokio::task::spawn_blocking(move || {
            let file = open_beneath(&home, &path)?;
            if file.metadata()?.len() != expected.bytes {
                return Ok(false);
            }
            let bytes = read_limited(file, MATERIALIZE_FILE_BYTES as u64)?;
            Ok::<_, io::Error>(FileIdentity::of(&bytes) == expected)
        })
        .await
        .is_ok_and(|result| result.is_ok_and(|same| same))
    }

    pub(super) async fn record(
        &mut self,
        files: Vec<(String, FileIdentity)>,
    ) -> Result<(), ProviderError> {
        if !files.is_empty() {
            self.manifest.files.extend(files);
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
        directory.write_atomic(name, &bytes)?;
        if !directory.still_at(&home, parent)? {
            return Err(io::Error::other("Materialize directory moved during write"));
        }
        Ok(())
    })
    .await
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
