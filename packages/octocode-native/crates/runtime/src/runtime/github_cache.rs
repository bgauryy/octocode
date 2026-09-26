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
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
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
    /// Disk writes by this process; pruning runs on the first and then every
    /// [`PRUNE_EVERY_WRITES`] writes instead of scanning the directory per write.
    disk_writes: Arc<AtomicUsize>,
}

/// Directory scans are O(entries); amortize them across writes.
const PRUNE_EVERY_WRITES: usize = 64;
/// Cross-process throttle: one-shot CLI processes each write a few entries, so
/// a marker file bounds pruning to once per interval across processes.
const PRUNE_INTERVAL: Duration = Duration::from_secs(60);
const PRUNE_MARKER: &str = ".last-prune";

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
    pub fn clear_memory(&self) {
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .invalidate_all();
    }

    pub fn clear(&self) {
        self.clear_memory();
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
            disk_writes: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn disk_file(&self, key: &CacheKey) -> Option<PathBuf> {
        self.disk.as_ref().map(|dir| {
            let mut digest = Sha256::new();
            // Match memory isolation and leave legacy, unpartitioned files unread.
            digest.update(b"github-content-cache-v2");
            for value in [
                &key.namespace,
                &key.partition.endpoint,
                &key.partition.credential_fingerprint,
                &key.resource,
            ] {
                digest.update((value.len() as u64).to_le_bytes());
                digest.update(value.as_bytes());
            }
            dir.join(format!("{}.json", hex::encode(digest.finalize())))
        })
    }

    /// Returns the entry and its age so a promoted memory copy expires when the
    /// disk copy would.
    fn read_disk(&self, key: &CacheKey) -> Option<(CachedContent, Duration)> {
        let path = self.disk_file(key)?;
        let bytes = fs::read(&path).ok()?;
        let entry: DiskEntry = serde_json::from_slice(&bytes).ok()?;
        // Honor config-revision invalidation and the TTL exactly like the memory
        // tier; a stale or wrong-revision entry is dropped (and its file removed)
        // rather than served. Without this a disk-persisted git-tree could be
        // returned indefinitely for a moving branch head.
        let age = now_unix().saturating_sub(entry.stored_at_unix);
        let fresh = entry.revision == self.revision && age <= self.ttl.as_secs();
        if !fresh {
            let _ = fs::remove_file(&path);
            return None;
        }
        Some((entry.value, Duration::from_secs(age)))
    }

    fn write_disk(&self, key: &CacheKey, value: &CachedContent) {
        let Some(path) = self.disk_file(key) else {
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
        if self
            .disk_writes
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(PRUNE_EVERY_WRITES)
            && self.prune_due()
        {
            self.prune_disk();
        }
    }

    /// True when no process pruned within [`PRUNE_INTERVAL`]; claims the slot
    /// by touching the marker.
    fn prune_due(&self) -> bool {
        let Some(dir) = &self.disk else {
            return false;
        };
        let marker = dir.join(PRUNE_MARKER);
        let recent = fs::metadata(&marker)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|elapsed| elapsed < PRUNE_INTERVAL);
        if recent {
            return false;
        }
        let _ = fs::write(&marker, b"");
        true
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
                if path.extension().is_none_or(|ext| ext != "json") {
                    return None;
                }
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
                credential_fingerprint: partition.identity().to_owned(),
            },
        }
    }
}

/// `GitHubProvider::get_file_content` keys bodies as `github-content:<digest>`
/// over owner/repo/path and the resolved 40-hex commit SHA (never a branch
/// name), so those entries are immutable. Other namespaces (for example
/// `github-tree:` listings keyed by a movable ref) keep ETag revalidation.
fn is_commit_pinned_content(key: &str) -> bool {
    key.starts_with("github-content:")
}

fn entry_bytes(key: &CacheKey, value: &CachedContent) -> usize {
    value
        .bytes
        .capacity()
        .saturating_add(value.resolved_ref.capacity())
        .saturating_add(value.etag.as_ref().map_or(0, String::capacity))
        .saturating_add(key.resource.capacity())
        .saturating_add(key.partition.credential_fingerprint.capacity())
        .saturating_add(std::mem::size_of::<CachedContent>())
}

impl ConditionalCache for GitHubContentCache {
    fn get<'a>(
        &'a self,
        partition: &'a ProviderPartition,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<CachedContent>> + Send + 'a>> {
        Box::pin(async move {
            let immutable = is_commit_pinned_content(key);
            let key = Self::key(partition, key);
            let hit = self.cache.lock().unwrap_or_else(|p| p.into_inner()).get(
                &key,
                self.revision,
                None,
                Instant::now(),
            );
            let value = match hit {
                CacheLookup::Hit { value, .. } => Some((*value).clone()),
                CacheLookup::Miss(_) => self.read_disk(&key).map(|(value, age)| {
                    // Promote so later reads in this process skip disk + JSON
                    // decode; backdate insertion so it expires with the disk copy.
                    let now = Instant::now();
                    let inserted = now.checked_sub(age).unwrap_or(now);
                    let bytes = entry_bytes(&key, &value);
                    self.cache.lock().unwrap_or_else(|p| p.into_inner()).insert(
                        key.clone(),
                        value.clone(),
                        bytes,
                        self.revision,
                        inserted,
                    );
                    value
                }),
            };
            // File bodies are keyed by the resolved commit SHA, so a hit can
            // never go stale. Dropping the ETag makes the provider serve it as
            // is instead of spending one conditional (304) round trip per read;
            // clasify pages a large file through ~20 such reads.
            value.map(|mut value| {
                if immutable {
                    value.etag = None;
                }
                value
            })
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
            let bytes = entry_bytes(&key, &value);
            self.write_disk(&key, &value);
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

    async fn partition(identity: &str) -> ProviderPartition {
        use crate::providers::github::{
            CredentialSource, GitHubEndpoint, GitHubTransport, RequestContext, RetryPolicy,
            StaticCredentialResolver,
        };
        let (endpoint, credential) = identity.split_once('/').unwrap_or(("fixture", identity));
        let transport = GitHubTransport::new(
            GitHubEndpoint::new(
                format!("https://{endpoint}.example/api/v3")
                    .parse()
                    .unwrap(),
            )
            .unwrap(),
            Arc::new(StaticCredentialResolver::new(
                credential,
                CredentialSource::Override,
            )),
            RetryPolicy::default(),
        )
        .unwrap();
        transport
            .cache_partition(
                &RequestContext::with_timeout(Duration::from_secs(1), 1),
                None,
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn disk_cache_isolates_endpoint_and_credential_partitions() {
        let dir = tempfile::tempdir().unwrap();
        let cache = || GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.path().into()));
        let first = partition("endpoint-a/credential-a").await;
        let other_credential = partition("endpoint-a/credential-b").await;
        let other_endpoint = partition("endpoint-b/credential-a").await;
        let content = CachedContent {
            bytes: b"private source".to_vec(),
            etag: Some("v1".into()),
            resolved_ref: "sha".into(),
        };
        cache().put(&first, "file".into(), content.clone()).await;
        assert_eq!(cache().get(&first, "file").await, Some(content));
        assert_eq!(cache().get(&other_credential, "file").await, None);
        assert_eq!(cache().get(&other_endpoint, "file").await, None);
    }

    #[tokio::test]
    async fn commit_pinned_file_bodies_skip_revalidation_but_trees_keep_etags() {
        let dir = tempfile::tempdir().unwrap();
        let cache = || GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.path().into()));
        let part = partition("endpoint/credential").await;
        let content = CachedContent {
            bytes: b"source".to_vec(),
            etag: Some("v1".into()),
            resolved_ref: "sha".into(),
        };
        let memory = cache();
        memory
            .put(&part, "github-content:abc".into(), content.clone())
            .await;
        memory
            .put(&part, "github-tree:abc".into(), content.clone())
            .await;
        for tier in [&memory, &cache()] {
            let file = tier.get(&part, "github-content:abc").await.unwrap();
            assert_eq!(file.etag, None, "immutable body must be served as is");
            assert_eq!(file.bytes, content.bytes);
            assert_eq!(
                tier.get(&part, "github-tree:abc").await.unwrap().etag,
                Some("v1".into())
            );
        }
    }

    #[tokio::test]
    async fn expired_disk_entries_and_explicit_clear_do_not_survive() {
        let dir = tempfile::tempdir().unwrap();
        let cache = || GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.path().into()));
        let part = partition("endpoint/credential").await;
        let content = CachedContent {
            bytes: b"source".to_vec(),
            etag: Some("v1".into()),
            resolved_ref: "sha".into(),
        };
        let original = cache();
        original.put(&part, "file".into(), content.clone()).await;
        let file = fs::read_dir(dir.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut entry: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        entry["stored_at_unix"] = serde_json::json!(0);
        fs::write(&file, serde_json::to_vec(&entry).unwrap()).unwrap();
        assert_eq!(cache().get(&part, "file").await, None);
        assert!(!file.exists());
        original.put(&part, "file".into(), content).await;
        original.clear();
        assert_eq!(original.get(&part, "file").await, None);
        assert_eq!(cache().get(&part, "file").await, None);
    }

    #[tokio::test]
    async fn legacy_unpartitioned_disk_entries_are_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let cache = GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.path().into()));
        let legacy_path = dir
            .path()
            .join(format!("{}.json", hex::encode(Sha256::digest(b"file"))));
        let content = CachedContent {
            bytes: b"unpartitioned source".to_vec(),
            etag: Some("v1".into()),
            resolved_ref: "sha".into(),
        };
        fs::write(
            legacy_path,
            serde_json::to_vec(&DiskEntryRef {
                revision: 7,
                stored_at_unix: now_unix(),
                value: &content,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(cache.get(&partition("partition").await, "file").await, None);
    }
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
        let one = partition("endpoint/credential/one").await;
        let two = partition("endpoint/credential/two").await;
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
    async fn disk_bodies_are_base64_legacy_arrays_decode_and_hits_promote() {
        let dir = tempfile::tempdir().unwrap();
        let part = partition("endpoint/credential").await;
        let body = vec![b'a'; 3000];
        let content = CachedContent {
            bytes: body.clone(),
            etag: None,
            resolved_ref: "sha".into(),
        };
        GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.path().into()))
            .put(&part, "github-content:k".into(), content.clone())
            .await;
        let file = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "json"))
            .unwrap();
        let size = fs::metadata(&file).unwrap().len() as usize;
        // base64 is 4/3 of the body; a number array was ~3.5-4x.
        assert!(size < body.len() * 3 / 2, "disk entry is {size} bytes");

        // A legacy number-array entry still decodes.
        let mut entry: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        entry["value"]["bytes"] = serde_json::json!(b"legacy".to_vec());
        fs::write(&file, serde_json::to_vec(&entry).unwrap()).unwrap();
        let reader = GitHubContentCache::new(CacheConfig::default(), 7, Some(dir.path().into()));
        let legacy = reader.get(&part, "github-content:k").await.unwrap();
        assert_eq!(legacy.bytes, b"legacy");

        // The disk hit was promoted: it survives the file disappearing.
        fs::remove_file(&file).unwrap();
        assert_eq!(
            reader.get(&part, "github-content:k").await.unwrap().bytes,
            b"legacy"
        );
    }

    #[tokio::test]
    async fn pruning_is_amortized_across_writes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = GitHubContentCache::new(
            CacheConfig {
                max_entries: 2,
                ..Default::default()
            },
            7,
            Some(dir.path().into()),
        );
        let part = partition("endpoint/credential").await;
        let content = CachedContent {
            bytes: b"x".to_vec(),
            etag: None,
            resolved_ref: "sha".into(),
        };
        for index in 0..5 {
            cache
                .put(&part, format!("key-{index}"), content.clone())
                .await;
        }
        let json_files = || {
            fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .count()
        };
        // Only the first write pruned (nothing to prune yet); later writes
        // did not rescan the directory.
        assert_eq!(json_files(), 5);
        assert!(dir.path().join(PRUNE_MARKER).exists());
    }

    #[tokio::test]
    async fn disk_reads_round_trip_but_reject_a_newer_config_revision() {
        let dir =
            std::env::temp_dir().join(format!("octocode-ghcache-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let part = partition("endpoint/credential").await;
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
