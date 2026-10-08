//! One cache store for the runtime: a byte-bounded LRU memory tier plus an
//! optional byte-bounded disk tier. The writer picks a [`CacheClass`] per
//! entry, so pinned facts never expire while movable ones age out or
//! revalidate.
pub mod evictions;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Create `path` (and missing parents) owner-only (0700) on Unix, and tighten
/// an existing directory, for caches that hold private repository content.
#[cfg(unix)]
pub(crate) fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
pub(crate) fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorePartition {
    pub endpoint: String,
    pub credential_fingerprint: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheKey {
    pub namespace: String,
    pub resource: String,
    pub partition: StorePartition,
}

/// How an entry ages; the writer chooses it at `put`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheClass {
    /// Keyed by a content digest or a commit SHA: never expires and is always
    /// fresh. Only the byte budget evicts it.
    Immutable,
    /// Fresh for [`CacheConfig::fresh`], then returned stale so the caller
    /// revalidates it (ETag; a 304 costs no rate limit). Only the byte budget
    /// evicts it.
    Revalidate,
    /// Expires after [`CacheConfig::ttl`].
    Volatile,
}

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub max_entries: usize,
    /// Memory budget; also the largest single entry either tier accepts.
    pub max_bytes: usize,
    /// Disk budget, pruned oldest-first by modification time.
    pub disk_bytes: u64,
    /// How long a [`CacheClass::Revalidate`] entry is served without a check.
    pub fresh: Duration,
    /// Lifetime of a [`CacheClass::Volatile`] entry.
    pub ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 4_096,
            max_bytes: 64 * 1024 * 1024,
            disk_bytes: 512 * 1024 * 1024,
            fresh: Duration::from_secs(60),
            ttl: Duration::from_secs(60),
        }
    }
}

/// A hit. `fresh` is false only for a [`CacheClass::Revalidate`] entry past
/// its fresh window: the caller must revalidate before serving it.
#[derive(Clone, Debug)]
pub struct Cached<V> {
    pub value: Arc<V>,
    pub fresh: bool,
}

struct Entry<V> {
    value: Arc<V>,
    bytes: usize,
    class: CacheClass,
    stored_at: Instant,
    /// This entry's slot in [`Memory::order`].
    tick: u64,
}

struct Memory<V> {
    entries: HashMap<CacheKey, Entry<V>>,
    /// Keys by last use: the first tick is the least recently used. A touch
    /// moves one key, O(log n), instead of rescanning the whole order.
    order: BTreeMap<u64, CacheKey>,
    next_tick: u64,
    bytes: usize,
}

impl<V> Memory<V> {
    fn remove(&mut self, key: &CacheKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.bytes = self.bytes.saturating_sub(entry.bytes);
            self.order.remove(&entry.tick);
        }
    }

    fn tick(&mut self) -> u64 {
        let tick = self.next_tick;
        self.next_tick += 1;
        tick
    }
}

/// Freshness of an entry `age` old, or `None` once it expired.
fn freshness(config: &CacheConfig, class: CacheClass, age: Duration) -> Option<bool> {
    match class {
        CacheClass::Immutable => Some(true),
        CacheClass::Revalidate => Some(age < config.fresh),
        CacheClass::Volatile => (age < config.ttl).then_some(true),
    }
}

pub struct Store<V> {
    config: CacheConfig,
    memory: Mutex<Memory<V>>,
    disk: Option<Disk>,
}

impl<V: Clone + Serialize + DeserializeOwned> Store<V> {
    /// `disk: None` keeps the store in this process only.
    pub fn new(config: CacheConfig, disk: Option<PathBuf>) -> Self {
        Self {
            disk: disk.map(Disk::new),
            memory: Mutex::new(Memory {
                entries: HashMap::new(),
                order: BTreeMap::new(),
                next_tick: 0,
                bytes: 0,
            }),
            config,
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<Cached<V>> {
        let now = Instant::now();
        {
            let mut memory = self.memory.lock().unwrap_or_else(|p| p.into_inner());
            let tick = memory.next_tick;
            let memory = &mut *memory;
            if let Some(entry) = memory.entries.get_mut(key) {
                let age = now.saturating_duration_since(entry.stored_at);
                match freshness(&self.config, entry.class, age) {
                    Some(fresh) => {
                        let value = Arc::clone(&entry.value);
                        if let Some(touched) = memory.order.remove(&entry.tick) {
                            memory.order.insert(tick, touched);
                            entry.tick = tick;
                            memory.next_tick += 1;
                        }
                        return Some(Cached { value, fresh });
                    }
                    None => memory.remove(key),
                }
            }
        }
        let (value, class, age, bytes) = self.disk.as_ref()?.read::<V>(key, &self.config)?;
        let fresh = freshness(&self.config, class, age)?;
        // Promote, backdated so the memory copy ages like the disk copy.
        let stored_at = now.checked_sub(age).unwrap_or(now);
        let value = self.insert_memory(key.clone(), value, bytes, class, stored_at);
        Some(Cached { value, fresh })
    }

    /// Store `value` (about `bytes` large) in memory and, when configured, on
    /// disk. An entry larger than the memory budget is not stored at all.
    pub fn put(&self, key: CacheKey, value: V, bytes: usize, class: CacheClass) {
        if self.config.max_entries == 0 || bytes > self.config.max_bytes {
            return;
        }
        if let Some(disk) = &self.disk {
            disk.write(&key, &value, class, &self.config);
        }
        self.insert_memory(key, value, bytes, class, Instant::now());
    }

    /// Drop `key` from both tiers.
    pub fn remove(&self, key: &CacheKey) {
        self.memory
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(key);
        if let Some(disk) = &self.disk {
            let _ = fs::remove_file(disk.file(key));
        }
    }

    pub fn clear_memory(&self) {
        let mut memory = self.memory.lock().unwrap_or_else(|p| p.into_inner());
        memory.entries.clear();
        memory.order.clear();
        memory.bytes = 0;
    }

    pub fn clear(&self) {
        self.clear_memory();
        if let Some(disk) = &self.disk {
            disk.clear();
        }
    }

    fn insert_memory(
        &self,
        key: CacheKey,
        value: V,
        bytes: usize,
        class: CacheClass,
        stored_at: Instant,
    ) -> Arc<V> {
        let value = Arc::new(value);
        if self.config.max_entries == 0 || bytes > self.config.max_bytes {
            return value;
        }
        let mut memory = self.memory.lock().unwrap_or_else(|p| p.into_inner());
        memory.remove(&key);
        let tick = memory.tick();
        memory.entries.insert(
            key.clone(),
            Entry {
                value: Arc::clone(&value),
                bytes,
                class,
                stored_at,
                tick,
            },
        );
        memory.order.insert(tick, key);
        memory.bytes = memory.bytes.saturating_add(bytes);
        while memory.entries.len() > self.config.max_entries || memory.bytes > self.config.max_bytes
        {
            let Some((_, oldest)) = memory.order.pop_first() else {
                break;
            };
            if let Some(entry) = memory.entries.remove(&oldest) {
                memory.bytes = memory.bytes.saturating_sub(entry.bytes);
            }
        }
        value
    }
}

/// Disk scans are O(files); amortize them across writes.
const PRUNE_EVERY_WRITES: usize = 64;
/// Cross-process throttle: one-shot CLI processes each write a few entries, so
/// a marker file bounds pruning to once per interval across processes.
const PRUNE_INTERVAL: Duration = Duration::from_secs(60);
const PRUNE_MARKER: &str = ".last-prune";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskEntryRef<'a, V> {
    class: CacheClass,
    stored_at_unix: u64,
    value: &'a V,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiskEntry<V> {
    class: CacheClass,
    stored_at_unix: u64,
    value: V,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

struct Disk {
    dir: PathBuf,
    /// Writes by this process: pruning runs on the first and then every
    /// [`PRUNE_EVERY_WRITES`] writes instead of scanning per write.
    writes: AtomicUsize,
}

impl Disk {
    fn new(dir: PathBuf) -> Self {
        let _ = create_private_dir_all(&dir);
        Self {
            dir,
            writes: AtomicUsize::new(0),
        }
    }

    fn file(&self, key: &CacheKey) -> PathBuf {
        let mut digest = Sha256::new();
        // Domain separation: files of an older layout are never read.
        digest.update(b"octocode-cache-store-v1");
        for value in [
            &key.namespace,
            &key.partition.endpoint,
            &key.partition.credential_fingerprint,
            &key.resource,
        ] {
            digest.update((value.len() as u64).to_le_bytes());
            digest.update(value.as_bytes());
        }
        self.dir
            .join(format!("{}.json", hex::encode(digest.finalize())))
    }

    fn read<V: DeserializeOwned>(
        &self,
        key: &CacheKey,
        config: &CacheConfig,
    ) -> Option<(V, CacheClass, Duration, usize)> {
        let path = self.file(key);
        // An entry is only a regular file this store could have written: a
        // FIFO, device, symlink, or file past the entry bound is evicted.
        let read = crate::private_file::open_no_follow(&path, false).and_then(|file| {
            let bytes = crate::private_file::read_limited(&file, config.max_bytes as u64)?;
            Ok((file, bytes))
        });
        let (file, bytes) = match read {
            Ok(read) => read,
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    let _ = fs::remove_file(&path);
                }
                return None;
            }
        };
        let Ok(entry) = serde_json::from_slice::<DiskEntry<V>>(&bytes) else {
            let _ = fs::remove_file(&path);
            return None;
        };
        let age = Duration::from_secs(now_unix().saturating_sub(entry.stored_at_unix));
        if freshness(config, entry.class, age).is_none() {
            let _ = fs::remove_file(&path);
            return None;
        }
        // A read counts as use for the oldest-first pruning. A read-only
        // handle may not set times (Windows), so a writer is the fallback.
        let now = SystemTime::now();
        if file.set_modified(now).is_err()
            && let Ok(writer) = fs::File::options().write(true).open(&path)
        {
            let _ = writer.set_modified(now);
        }
        Some((entry.value, entry.class, age, bytes.len()))
    }

    fn write<V: Serialize>(
        &self,
        key: &CacheKey,
        value: &V,
        class: CacheClass,
        config: &CacheConfig,
    ) {
        let path = self.file(key);
        let entry = DiskEntryRef {
            class,
            stored_at_unix: now_unix(),
            value,
        };
        let Ok(bytes) = serde_json::to_vec(&entry) else {
            return;
        };
        if bytes.len() > config.max_bytes {
            return;
        }
        // Owner-only replacement: values may be private source, and an
        // in-place rewrite would keep a pre-existing file's loose mode.
        let _ = crate::private_file::write_atomic(&path, &bytes, false);
        if self
            .writes
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(PRUNE_EVERY_WRITES)
            && self.prune_due()
        {
            self.prune(config.disk_bytes);
        }
    }

    /// True when no process pruned within [`PRUNE_INTERVAL`]; claims the slot
    /// by touching the marker.
    fn prune_due(&self) -> bool {
        let marker = self.dir.join(PRUNE_MARKER);
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

    /// Keep the directory within `budget` bytes, removing the least recently
    /// used files first.
    fn prune(&self, budget: u64) {
        let Ok(read_dir) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = read_dir
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "json") {
                    return None;
                }
                let meta = entry.metadata().ok()?;
                Some((meta.modified().ok()?, meta.len(), path))
            })
            .collect();
        let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
        if total <= budget {
            return;
        }
        files.sort_by_key(|(mtime, _, _)| *mtime);
        for (_, len, path) in files {
            if total <= budget {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(len);
            }
        }
    }

    fn clear(&self) {
        let _ = fs::remove_dir_all(&self.dir);
        let _ = create_private_dir_all(&self.dir);
    }
}

#[cfg(test)]
pub(crate) mod tests;
