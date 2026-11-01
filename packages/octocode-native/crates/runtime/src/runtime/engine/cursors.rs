//! Issued handles for saved tool work and immutable response pages.
//! A handle names one retained continuation by its content ([`handle`]). It
//! never contains caller-editable progress.
use super::page_replay::Refusal;
use crate::cache::{CacheConfig, CacheKey, Store};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::cursor::{Cursor, issued};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest as _;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum SavedWork {
    ToolResume {
        index: Option<usize>,
    },
    /// A later page of a saved response, and the query index its rows
    /// inherit, so a replay that is no longer retained re-executes the
    /// same rows under the same indices ([`Cursor::key`] checks both).
    ResponsePage {
        cursor: Cursor,
        index: Option<usize>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Member {
    pub tool: String,
    /// Complete normalized input, including contract-typed private progress.
    pub input: Value,
    pub work: SavedWork,
}

/// One issued continuation as the registry keeps it: the access identity
/// it was issued to and the work it resumes.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Issued {
    access: [u8; 32],
    pub github_identity: Option<String>,
    member: Member,
}

pub(super) struct Registry {
    store: Store<Issued>,
    policy: String,
    disk: Option<PathBuf>,
    mutation: Mutex<()>,
}

/// The continuations one output issues, retained together before it is
/// published.
pub(super) struct Pending {
    access: [u8; 32],
    github_identity: Option<String>,
    issued: BTreeMap<String, Member>,
}

pub(super) struct Opened {
    issued: Arc<Issued>,
}

impl Opened {
    pub fn member(&self) -> &Member {
        &self.issued.member
    }
    pub fn github_identity(&self) -> Option<&str> {
        self.issued.github_identity.as_deref()
    }
}

impl Registry {
    pub fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        Self {
            store: Store::new(config, disk.clone()),
            policy,
            disk,
            mutation: Mutex::new(()),
        }
    }

    fn key(&self, token: &str) -> CacheKey {
        CacheKey {
            namespace: "issued-continuations-v2".into(),
            scope: self.policy.clone(),
            resource: token.into(),
        }
    }

    pub fn clear_memory(&self) {
        self.store.clear_memory();
    }

    /// Every handle in one output becomes reachable atomically, before
    /// publication. A handle names its content ([`handle`]), so one an
    /// earlier output issued is the same continuation: retaining it again
    /// refreshes its retention.
    pub fn retain(
        &self,
        pending: Pending,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        if pending.issued.is_empty() {
            return Ok(());
        }
        let _storage = self.process_lock(context)?;
        let _mutation = self.mutation_lock(context)?;
        context.check()?;
        for (token, member) in pending.issued {
            let key = self.key(&token);
            if self
                .store
                .get_current(&key)
                .is_some_and(|current| current.value.access != pending.access)
            {
                return Err(ExecutionError::WorkerFailed);
            }
            let issued = Issued {
                access: pending.access,
                github_identity: pending.github_identity.clone(),
                member,
            };
            let bytes = octocode_engine::json_size::bytes(&issued).unwrap_or(usize::MAX);
            if !self
                .store
                .put_confirmed(key, issued, bytes, super::page_replay::RETENTION)
                .1
            {
                return Err(ExecutionError::WorkerFailed);
            }
        }
        Ok(())
    }

    fn mutation_lock(
        &self,
        context: &ExecutionContext,
    ) -> Result<std::sync::MutexGuard<'_, ()>, ExecutionError> {
        loop {
            context.check()?;
            match self.mutation.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err(ExecutionError::WorkerFailed);
                }
                Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(
                    octocode_io::lock::LOCK_POLL
                        .min(context.deadline.saturating_duration_since(Instant::now())),
                ),
            }
        }
    }

    fn process_lock(
        &self,
        context: &ExecutionContext,
    ) -> Result<Option<octocode_io::lock::LockedFile>, ExecutionError> {
        let Some(disk) = &self.disk else {
            return Ok(None);
        };
        octocode_io::atomic::create_private_dir_all(disk)
            .map_err(|_| ExecutionError::WorkerFailed)?;
        let file = octocode_io::lock::open_lock_file(&disk.join("continuations.lock"))
            .map_err(|_| ExecutionError::WorkerFailed)?;
        // One stable sidecar bounds lock files and serializes short merges.
        loop {
            context.check()?;
            let wait = context
                .deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            match octocode_io::lock::lock_within(
                file.try_clone().map_err(|_| ExecutionError::WorkerFailed)?,
                wait,
                octocode_io::lock::LOCK_POLL,
            ) {
                Ok(lock) => return Ok(Some(lock)),
                Err(octocode_io::lock::LockError::Busy) => {}
                Err(octocode_io::lock::LockError::Io(_)) => {
                    return Err(ExecutionError::WorkerFailed);
                }
            }
        }
    }

    /// The work `token` resumes for `tool` under `access`, or why it does
    /// not open here.
    pub fn open(&self, tool: ToolId, token: &str, access: [u8; 32]) -> Result<Opened, Refusal> {
        issued::parts(token).ok_or(Refusal::Malformed)?;
        let issued = self
            .store
            .get_current(&self.key(token))
            .ok_or(Refusal::NotRetained)?
            .value;
        if issued.access != access {
            return Err(Refusal::OtherAccess);
        }
        if issued.member.tool != tool.as_str() {
            return Err(Refusal::OtherTool);
        }
        Ok(Opened { issued })
    }
}

impl Pending {
    pub fn new(access: [u8; 32], github_identity: Option<String>) -> Self {
        Self {
            access,
            github_identity,
            issued: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.issued.is_empty()
    }

    pub fn bind_github_identity(&mut self, identity: Option<String>) {
        self.github_identity = identity;
    }

    /// The handle of `work` resuming `tool` with `input`, issued once per
    /// output; the same continuation always has the same handle
    /// ([`handle`]).
    pub fn insert(&mut self, tool: ToolId, input: Value, work: SavedWork) -> String {
        let token = handle(&self.access, tool, &input, &work);
        self.issued.entry(token.clone()).or_insert(Member {
            tool: tool.as_str().into(),
            input,
            work,
        });
        token
    }
}

/// The opaque handle of one continuation: a digest of the access identity
/// it is issued to (the call's credential scope), the tool, its complete
/// canonical input and the saved work (a response page's replay key). It
/// reveals none of them, and the same continuation issued by any output
/// within its retention has the same handle, so a reader that follows a
/// lead repeated on several pages reads it once.
fn handle(access: &[u8; 32], tool: ToolId, input: &Value, work: &SavedWork) -> String {
    use octocode_engine::digest::{Nulls, canonical_json};
    let identity = serde_json::json!([
        "issued-continuation",
        hex::encode(access),
        tool.as_str(),
        canonical_json(input.clone(), Nulls::Keep),
        canonical_json(serde_json::to_value(work).unwrap_or_default(), Nulls::Keep),
    ]);
    let digest = octocode_engine::digest::json_sha256(&identity)
        .unwrap_or_else(|| sha2::Sha256::digest([]).into());
    let (set, member) = digest.split_at(issued::ID_BYTES);
    let part = |bytes: &[u8]| {
        let mut id = [0; issued::ID_BYTES];
        id.copy_from_slice(bytes);
        issued::encode(&id)
    };
    issued::format(&part(set), &part(member))
}

#[cfg(test)]
mod tests;
