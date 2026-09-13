use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheStorageMode {
    MemoryOnly,
}

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub max_entries: usize,
    pub max_bytes: usize,
    pub ttl: Duration,
    pub storage: CacheStorageMode,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1_000,
            max_bytes: 32 * 1024 * 1024,
            ttl: Duration::from_secs(300),
            storage: CacheStorageMode::MemoryOnly,
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
        let entry = self.entries.get(key).expect("entry checked above");
        CacheLookup::Hit {
            value: Arc::clone(&entry.value),
            snapshot: CacheSnapshot {
                generation: entry.generation,
                config_revision: entry.config_revision,
            },
        }
    }

    pub fn invalidate_partition(&mut self, partition: &CachePartition) -> usize {
        let keys = self
            .entries
            .keys()
            .filter(|key| &key.partition == partition)
            .cloned()
            .collect::<Vec<_>>();
        let count = keys.len();
        for key in keys {
            self.remove(&key, true);
        }
        self.stats.invalidations += count as u64;
        count
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
            storage: CacheStorageMode::MemoryOnly,
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
    fn partitions_by_endpoint_and_credential_and_invalidates_one_partition() {
        let mut cache = BoundedCache::new(CacheConfig::default());
        let now = Instant::now();
        cache.insert(key("a", "first"), 1, 1, 1, now);
        cache.insert(key("a", "second"), 2, 1, 1, now);
        assert_eq!(cache.invalidate_partition(&key("a", "first").partition), 1);
        assert!(matches!(
            cache.get(&key("a", "first"), 1, None, now),
            CacheLookup::Miss(CacheMiss::Absent)
        ));
        assert!(matches!(
            cache.get(&key("a", "second"), 1, None, now),
            CacheLookup::Hit { .. }
        ));
    }
}
