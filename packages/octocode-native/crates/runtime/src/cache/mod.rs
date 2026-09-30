pub mod evictions;

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Create or truncate `path` with owner-only (0600) permissions on Unix, for
/// state files that hold keys or lock metadata.
#[cfg(unix)]
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

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
pub struct CachePartition {
    pub endpoint: String,
    pub credential_fingerprint: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheKey {
    pub namespace: String,
    pub resource: String,
    pub partition: CachePartition,
}

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub max_entries: usize,
    pub max_bytes: usize,
    pub ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1_000,
            max_bytes: 32 * 1024 * 1024,
            ttl: Duration::from_secs(300),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheStats {
    pub entries: usize,
    pub bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub expirations: u64,
    pub evictions: u64,
    pub invalidations: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheSnapshot {
    pub generation: u64,
    pub config_revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheMiss {
    Absent,
    Expired,
    ConfigRevisionChanged,
    StaleSnapshot,
}

#[derive(Clone, Debug)]
pub enum CacheLookup<V> {
    Hit {
        value: Arc<V>,
        snapshot: CacheSnapshot,
    },
    Miss(CacheMiss),
}

struct Entry<V> {
    value: Arc<V>,
    bytes: usize,
    expires_at: Instant,
    config_revision: u64,
    generation: u64,
}

pub struct BoundedCache<V> {
    config: CacheConfig,
    entries: HashMap<CacheKey, Entry<V>>,
    order: VecDeque<CacheKey>,
    stats: CacheStats,
    generation: u64,
}

impl<V> BoundedCache<V> {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
            order: VecDeque::new(),
            stats: CacheStats::default(),
            generation: 0,
        }
    }

    pub fn insert(
        &mut self,
        key: CacheKey,
        value: V,
        bytes: usize,
        config_revision: u64,
        now: Instant,
    ) -> Option<CacheSnapshot> {
        if self.config.max_entries == 0 || bytes > self.config.max_bytes {
            return None;
        }
        self.remove(&key, false);
        self.generation = self.generation.wrapping_add(1);
        let snapshot = CacheSnapshot {
            generation: self.generation,
            config_revision,
        };
        self.entries.insert(
            key.clone(),
            Entry {
                value: Arc::new(value),
                bytes,
                expires_at: now + self.config.ttl,
                config_revision,
                generation: snapshot.generation,
            },
        );
        self.order.push_back(key);
        self.recount();
        self.evict_to_budget();
        Some(snapshot)
    }

    pub fn get(
        &mut self,
        key: &CacheKey,
        config_revision: u64,
        expected: Option<CacheSnapshot>,
        now: Instant,
    ) -> CacheLookup<V> {
        let reason = match self.entries.get(key) {
            None => Some(CacheMiss::Absent),
            Some(entry) if now >= entry.expires_at => Some(CacheMiss::Expired),
            Some(entry) if entry.config_revision != config_revision => {
                Some(CacheMiss::ConfigRevisionChanged)
            }
            Some(entry)
                if expected.is_some_and(|snapshot| {
                    snapshot.generation != entry.generation
                        || snapshot.config_revision != entry.config_revision
                }) =>
            {
                Some(CacheMiss::StaleSnapshot)
            }
            Some(_) => None,
        };
        if let Some(reason) = reason {
            self.stats.misses += 1;
            if matches!(
                reason,
                CacheMiss::Expired | CacheMiss::ConfigRevisionChanged
            ) {
                self.remove(key, true);
                if reason == CacheMiss::Expired {
                    self.stats.expirations += 1;
                } else {
                    self.stats.invalidations += 1;
                }
            }
            return CacheLookup::Miss(reason);
        }
        self.touch(key);
        self.stats.hits += 1;
        // Presence was established by the miss checks above before `touch`.
        #[allow(clippy::expect_used)]
        let entry = self.entries.get(key).expect("entry checked above");
        CacheLookup::Hit {
            value: Arc::clone(&entry.value),
            snapshot: CacheSnapshot {
                generation: entry.generation,
                config_revision: entry.config_revision,
            },
        }
    }

    pub fn invalidate_all(&mut self) {
        let count = self.entries.len();
        self.entries.clear();
        self.order.clear();
        self.stats.invalidations += count as u64;
        self.recount();
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    fn touch(&mut self, key: &CacheKey) {
        self.order.retain(|candidate| candidate != key);
        self.order.push_back(key.clone());
    }

    fn remove(&mut self, key: &CacheKey, recount: bool) {
        self.entries.remove(key);
        self.order.retain(|candidate| candidate != key);
        if recount {
            self.recount();
        }
    }

    fn recount(&mut self) {
        self.stats.entries = self.entries.len();
        self.stats.bytes = self.entries.values().map(|entry| entry.bytes).sum();
    }

    fn evict_to_budget(&mut self) {
        while self.entries.len() > self.config.max_entries
            || self.stats.bytes > self.config.max_bytes
        {
            let Some(key) = self.order.pop_front() else {
                break;
            };
            if self.entries.remove(&key).is_some() {
                self.stats.evictions += 1;
            }
            self.recount();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(resource: &str, credential: &str) -> CacheKey {
        CacheKey {
            namespace: "github".into(),
            resource: resource.into(),
            partition: CachePartition {
                endpoint: "https://api.github.com".into(),
                credential_fingerprint: credential.into(),
            },
        }
    }

    #[test]
    fn enforces_byte_and_entry_lru_budgets() {
        let mut cache = BoundedCache::new(CacheConfig {
            max_entries: 2,
            max_bytes: 5,
            ttl: Duration::from_secs(60),
        });
        let now = Instant::now();
        cache.insert(key("a", "x"), 1, 2, 1, now);
        cache.insert(key("b", "x"), 2, 2, 1, now);
        let _ = cache.get(&key("a", "x"), 1, None, now);
        cache.insert(key("c", "x"), 3, 2, 1, now);
        assert!(matches!(
            cache.get(&key("b", "x"), 1, None, now),
            CacheLookup::Miss(CacheMiss::Absent)
        ));
        assert_eq!(cache.stats().evictions, 1);
        assert_eq!(cache.stats().bytes, 4);
    }

    #[test]
    fn expires_and_invalidates_revision_without_mixing_snapshots() {
        let mut cache = BoundedCache::new(CacheConfig {
            ttl: Duration::from_secs(1),
            ..Default::default()
        });
        let now = Instant::now();
        let snapshot = cache
            .insert(key("a", "x"), 1, 1, 7, now)
            .expect("cacheable");
        assert!(matches!(
            cache.get(&key("a", "x"), 7, Some(snapshot), now),
            CacheLookup::Hit { .. }
        ));
        assert!(matches!(
            cache.get(&key("a", "x"), 8, None, now),
            CacheLookup::Miss(CacheMiss::ConfigRevisionChanged)
        ));
        let old = cache
            .insert(key("a", "x"), 2, 1, 8, now)
            .expect("cacheable");
        let _new = cache
            .insert(key("a", "x"), 3, 1, 8, now)
            .expect("cacheable");
        assert!(matches!(
            cache.get(&key("a", "x"), 8, Some(old), now),
            CacheLookup::Miss(CacheMiss::StaleSnapshot)
        ));
        assert!(matches!(
            cache.get(&key("a", "x"), 8, None, now + Duration::from_secs(2)),
            CacheLookup::Miss(CacheMiss::Expired)
        ));
    }

    #[test]
    fn partitions_by_endpoint_and_credential() {
        let mut cache = BoundedCache::new(CacheConfig::default());
        let now = Instant::now();
        cache.insert(key("a", "first"), 1, 1, 1, now);
        cache.insert(key("a", "second"), 2, 1, 1, now);
        assert!(matches!(
            cache.get(&key("a", "first"), 1, None, now),
            CacheLookup::Hit { .. }
        ));
        assert!(matches!(
            cache.get(&key("a", "second"), 1, None, now),
            CacheLookup::Hit { .. }
        ));
    }

    /// `max_entries: 0` is the programmatic "cache disabled" signal.
    /// `insert` must return `None` (refused) and every `get` must be `Absent`.
    #[test]
    fn max_entries_zero_disables_all_inserts_and_reads() {
        let mut cache = BoundedCache::<u32>::new(CacheConfig {
            max_entries: 0,
            ..Default::default()
        });
        let now = Instant::now();
        let snapshot = cache.insert(key("a", "x"), 99, 4, 1, now);
        assert!(snapshot.is_none(), "max_entries=0 must refuse inserts");
        assert!(
            matches!(
                cache.get(&key("a", "x"), 1, None, now),
                CacheLookup::Miss(CacheMiss::Absent)
            ),
            "refused insert must leave nothing readable"
        );
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(cache.stats().bytes, 0);
    }

    /// Hits, misses, expirations, and evictions must each increment only their
    /// own counter; no counter should bleed into another.
    #[test]
    fn stats_counters_increment_independently() {
        let mut cache = BoundedCache::<u32>::new(CacheConfig {
            max_entries: 1,
            max_bytes: 100,
            ttl: Duration::from_secs(60),
        });
        let now = Instant::now();

        // Miss on empty cache.
        let _ = cache.get(&key("missing", "x"), 1, None, now);
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);

        // Insert and hit.
        cache.insert(key("a", "x"), 1, 4, 1, now);
        let _ = cache.get(&key("a", "x"), 1, None, now);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1); // unchanged

        // Revision change → miss + invalidation (not expiration).
        let _ = cache.get(&key("a", "x"), 2, None, now);
        assert_eq!(cache.stats().invalidations, 1);
        assert_eq!(cache.stats().expirations, 0);

        // Expired entry → miss + expiration counter.
        cache.insert(key("b", "x"), 2, 4, 1, now);
        let future = now + Duration::from_secs(400);
        let _ = cache.get(&key("b", "x"), 1, None, future);
        assert_eq!(cache.stats().expirations, 1);

        // Eviction: new insert exceeds max_entries=1, oldest is evicted.
        cache.insert(key("c", "x"), 3, 4, 1, now);
        cache.insert(key("d", "x"), 4, 4, 1, now);
        assert_eq!(cache.stats().evictions, 1);
    }

    /// A `get()` call re-orders the accessed entry to the back of the LRU queue.
    /// After a hit, filling the budget must evict the un-touched entry, not the
    /// recently accessed one.
    #[test]
    fn lru_touch_on_hit_protects_accessed_entry_from_eviction() {
        let mut cache = BoundedCache::<u32>::new(CacheConfig {
            max_entries: 2,
            max_bytes: 1000,
            ttl: Duration::from_secs(60),
        });
        let now = Instant::now();

        // Insert two entries; "a" is older (front of LRU queue).
        cache.insert(key("a", "x"), 1, 4, 1, now);
        cache.insert(key("b", "x"), 2, 4, 1, now);

        // Touch "a" → moves it to the back; "b" is now the eviction candidate.
        let _ = cache.get(&key("a", "x"), 1, None, now);

        // Insert "c" → budget exceeded, LRU entry ("b") is evicted.
        cache.insert(key("c", "x"), 3, 4, 1, now);

        assert!(
            matches!(
                cache.get(&key("a", "x"), 1, None, now),
                CacheLookup::Hit { .. }
            ),
            "touched entry 'a' must survive eviction"
        );
        assert!(
            matches!(
                cache.get(&key("b", "x"), 1, None, now),
                CacheLookup::Miss(CacheMiss::Absent)
            ),
            "un-touched entry 'b' must be evicted"
        );
        assert!(
            matches!(
                cache.get(&key("c", "x"), 1, None, now),
                CacheLookup::Hit { .. }
            ),
            "newest entry 'c' must survive"
        );
    }
}
