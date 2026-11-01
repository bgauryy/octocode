//! The one in-process memo outside the engine's own caches ([`crate`]):
//! least-recently-used order, an entry cap and a weight cap, a lifetime
//! chosen per entry, shared (`Arc`) values, and a lock that recovers from
//! poisoning instead of skipping work.
//!
//! Single flight is composed, not built in: a caller that must not run the
//! same load twice holds a `KeyedFlight` turn ([`crate::flight`]) around its
//! `get`-then-`insert`.
use lru::LruCache;
use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

/// A memo's budgets.
#[derive(Clone, Copy, Debug)]
pub struct MemoLimits {
    pub max_entries: usize,
    /// Total weight kept, in the caller's unit (bytes, graph elements...).
    pub max_weight: usize,
    /// Keep the newest entry alone even when it exceeds `max_weight`.
    pub keep_newest: bool,
}

/// A hit: the shared value, the metadata stored with it, and its age.
#[derive(Debug)]
pub struct Hit<V, M> {
    pub value: Arc<V>,
    pub meta: M,
    pub age: Duration,
}

struct Entry<V, M> {
    value: Arc<V>,
    meta: M,
    weight: usize,
    stored_at: Instant,
    expires_at: Option<Instant>,
}

struct Inner<K, V, M> {
    entries: LruCache<K, Entry<V, M>>,
    /// Only expiring entries occupy this index; removal also removes the deadline.
    expiry: BTreeSet<(Instant, K)>,
    weight: usize,
}

impl<K: Hash + Eq + Ord + Clone, V, M> Inner<K, V, M> {
    fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<Entry<V, M>>
    where
        K: Borrow<Q>,
    {
        let (key, entry) = self.entries.pop_entry(key)?;
        self.forget(&key, &entry);
        Some(entry)
    }

    fn forget(&mut self, key: &K, entry: &Entry<V, M>) {
        self.weight = self.weight.saturating_sub(entry.weight);
        if let Some(deadline) = entry.expires_at {
            self.expiry.remove(&(deadline, key.clone()));
        }
    }

    fn expire(&mut self, now: Instant) {
        while self
            .expiry
            .first()
            .is_some_and(|(deadline, _)| *deadline <= now)
        {
            let Some((_, key)) = self.expiry.pop_first() else {
                break;
            };
            if let Some(entry) = self.entries.pop(&key) {
                self.weight = self.weight.saturating_sub(entry.weight);
            }
        }
    }
}

pub struct Memo<K, V, M = ()> {
    limits: MemoLimits,
    inner: OnceLock<Arc<Mutex<Inner<K, V, M>>>>,
}

impl<
    K: Hash + Eq + Ord + Clone + Send + 'static,
    V: Send + Sync + 'static,
    M: Clone + Send + 'static,
> Memo<K, V, M>
{
    pub const fn new(limits: MemoLimits) -> Self {
        Self {
            limits,
            inner: OnceLock::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner<K, V, M>> {
        self.inner
            .get_or_init(|| {
                let inner = Arc::new(Mutex::new(Inner {
                    entries: LruCache::unbounded(),
                    expiry: BTreeSet::new(),
                    weight: 0,
                }));
                REGISTRY.register(inner.clone());
                inner
            })
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The unexpired entry for `key`, now the most recently used.
    pub fn get<Q: Hash + Eq + ?Sized>(&self, key: &Q) -> Option<Hit<V, M>>
    where
        K: Borrow<Q>,
    {
        self.get_at(key, Instant::now())
    }

    fn get_at<Q: Hash + Eq + ?Sized>(&self, key: &Q, now: Instant) -> Option<Hit<V, M>>
    where
        K: Borrow<Q>,
    {
        let mut inner = self.lock();
        if inner
            .entries
            .peek(key)?
            .expires_at
            .is_some_and(|deadline| deadline <= now)
        {
            inner.remove(key);
            return None;
        }
        let entry = inner.entries.get(key)?;
        Some(Hit {
            value: Arc::clone(&entry.value),
            meta: entry.meta.clone(),
            age: now.saturating_duration_since(entry.stored_at),
        })
    }

    /// Restart the lifetime of `key`'s entry: its age counts from now and it
    /// expires `ttl` from now. A missing entry is left alone.
    pub fn touch<Q: Hash + Eq + ?Sized>(&self, key: &Q, ttl: Duration)
    where
        K: Borrow<Q>,
    {
        let now = Instant::now();
        let mut inner = self.lock();
        let Some((key, mut entry)) = inner.entries.pop_entry(key) else {
            return;
        };
        if let Some(deadline) = entry.expires_at {
            inner.expiry.remove(&(deadline, key.clone()));
        }
        entry.stored_at = now;
        entry.expires_at = now.checked_add(ttl);
        if let Some(deadline) = entry.expires_at {
            inner.expiry.insert((deadline, key.clone()));
        }
        inner.entries.put(key, entry);
    }

    /// Store `value` for `key` (replacing any entry) with no metadata.
    pub fn insert(
        &self,
        key: K,
        value: impl Into<Arc<V>>,
        weight: usize,
        ttl: Option<Duration>,
    ) -> Arc<V>
    where
        M: Default,
    {
        self.insert_full(key, value, M::default(), weight, ttl, Instant::now())
    }

    /// Expired entries go first, then LRU entries until both strict budgets hold.
    /// An already expired value is returned to its caller without displacing live entries.
    pub fn insert_full(
        &self,
        key: K,
        value: impl Into<Arc<V>>,
        meta: M,
        weight: usize,
        ttl: Option<Duration>,
        stored_at: Instant,
    ) -> Arc<V> {
        let value = value.into();
        let mut inner = self.lock();
        inner.remove(&key);
        let now = Instant::now();
        if self.limits.max_entries == 0
            || (weight > self.limits.max_weight && !self.limits.keep_newest)
            || ttl.is_some_and(|ttl| now.saturating_duration_since(stored_at) >= ttl)
        {
            return value;
        }
        inner.expire(now);
        while !inner.entries.is_empty()
            && (inner.entries.len() >= self.limits.max_entries
                || inner.weight.saturating_add(weight) > self.limits.max_weight)
        {
            let Some((old_key, entry)) = inner.entries.pop_lru() else {
                break;
            };
            inner.forget(&old_key, &entry);
        }
        let expires_at = ttl.and_then(|ttl| stored_at.checked_add(ttl));
        if let Some(deadline) = expires_at {
            inner.expiry.insert((deadline, key.clone()));
        }
        inner.weight = inner.weight.saturating_add(weight);
        inner.entries.put(
            key,
            Entry {
                value: Arc::clone(&value),
                meta,
                weight,
                stored_at,
                expires_at,
            },
        );
        value
    }

    pub fn remove<Q: Hash + Eq + ?Sized>(&self, key: &Q)
    where
        K: Borrow<Q>,
    {
        self.lock().remove(key);
    }

    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.entries = LruCache::unbounded();
        inner.expiry.clear();
        inner.weight = 0;
    }

    /// Entries held, expired ones included until touched or another insertion.
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Keys from least to most recently used, and their total weight.
    pub fn contents(&self) -> (Vec<K>, usize) {
        let inner = self.lock();
        (
            inner
                .entries
                .iter()
                .rev()
                .map(|(key, _)| key.clone())
                .collect(),
            inner.weight,
        )
    }
}

trait Clear: Send + Sync {
    fn clear(&self);
}

impl<K: Hash + Eq + Send, V: Send + Sync, M: Send> Clear for Mutex<Inner<K, V, M>> {
    fn clear(&self) {
        let mut inner = self.lock().unwrap_or_else(PoisonError::into_inner);
        inner.entries = LruCache::unbounded();
        inner.expiry.clear();
        inner.weight = 0;
    }
}

#[derive(Default)]
struct Registry(Mutex<Vec<Weak<dyn Clear>>>);
static REGISTRY: Registry = Registry(Mutex::new(Vec::new()));

impl Registry {
    fn register(&self, cache: Arc<dyn Clear>) {
        let mut stores = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        stores.retain(|store| store.strong_count() != 0);
        stores.push(Arc::downgrade(&cache));
    }

    fn clear(&self) {
        // Release the registry lock before entering a cache.
        let stores: Vec<_> = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        for store in stores {
            store.clear();
        }
    }
}

/// Clear every initialized memo in this process, including tool-local statics.
/// Callers must quiesce tool execution first; values already borrowed by a caller remain valid.
pub fn clear_process_memos() {
    REGISTRY.clear();
}

#[cfg(test)]
mod tests;
