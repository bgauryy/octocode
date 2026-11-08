//! Retained assessment plans: the cache store and the process and
//! transaction locks that serialize them.
use super::{Session, unavailable};
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::execution::ExecutionContext;
use crate::tools::judgment::ClassificationError;
use octocode_io::flight::KeyedFlight;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Frozen assessment plans, and the cursors and saved pages that name them,
/// remain reachable for thirty minutes after their last use: an agent reads a
/// page's hits before asking for the next, and a restart would bill the plan
/// again. Idle, so every read restarts the lifetime each page states as
/// `now + 30 min`.
pub(crate) const PLAN_RETENTION: CacheClass = CacheClass::Idle { ttl: PLAN_IDLE };

/// [`PLAN_RETENTION`]'s lifetime after the last use.
pub(crate) const PLAN_IDLE: Duration = Duration::from_secs(30 * 60);

/// Runtime-owned retention; policy/access identities are supplied by the same
/// owner as display snapshots, never reconstructed from environment here.
pub(crate) struct Sessions {
    pub(super) store: Store<Session>,
    pub(super) flights: KeyedFlight<String, ()>,
    policy: String,
    disk: Option<PathBuf>,
}

impl Sessions {
    pub(crate) fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        Self {
            store: Store::new(config, disk.clone()),
            flights: KeyedFlight::transient(),
            policy,
            disk,
        }
    }

    pub(crate) fn clear_memory(&self) {
        self.store.clear_memory();
    }

    fn process_lock(
        &self,
        name: &str,
        context: &ExecutionContext,
    ) -> Result<Option<octocode_io::lock::LockedFile>, ClassificationError> {
        let Some(disk) = &self.disk else {
            return Ok(None);
        };
        octocode_io::atomic::create_private_dir_all(disk)
            .map_err(|_| unavailable("The assessment storage directory is unavailable."))?;
        let path = disk.join(name);
        loop {
            let wait = crate::execution::cancel::deadline_of(context)
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            match octocode_io::lock::lock_path_within(&path, wait, octocode_io::lock::LOCK_POLL) {
                Ok(lock) => return Ok(Some(lock)),
                Err(octocode_io::lock::LockError::Busy) => {
                    context.check().map_err(|_| unavailable("The request stopped before the assessment transaction lock was acquired."))?;
                }
                Err(octocode_io::lock::LockError::Io(_)) => {
                    return Err(unavailable(
                        "The assessment transaction lock could not be acquired.",
                    ));
                }
            }
        }
    }

    pub(super) fn key(&self, access: &[u8; 32], id: &str) -> CacheKey {
        CacheKey {
            namespace: "section-assessment-v2".into(),
            scope: format!("{}:{}", self.policy, hex::encode(access)),
            resource: id.into(),
        }
    }

    pub(super) fn transaction_lock(
        &self,
        key: &CacheKey,
        context: &ExecutionContext,
    ) -> Result<Option<octocode_io::lock::LockedFile>, ClassificationError> {
        // A plan's lock file outlives it only until a scan of the store finds
        // that, and removes it unheld; a locker checks what it locked.
        let Some(name) = self
            .store
            .entry_lock(key)
            .and_then(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
        else {
            return Ok(None);
        };
        self.process_lock(&name, context)
    }

    pub(super) fn current(
        &self,
        key: &CacheKey,
        context: &ExecutionContext,
    ) -> Result<Option<Session>, ClassificationError> {
        let _storage = self.process_lock("assessment.lock", context)?;
        Ok(self
            .store
            .get(key)
            .map(|saved| saved.value.as_ref().clone()))
    }

    pub(super) fn remove(&self, key: &CacheKey, context: &ExecutionContext) {
        if let Ok(_storage) = self.process_lock("assessment.lock", context) {
            self.store.remove(key);
        }
    }

    pub(super) fn save(&self, key: CacheKey, session: Session, context: &ExecutionContext) -> bool {
        let Ok(_storage) = self.process_lock("assessment.lock", context) else {
            return false;
        };
        let bytes = octocode_engine::json_size::bytes(&session).unwrap_or(usize::MAX);
        self.store
            .put_until(
                crate::execution::cancel::deadline_of(context),
                key,
                session,
                bytes,
                PLAN_RETENTION,
            )
            .1
    }
}
