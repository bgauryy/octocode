//! One cache store for the runtime: the shared in-process
//! [`octocode_io::memo::Memo`] as its memory tier plus an optional
//! byte-bounded disk tier. The writer picks a [`CacheClass`] per entry, so
//! pinned facts never expire while movable ones age out or revalidate.
mod disk;
pub mod evictions;
pub(crate) use disk::Disk;

use octocode_io::memo::{Memo, MemoLimits};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// An entry's identity. `scope` is opaque to the store: a provider passes
/// its credential [`octocode_github::Partition::identity`], a local memo
/// whatever separates its entries (a path, a parser, a files digest).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CacheKey {
    pub namespace: String,
    pub scope: String,
    pub resource: String,
}

/// How an entry ages; the writer chooses it, with its window, at `put`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheClass {
    /// Keyed by a content digest or a commit SHA: never expires and is always
    /// fresh. Only the byte budget evicts it.
    Immutable,
    /// Fresh for `fresh`, then returned stale so the caller revalidates it
    /// (ETag; a 304 costs no rate limit). Only the byte budget evicts it.
    Revalidate { fresh: Duration },
    /// Expires `ttl` after it is stored.
    Volatile { ttl: Duration },
}

impl CacheClass {
    fn ttl(self) -> Option<Duration> {
        match self {
            Self::Volatile { ttl } => Some(ttl),
            Self::Immutable | Self::Revalidate { .. } => None,
        }
    }

    /// Freshness of an entry `age` old, or `None` once it expired.
    fn freshness(self, age: Duration) -> Option<bool> {
        match self {
            Self::Immutable => Some(true),
            Self::Revalidate { fresh } => Some(age < fresh),
            Self::Volatile { ttl } => (age < ttl).then_some(true),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub max_entries: usize,
    /// Memory budget; also the largest single entry either tier accepts.
    pub max_bytes: usize,
    /// Disk budget, pruned oldest-first by modification time.
    pub disk_bytes: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 4_096,
            max_bytes: 64 * 1024 * 1024,
            disk_bytes: 512 * 1024 * 1024,
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

/// How a store lays its values out on disk, after the entry header.
pub trait DiskCodec<V> {
    fn encode(value: &V) -> Option<Vec<u8>>;
    fn decode(bytes: &[u8]) -> Option<V>;
}

/// A value as JSON: the layout of every store whose values are not bytes.
pub struct JsonCodec;

impl<V: Serialize + DeserializeOwned> DiskCodec<V> for JsonCodec {
    fn encode(value: &V) -> Option<Vec<u8>> {
        serde_json::to_vec(value).ok()
    }

    fn decode(bytes: &[u8]) -> Option<V> {
        serde_json::from_slice(bytes).ok()
    }
}

/// [`Memo`] (memory) plus [`Disk`] (optional, across processes); `C` lays
/// values out on disk.
pub struct Store<V, C = JsonCodec> {
    config: CacheConfig,
    memory: Memo<CacheKey, V, CacheClass>,
    disk: Option<Disk>,
    codec: PhantomData<C>,
}

impl<V, C: DiskCodec<V>> Store<V, C> {
    /// `disk: None` keeps the store in this process only.
    pub(crate) fn new(config: CacheConfig, disk: Option<PathBuf>) -> Self {
        Self {
            disk: disk.map(Disk::new),
            memory: Memo::new(MemoLimits {
                max_entries: config.max_entries,
                max_weight: config.max_bytes,
                keep_newest: false,
            }),
            config,
            codec: PhantomData,
        }
    }

    pub(crate) fn get(&self, key: &CacheKey) -> Option<Cached<V>> {
        if let Some(hit) = self.memory.get(key) {
            // The memo already dropped an expired `Volatile` entry.
            if let Some(fresh) = hit.meta.freshness(hit.age) {
                return Some(Cached {
                    value: hit.value,
                    fresh,
                });
            }
            self.memory.remove(key);
        }
        self.read_disk(key)
    }

    /// Mutable retained work is read under its owner's transaction lock.
    /// Persistent state must observe another process's latest write, including
    /// eviction or expiry, rather than revive an older memory entry.
    pub(crate) fn get_current(&self, key: &CacheKey) -> Option<Cached<V>> {
        if self.disk.is_none() {
            return self.get(key);
        }
        self.memory.remove(key);
        self.read_disk(key)
    }

    fn read_disk(&self, key: &CacheKey) -> Option<Cached<V>> {
        let (value, class, age, bytes) = self.disk.as_ref()?.read::<V, C>(key, &self.config)?;
        let fresh = class.freshness(age)?;
        let now = Instant::now();
        let stored_at = now.checked_sub(age).unwrap_or(now);
        let value =
            self.memory
                .insert_full(key.clone(), value, class, bytes, class.ttl(), stored_at);
        Some(Cached { value, fresh })
    }

    /// Store `value` (about `bytes` large) in memory and, when configured, on
    /// disk. An entry larger than the memory budget is not stored at all.
    /// The stored value is returned, shared with the entry.
    pub(crate) fn put(&self, key: CacheKey, value: V, bytes: usize, class: CacheClass) -> Arc<V> {
        self.put_confirmed(key, value, bytes, class).0
    }

    /// Whether the value was retained in every configured tier. A result
    /// continuation must check this before advertising saved pages.
    pub(crate) fn put_confirmed(
        &self,
        key: CacheKey,
        value: V,
        bytes: usize,
        class: CacheClass,
    ) -> (Arc<V>, bool) {
        if self.config.max_entries == 0 || bytes > self.config.max_bytes {
            return (Arc::new(value), false);
        }
        let saved = self
            .disk
            .as_ref()
            .is_none_or(|disk| disk.write::<V, C>(&key, &value, class, &self.config));
        let value = self
            .memory
            .insert_full(key, value, class, bytes, class.ttl(), Instant::now());
        (value, saved)
    }

    /// Drop `key` from both tiers.
    pub(crate) fn remove(&self, key: &CacheKey) {
        self.memory.remove(key);
        if let Some(disk) = &self.disk {
            let _ = fs::remove_file(disk.file(key));
        }
    }

    pub(crate) fn clear_memory(&self) {
        self.memory.clear();
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod current_tests;

#[cfg(test)]
mod json_size_policy_tests;
