//! Issued handles for saved tool work and immutable response pages.
//! A handle selects a member of one retained set. It never contains caller-editable progress.
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::cursor::Cursor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PREFIX: &str = "cursor1.";
const TTL: Duration = Duration::from_secs(120);

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum SavedWork {
    ToolResume { index: Option<usize> },
    ResponsePage(Cursor),
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Member {
    pub tool: String,
    /// Complete normalized input, including contract-typed private progress.
    pub input: Value,
    pub work: SavedWork,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct CursorSet {
    access: [u8; 32],
    pub github_identity: Option<String>,
    members: BTreeMap<String, Member>,
}

pub(super) struct Registry {
    store: Store<CursorSet>,
    policy: String,
    disk: Option<PathBuf>,
    mutation: Mutex<()>,
}

pub(super) struct Pending {
    id: String,
    set: CursorSet,
}

pub(super) struct Opened {
    set: Arc<CursorSet>,
    member: String,
    id: String,
}

impl Opened {
    pub fn pending(&self) -> Pending {
        Pending {
            id: self.id.clone(),
            set: (*self.set).clone(),
        }
    }
    pub fn member(&self) -> &Member {
        &self.set.members[&self.member]
    }
    pub fn github_identity(&self) -> Option<&str> {
        self.set.github_identity.as_deref()
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

    fn key(&self, id: &str) -> CacheKey {
        CacheKey {
            namespace: "issued-continuations-v1".into(),
            scope: self.policy.clone(),
            resource: id.into(),
        }
    }

    pub fn clear_memory(&self) {
        self.store.clear_memory();
    }

    /// Every handle in one output becomes reachable atomically, before publication.
    pub fn retain(
        &self,
        mut pending: Pending,
        context: &ExecutionContext,
    ) -> Result<(), ExecutionError> {
        if pending.set.members.is_empty() {
            return Ok(());
        }
        let _storage = self.process_lock(context)?;
        let _mutation = self.mutation_lock(context)?;
        context.check()?;
        let key = self.key(&pending.id);
        if let Some(current) = self.store.get_current(&key) {
            if current.value.access != pending.set.access
                || current.value.github_identity != pending.set.github_identity
            {
                return Err(ExecutionError::WorkerFailed);
            }
            // Two callers may extend the same earlier page. Keep every handle
            // already published by either branch before publishing this one.
            for (id, member) in &current.value.members {
                pending
                    .set
                    .members
                    .entry(id.clone())
                    .or_insert_with(|| member.clone());
            }
        }
        let bytes = octocode_engine::json_size::bytes(&pending.set).unwrap_or(usize::MAX);
        let saved = self
            .store
            .put_confirmed(key, pending.set, bytes, CacheClass::Volatile { ttl: TTL })
            .1;
        if saved {
            Ok(())
        } else {
            Err(ExecutionError::WorkerFailed)
        }
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

    pub fn open(&self, tool: ToolId, token: &str, access: [u8; 32]) -> Option<Opened> {
        let encoded = token.strip_prefix(PREFIX)?;
        if encoded.len() != 64 || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let (id, member) = encoded.split_at(32);
        let set = self.store.get_current(&self.key(id))?.value;
        if set.access != access || set.members.get(member)?.tool != tool.as_str() {
            return None;
        }
        Some(Opened {
            set,
            member: member.into(),
            id: id.into(),
        })
    }
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        self.set.members.is_empty()
    }

    pub fn bind_github_identity(&mut self, identity: Option<String>) {
        self.set.github_identity = identity;
    }

    pub fn new(access: [u8; 32], github_identity: Option<String>) -> Option<Self> {
        Some(Self {
            id: random_id()?,
            set: CursorSet {
                access,
                github_identity,
                members: BTreeMap::new(),
            },
        })
    }

    pub fn insert(&mut self, tool: ToolId, input: Value, work: SavedWork) -> Option<String> {
        let id = random_id()?;
        let token = format!("{PREFIX}{}{id}", self.id);
        self.set.members.insert(
            id,
            Member {
                tool: tool.as_str().into(),
                input,
                work,
            },
        );
        Some(token)
    }
}

fn random_id() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(hex::encode(bytes))
}

#[cfg(test)]
mod tests;
