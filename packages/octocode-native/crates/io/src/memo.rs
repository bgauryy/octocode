//! The one in-process memo outside the engine's own caches ([`crate`]):
//! least-recently-used order, a lifetime chosen per entry, shared (`Arc`)
//! values, and a lock that recovers from poisoning instead of skipping work.
//!
//! Every memo draws on one [`MemoryBudget`] of bytes, shared with every other
//! memo built on the same budget: an insertion past it evicts the least
//! recently used entries of all of them, oldest first, and a value larger
//! than the whole budget is not kept. A memo sets no limit of its own.
//!
//! A read never serves an entry past its deadline. Its memory goes on a read
//! of its key, the next insertion into its memo, or, in a process that stays
//! up, [`MemoryBudget::sweep_expired_while_running`].
//!
//! Single flight is composed, not built in: a caller that must not run the
//! same load twice holds a `KeyedFlight` turn ([`crate::flight`]) around its
//! `get`-then-`insert`.
mod budget;

use budget::{Held, next_use};
pub use budget::{MemoryBudget, Reservation};
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

/// Collections detached while locked and destroyed after the lock is released.
type Cleared<K, V, M> = (LruCache<K, Entry<V, M>>, BTreeSet<(Instant, K)>);

struct Inner<K, V, M> {
    entries: LruCache<K, Entry<V, M>>,
    /// Only expiring entries occupy this index; removal also removes the deadline.
    expiry: BTreeSet<(Instant, K)>,
    weight: usize,
    budget: &'static MemoryBudget,
}

impl<K: Hash + Eq + Ord + Clone, V, M> Inner<K, V, M> {
    fn remove<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<(K, Entry<V, M>)>
    where
        K: Borrow<Q>,
    {
        let (key, entry) = self.entries.pop_entry(key)?;
        self.forget(&key, &entry);
        Some((key, entry))
    }

    fn forget(&mut self, key: &K, entry: &Entry<V, M>) {
        self.weight = self.weight.saturating_sub(entry.weight);
        self.budget.release(entry.weight);
        if let Some(deadline) = entry.expires_at {
            self.expiry.remove(&(deadline, key.clone()));
        }
    }

    /// Take out what expired at `now`, for the caller to drop after unlocking:
    /// freeing a large value should not stall a read of this memo.
    fn expire(&mut self, now: Instant) -> Vec<Entry<V, M>> {
        let mut expired = Vec::new();
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
                expired.push(entry);
            }
        }
        expired
    }

    /// Detach retained state under the lock; its destructors run after unlock.
    fn clear(&mut self) -> Cleared<K, V, M> {
        let entries = std::mem::replace(&mut self.entries, LruCache::unbounded());
        let expiry = std::mem::take(&mut self.expiry);
        self.budget.release(self.weight);
        self.weight = 0;
        (entries, expiry)
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
            let expired = inner.remove(key);
            drop(inner);
            drop(expired);
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
        let was_due = entry.expires_at;
        if let Some(deadline) = was_due {
            inner.expiry.remove(&(deadline, key.clone()));
        }
        entry.stored_at = now;
        entry.expires_at = now.checked_add(ttl);
        entry.used = next_use();
        let due = entry.expires_at;
        if let Some(deadline) = due {
            inner.expiry.insert((deadline, key.clone()));
        }
        inner.entries.put(key, entry);
        drop(inner);
        // A later deadline is found when the old one comes: the usual read
        // that restarts a lifetime reaches nothing outside this memo.
        if let Some(deadline) = due
            && was_due.is_none_or(|was_due| deadline < was_due)
        {
            self.budget.note_deadline(deadline);
        }
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
        let (expired, replaced, deadline) = {
            let mut inner = self.lock();
            let replaced = inner.remove(&key);
            let now = Instant::now();
            if weight > self.budget.limit()
                || ttl.is_some_and(|ttl| now.saturating_duration_since(stored_at) >= ttl)
            {
                drop(inner);
                drop(replaced);
                return (value, false);
            }
            let expired = inner.expire(now);
            let expires_at = ttl.and_then(|ttl| stored_at.checked_add(ttl));
            if let Some(deadline) = expires_at {
                inner.expiry.insert((deadline, key.clone()));
            }
            inner.weight = inner.weight.saturating_add(weight);
            self.budget.charge(weight);
            inner.entries.put(
                key.clone(),
                Entry {
                    value: Arc::clone(&value),
                    meta,
                    weight,
                    stored_at,
                    expires_at,
                    used: next_use(),
                },
            );
            (expired, replaced, expires_at)
        };
        drop(replaced);
        drop(expired);
        // Outside this memo's lock: reclaiming locks each memo in turn.
        if let Some(deadline) = deadline {
            self.budget.note_deadline(deadline);
        }
        self.budget.reclaim();
        // Reclaim can evict this insertion; a concurrent writer can also
        // replace its key, so key presence alone does not prove retention.
        let kept = self
            .lock()
            .entries
            .peek(&key)
            .is_some_and(|entry| Arc::ptr_eq(&entry.value, &value));
        (value, kept)
    }

    pub fn remove<Q: Hash + Eq + ?Sized>(&self, key: &Q)
    where
        K: Borrow<Q>,
    {
        let removed = self.lock().remove(key);
        drop(removed);
    }

    /// Charge `extra` more bytes to the live entry for `key`, for what its
    /// value learned after it was stored. One step under the lock: the entry's
    /// weight, this memo's and the budget's grow together, or none does. It
    /// is refused (false, nothing changed) when no live entry holds `key`, or
    /// when the budget cannot hold `extra` without reclaiming, since reclaiming
    /// could evict this very entry; the caller then keeps nothing it learned.
    pub fn charge<Q: Hash + Eq + ?Sized>(&self, key: &Q, extra: usize) -> bool
    where
        K: Borrow<Q>,
    {
        let mut inner = self.lock();
        let now = Instant::now();
        let Some(entry) = inner.entries.peek_mut(key) else {
            return false;
        };
        if entry.expires_at.is_some_and(|deadline| deadline <= now)
            || !self.budget.try_charge(extra)
        {
            return false;
        }
        entry.weight += extra;
        inner.weight = inner.weight.saturating_add(extra);
        true
    }

    pub fn clear(&self) {
        let removed = self.lock().clear();
        drop(removed);
    }

    /// Entries held, expired ones included until they are released.
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
        let removed = self.lock().unwrap_or_else(PoisonError::into_inner).clear();
        drop(removed);
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
        drop(inner);
        drop((key, entry));
        true
    }

    fn sweep(&self, now: Instant) -> Option<Instant> {
        let mut inner = self.lock().unwrap_or_else(PoisonError::into_inner);
        let expired = inner.expire(now);
        let next = inner.expiry.first().map(|(deadline, _)| *deadline);
        drop(inner);
        drop(expired);
        next
    }
}

#[cfg(test)]
mod tests;
