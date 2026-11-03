//! The one in-process memo outside the engine's own caches ([`crate`]):
//! least-recently-used order, a lifetime chosen per entry, shared (`Arc`)
//! values, and a lock that recovers from poisoning instead of skipping work.
//!
//! Every memo draws on one [`MemoryBudget`] of bytes, shared with every other
//! memo built on the same budget: an insertion past it evicts the least
//! recently used entries of all of them, oldest first, and a value larger
//! than the whole budget is not kept. A memo sets no limit of its own.
//!
//! Single flight is composed, not built in: a caller that must not run the
//! same load twice holds a `KeyedFlight` turn ([`crate::flight`]) around its
//! `get`-then-`insert`.
mod budget;

pub use budget::MemoryBudget;
use budget::{Held, next_use};
use lru::LruCache;
use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

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
    /// When it was last stored or read, in the budget's use order.
    used: u64,
}

struct Inner<K, V, M> {
    entries: LruCache<K, Entry<V, M>>,
    /// Only expiring entries occupy this index; removal also removes the deadline.
    expiry: BTreeSet<(Instant, K)>,
    weight: usize,
    budget: &'static MemoryBudget,
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
        self.budget.release(entry.weight);
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
                self.budget.release(entry.weight);
            }
        }
    }

    fn clear(&mut self) {
        self.entries = LruCache::unbounded();
        self.expiry.clear();
        self.budget.release(self.weight);
        self.weight = 0;
    }
}

impl<K, V, M> Drop for Inner<K, V, M> {
    /// A dropped memo (one an engine owned) returns what it held.
    fn drop(&mut self) {
        self.budget.release(self.weight);
    }
}

pub struct Memo<K, V, M = ()> {
    budget: &'static MemoryBudget,
    inner: OnceLock<Arc<Mutex<Inner<K, V, M>>>>,
}

impl<
    K: Hash + Eq + Ord + Clone + Send + 'static,
    V: Send + Sync + 'static,
    M: Clone + Send + 'static,
> Memo<K, V, M>
{
    /// A memo drawing on `budget`, shared with every memo built on it.
    pub const fn new(budget: &'static MemoryBudget) -> Self {
        Self {
            budget,
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
                    budget: self.budget,
                }));
                self.budget.register(inner.clone());
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
        let entry = inner.entries.get_mut(key)?;
        entry.used = next_use();
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
        entry.used = next_use();
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
    ) -> (Arc<V>, bool)
    where
        M: Default,
    {
        self.insert_full(key, value, M::default(), weight, ttl, Instant::now())
    }

    /// Store `value`, `weight` bytes, for `key`, replacing any entry; then
    /// expired entries go first, and the budget's least recently used entries
    /// across its memos until it holds. Returns the value and whether it was
    /// kept: one heavier than the whole budget, or already expired, is
    /// returned to its caller without displacing anything (an entry it
    /// replaces is still dropped).
    pub fn insert_full(
        &self,
        key: K,
        value: impl Into<Arc<V>>,
        meta: M,
        weight: usize,
        ttl: Option<Duration>,
        stored_at: Instant,
    ) -> (Arc<V>, bool) {
        let value = value.into();
        {
            let mut inner = self.lock();
            inner.remove(&key);
            let now = Instant::now();
            if weight > self.budget.limit()
                || ttl.is_some_and(|ttl| now.saturating_duration_since(stored_at) >= ttl)
            {
                return (value, false);
            }
            inner.expire(now);
            let expires_at = ttl.and_then(|ttl| stored_at.checked_add(ttl));
            if let Some(deadline) = expires_at {
                inner.expiry.insert((deadline, key.clone()));
            }
            inner.weight = inner.weight.saturating_add(weight);
            self.budget.charge(weight);
            inner.entries.put(
                key,
                Entry {
                    value: Arc::clone(&value),
                    meta,
                    weight,
                    stored_at,
                    expires_at,
                    used: next_use(),
                },
            );
        }
        // Outside this memo's lock: reclaiming locks each memo in turn.
        self.budget.reclaim();
        (value, true)
    }

    pub fn remove<Q: Hash + Eq + ?Sized>(&self, key: &Q)
    where
        K: Borrow<Q>,
    {
        self.lock().remove(key);
    }

    pub fn clear(&self) {
        self.lock().clear();
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

impl<K: Hash + Eq + Ord + Clone + Send, V: Send + Sync, M: Send> Held for Mutex<Inner<K, V, M>> {
    fn clear(&self) {
        self.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    fn oldest(&self) -> Option<u64> {
        let inner = self.lock().unwrap_or_else(PoisonError::into_inner);
        inner.entries.peek_lru().map(|(_, entry)| entry.used)
    }

    fn evict_oldest(&self, used: u64) -> bool {
        let mut inner = self.lock().unwrap_or_else(PoisonError::into_inner);
        if inner
            .entries
            .peek_lru()
            .is_none_or(|(_, entry)| entry.used != used)
        {
            return false;
        }
        let Some((key, entry)) = inner.entries.pop_lru() else {
            return false;
        };
        inner.forget(&key, &entry);
        true
    }
}

#[cfg(test)]
mod tests;
