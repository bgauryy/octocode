//! Provider cache partitions are minted by the transport after credential pinning.
use crate::cache::{BoundedCache, CacheConfig, CacheKey, CacheLookup, CachePartition};
use crate::providers::github::{
    CachePartition as ProviderPartition, CachedContent, ConditionalCache,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub(super) struct GitHubContentCache {
    cache: Arc<Mutex<BoundedCache<CachedContent>>>,
    revision: u64,
    disk: Option<PathBuf>,
    /// Freshness window mirrored from the memory tier so disk reads expire too.
    ttl: Duration,
    /// Upper bound on persisted disk entries (LRU-by-mtime pruned on write).
    max_disk_entries: usize,
}

/// On-disk envelope written for serialization (borrows the value to avoid a clone).
#[derive(serde::Serialize)]
struct DiskEntryRef<'a> {
    revision: u64,
    stored_at_unix: u64,
    value: &'a CachedContent,
}

/// On-disk envelope read back: the cached value plus the freshness metadata the
/// memory tier keeps in RAM, so disk reads honor the same TTL and
/// config-revision invalidation instead of serving entries indefinitely.
#[derive(serde::Deserialize)]
struct DiskEntry {
    revision: u64,
    stored_at_unix: u64,
    value: CachedContent,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

impl GitHubContentCache {
    pub fn clear(&self) {
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .invalidate_all();
        if let Some(disk) = &self.disk {
            let _ = fs::remove_dir_all(disk);
            let _ = fs::create_dir_all(disk);
        }
    }
    pub fn new(config: CacheConfig, revision: u64, disk: Option<PathBuf>) -> Self {
        if let Some(disk) = &disk {
            let _ = fs::create_dir_all(disk);
        }
        let ttl = config.ttl;
        let max_disk_entries = config.max_entries;
        Self {
            cache: Arc::new(Mutex::new(BoundedCache::new(config))),
            revision,
            disk,
            ttl,
            max_disk_entries,
        }
    }

    fn disk_file(&self, resource: &str) -> Option<PathBuf> {
        self.disk.as_ref().map(|dir| {
            let mut digest = Sha256::new();
            digest.update(resource.as_bytes());
            dir.join(format!("{}.json", hex::encode(digest.finalize())))
        })
    }

    fn read_disk(&self, resource: &str) -> Option<CachedContent> {
        let path = self.disk_file(resource)?;
        let bytes = fs::read(&path).ok()?;
        let entry: DiskEntry = serde_json::from_slice(&bytes).ok()?;
        // Honor config-revision invalidation and the TTL exactly like the memory
        // tier; a stale or wrong-revision entry is dropped (and its file removed)
        // rather than served. Without this a disk-persisted git-tree could be
        // returned indefinitely for a moving branch head.
        let fresh = entry.revision == self.revision
            && now_unix().saturating_sub(entry.stored_at_unix) <= self.ttl.as_secs();
        if !fresh {
            let _ = fs::remove_file(&path);
            return None;
        }
        Some(entry.value)
    }

    fn write_disk(&self, resource: &str, value: &CachedContent) {
        let Some(path) = self.disk_file(resource) else {
            return;
        };
        let entry = DiskEntryRef {
            revision: self.revision,
            stored_at_unix: now_unix(),
            value,
        };
        if let Ok(bytes) = serde_json::to_vec(&entry) {
            let _ = fs::write(path, bytes);
        }
        self.prune_disk();
    }

    /// Bound the disk tier: keep at most `max_disk_entries` files, evicting the
    /// oldest by mtime. The `BoundedCache` budget governs memory only, so without
    /// this the on-disk directory would grow without bound.
    fn prune_disk(&self) {
        let Some(dir) = &self.disk else {
            return;
        };
        let Ok(read_dir) = fs::read_dir(dir) else {
            return;
        };
        let mut files: Vec<(SystemTime, PathBuf)> = read_dir
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let mtime = entry.metadata().ok()?.modified().ok()?;
                Some((mtime, path))
            })
            .collect();
        if files.len() <= self.max_disk_entries {
            return;
        }
        files.sort_by_key(|(mtime, _)| *mtime);
        let excess = files.len().saturating_sub(self.max_disk_entries);
        for (_, path) in files.into_iter().take(excess) {
            let _ = fs::remove_file(path);
        }
    }

    fn key(partition: &ProviderPartition, resource: &str) -> CacheKey {
        CacheKey {
            namespace: "github-content".into(),
            resource: resource.into(),
            partition: CachePartition {
                endpoint: "provider-partition-v1".into(),
                credential_fingerprint: partition.0.clone(),
            },
        }
    }
}

impl ConditionalCache for GitHubContentCache {
    fn get<'a>(
        &'a self,
        partition: &'a ProviderPartition,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CachedContent>> + Send + 'a>> {
        Box::pin(async move {
            match self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(
                &Self::key(partition, key),
                self.revision,
                None,
                Instant::now(),
            ) {
                CacheLookup::Hit { value, .. } => Some((*value).clone()),
                CacheLookup::Miss(_) => self.read_disk(key),
            }
        })
    }

    fn put<'a>(
        &'a self,
        partition: &'a ProviderPartition,
        key: String,
        value: CachedContent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let key = Self::key(partition, &key);
            let bytes = value
                .bytes
                .capacity()
                .saturating_add(value.resolved_ref.capacity())
                .saturating_add(value.etag.as_ref().map_or(0, String::capacity))
                .saturating_add(key.resource.capacity())
                .saturating_add(key.partition.credential_fingerprint.capacity())
                .saturating_add(std::mem::size_of::<CachedContent>());
            self.write_disk(&key.resource, &value);
            self.cache.lock().unwrap_or_else(|p| p.into_inner()).insert(
                key,
                value,
                bytes,
                self.revision,
                Instant::now(),
            );
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn refuses_oversized_bodies_and_never_crosses_provider_partitions() {
        let cache = GitHubContentCache::new(
            CacheConfig {
                max_bytes: 1024,
                ..Default::default()
            },
            7,
            None,
        );
        let one = ProviderPartition("endpoint/credential/one".into());
        let two = ProviderPartition("endpoint/credential/two".into());
        let content = CachedContent {
            bytes: b"private source".to_vec(),
            etag: Some("v1".into()),
            resolved_ref: "sha".into(),
        };
        cache.put(&one, "file".into(), content.clone()).await;
        assert_eq!(cache.get(&one, "file").await, Some(content));
        assert_eq!(cache.get(&two, "file").await, None);
        cache
            .put(
                &one,
                "large".into(),
                CachedContent {
                    bytes: vec![0; 2048],
                    etag: None,
                    resolved_ref: "sha".into(),
                },
            )
            .await;
        assert_eq!(cache.get(&one, "large").await, None);
    }

    #[tokio::test]
    async fn disk_reads_round_trip_but_reject_a_newer_config_revision() {
        let dir =
            std::env::temp_dir().join(format!("octocode-ghcache-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let part = ProviderPartition("endpoint/credential".into());
        let content = CachedContent {
            bytes: b"tree-body".to_vec(),
            etag: None,
            resolved_ref: "sha".into(),
        };

        // Persist at revision 7, then read back through a fresh cache (memory
        // miss → disk) at the same revision: the disk entry is served.
        GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.clone()))
            .put(&part, "resource".into(), content.clone())
            .await;
        let same_revision = GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.clone()));
        assert_eq!(same_revision.get(&part, "resource").await, Some(content));

        // A newer config revision must not serve the stale disk entry.
        let newer_revision = GitHubContentCache::new(CacheConfig::default(), 8, Some(dir.clone()));
        assert_eq!(newer_revision.get(&part, "resource").await, None);

        let _ = fs::remove_dir_all(&dir);
    }
}
