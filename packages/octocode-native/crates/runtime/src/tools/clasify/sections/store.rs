//! Retained assessment plans: the cache store and the process and
//! transaction locks that serialize them.
use super::{Session, unavailable};
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::execution::ExecutionContext;
use crate::tools::judgment::ClassificationError;
use octocode_io::flight::KeyedFlight;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Runtime-owned retention; policy/access identities are supplied by the same
/// owner as display snapshots, never reconstructed from environment here.
pub(crate) struct Sessions {
    pub(super) store: Store<Session>,
    pub(super) flights: KeyedFlight<String, ()>,
    policy: String,
    pub(super) max_bytes: usize,
    disk: Option<PathBuf>,
}

impl Sessions {
    pub(crate) fn new(config: CacheConfig, disk: Option<PathBuf>, policy: String) -> Self {
        let max_bytes = config.max_bytes;
        Self {
            store: Store::new(config, disk.clone()),
            flights: KeyedFlight::transient(),
            policy,
            max_bytes,
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
        std::fs::create_dir_all(disk)
            .map_err(|_| unavailable("The assessment storage directory is unavailable."))?;
        loop {
            let mut options = std::fs::OpenOptions::new();
            options.create(true).truncate(false).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            let file = options
                .open(disk.join(name))
                .map_err(|_| unavailable("The assessment transaction lock is unavailable."))?;
            let wait = context
                .deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100));
            match octocode_io::lock::lock_within(file, wait, Duration::from_millis(10)) {
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
        let digest = crate::digest::json_sha256(&json!([key.namespace, key.scope, key.resource]));
        // Stable lock files remain after expiry: unlinking one can split waiting
        // processes across different inodes and let both mutate the same plan.
        self.process_lock(&format!("assessment-{digest}.lock"), context)
    }

    pub(super) fn current(
        &self,
        key: &CacheKey,
        context: &ExecutionContext,
    ) -> Result<Option<Session>, ClassificationError> {
        let _storage = self.process_lock("assessment.lock", context)?;
        Ok(self
            .store
            .get_current(key)
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
            .put_confirmed(
                key,
                session,
                bytes,
                CacheClass::Volatile {
                    ttl: crate::tools::judgment::retention(),
                },
            )
            .1
    }
}
