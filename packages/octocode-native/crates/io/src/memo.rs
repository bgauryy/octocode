//! The one in-process memo outside the engine's own caches ([`crate`]):
//! least-recently-used order, an entry cap and a weight cap, a lifetime
//! chosen per entry, shared (`Arc`) values, and a lock that recovers from
//! poisoning instead of skipping work.
//!
//! Single flight is composed, not built in: a caller that must not run the
//! same load twice holds a `KeyedFlight` turn ([`crate::flight`]) around its
//! `get`-then-`insert`.
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A memo's budgets.
#[derive(Clone, Copy, Debug)]
pub struct MemoLimits {
    pub max_entries: usize,
    /// Total weight kept, in the caller's unit (bytes, graph elements...).
    pub max_weight: usize,
    /// Keep the newest entry even when it alone exceeds `max_weight` (and
    /// evict everything else). Otherwise such an entry is not stored.
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
    ttl: Option<Duration>,
    /// This entry's slot in [`Inner::order`].
    tick: u64,
}

impl<V, M> Entry<V, M> {
    fn expired(&self, now: Instant) -> bool {
        self.ttl
            .is_some_and(|ttl| now.saturating_duration_since(self.stored_at) >= ttl)
    }
}

struct Inner<K, V, M> {
    entries: BTreeMap<K, Entry<V, M>>,
    /// Keys by last use: the first tick is the least recently used.
    order: BTreeMap<u64, K>,
    next_tick: u64,
    weight: usize,
}

impl<K: Ord, V, M> Inner<K, V, M> {
    fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<Entry<V, M>>
    where
        K: Borrow<Q>,
    {
        let entry = self.entries.remove(key)?;
        self.weight = self.weight.saturating_sub(entry.weight);
        self.order.remove(&entry.tick);
        Some(entry)
    }

    fn pop_oldest(&mut self) -> bool {
        let Some((_, oldest)) = self.order.pop_first() else {
            return false;
        };
        if let Some(entry) = self.entries.remove(&oldest) {
            self.weight = self.weight.saturating_sub(entry.weight);
        }
        true
    }
}

pub struct Memo<K, V, M = ()> {
    limits: MemoLimits,
    inner: Mutex<Inner<K, V, M>>,
}

impl<K: Ord + Clone, V, M: Clone> Memo<K, V, M> {
    pub const fn new(limits: MemoLimits) -> Self {
        Self {
            limits,
            inner: Mutex::new(Inner {
                entries: BTreeMap::new(),
                order: BTreeMap::new(),
                next_tick: 0,
                weight: 0,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner<K, V, M>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The unexpired entry for `key`, now the most recently used.
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<Hit<V, M>>
    where
        K: Borrow<Q>,
    {
        self.get_at(key, Instant::now())
    }

    /// [`Self::get`] as of `now`: the clock is a parameter so a test can age
    /// an entry without sleeping, as [`Self::insert_full`] takes `stored_at`.
    fn get_at<Q: Ord + ?Sized>(&self, key: &Q, now: Instant) -> Option<Hit<V, M>>
    where
        K: Borrow<Q>,
    {
        let mut inner = self.lock();
        if inner.entries.get(key)?.expired(now) {
            inner.remove(key);
            return None;
        }
        let tick = inner.next_tick;
        inner.next_tick += 1;
        let inner = &mut *inner;
        let entry = inner.entries.get_mut(key)?;
        if let Some(touched) = inner.order.remove(&entry.tick) {
            inner.order.insert(tick, touched);
            entry.tick = tick;
        }
        Some(Hit {
            value: Arc::clone(&entry.value),
            meta: entry.meta.clone(),
            age: now.saturating_duration_since(entry.stored_at),
        })
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

    /// Store `value` with `meta`, `weight` and `ttl`, aged from `stored_at`.
    /// Expired entries go first, then the least recently used ones until
    /// both budgets hold.
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
        if self.limits.max_entries == 0
            || (weight > self.limits.max_weight && !self.limits.keep_newest)
        {
            return value;
        }
        let now = Instant::now();
        let expired = inner
            .entries
            .iter()
            .filter(|(_, entry)| entry.expired(now))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired {
            inner.remove(&key);
        }
        while !inner.entries.is_empty()
            && (inner.entries.len() >= self.limits.max_entries
                || inner.weight.saturating_add(weight) > self.limits.max_weight)
        {
            inner.pop_oldest();
        }
        let tick = inner.next_tick;
        inner.next_tick += 1;
        inner.order.insert(tick, key.clone());
        inner.weight = inner.weight.saturating_add(weight);
        inner.entries.insert(
            key,
            Entry {
                value: Arc::clone(&value),
                meta,
                weight,
                stored_at,
                ttl,
                tick,
            },
        );
        value
    }

    pub fn remove<Q: Ord + ?Sized>(&self, key: &Q)
    where
        K: Borrow<Q>,
    {
        self.lock().remove(key);
    }

    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.entries.clear();
        inner.order.clear();
        inner.weight = 0;
    }

    /// Entries held, expired ones included until they are next touched.
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Keys from least to most recently used, and their total weight.
    pub fn contents(&self) -> (Vec<K>, usize) {
        let inner = self.lock();
        (inner.order.values().cloned().collect(), inner.weight)
    }
}

#[cfg(test)]
mod tests {
    use super::{Memo, MemoLimits};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const fn limits(max_entries: usize, max_weight: usize) -> MemoLimits {
        MemoLimits {
            max_entries,
            max_weight,
            keep_newest: false,
        }
    }

    #[test]
    fn least_recently_used_entries_go_first_to_keep_both_budgets() {
        let memo = Memo::<&str, u8>::new(limits(3, 10));
        for key in ["a", "b", "c"] {
            memo.insert(key, 0, 3, None);
        }
        assert!(memo.get("a").is_some(), "touching makes `a` the newest");
        memo.insert("d", 0, 3, None);
        assert_eq!(memo.contents(), (vec!["c", "a", "d"], 9));
        memo.insert("big", 0, 8, None);
        assert_eq!(memo.contents(), (vec!["big"], 8));
        memo.insert("huge", 0, 11, None);
        assert_eq!(memo.contents(), (vec!["big"], 8), "over budget: not kept");
        memo.insert("big", 0, 11, None);
        assert!(
            memo.is_empty(),
            "a too-heavy replacement still drops the old value"
        );
    }

    #[test]
    fn keep_newest_keeps_an_overweight_entry_alone() {
        let memo = Memo::<&str, u8>::new(MemoLimits {
            keep_newest: true,
            ..limits(2, 10)
        });
        memo.insert("a", 0, 4, None);
        memo.insert("heavy", 0, 30, None);
        assert_eq!(memo.contents(), (vec!["heavy"], 30));
        memo.insert("b", 0, 4, None);
        assert_eq!(memo.contents(), (vec!["b"], 4));
    }

    #[test]
    fn an_entry_that_exactly_fits_the_weight_budget_is_kept() {
        let memo = Memo::<&str, u8>::new(limits(4, 10));
        memo.insert("exact", 0, 10, None);
        assert_eq!(memo.contents(), (vec!["exact"], 10), "alone at the cap");

        let memo = Memo::<&str, u8>::new(limits(4, 10));
        memo.insert("a", 0, 3, None);
        memo.insert("b", 0, 7, None);
        assert_eq!(
            memo.contents(),
            (vec!["a", "b"], 10),
            "a total equal to the cap evicts nothing"
        );
        memo.insert("c", 0, 1, None);
        assert_eq!(
            memo.contents(),
            (vec!["b", "c"], 8),
            "one over evicts the oldest"
        );
    }

    #[test]
    fn clear_drops_every_entry_and_frees_the_whole_budget() {
        let memo = Memo::<&str, u8>::new(limits(4, 10));
        memo.insert("a", 0, 6, None);
        memo.insert("b", 0, 4, None);
        memo.clear();
        assert_eq!(memo.len(), 0);
        assert!(memo.get("a").is_none());
        assert_eq!(memo.contents(), (Vec::<&str>::new(), 0));
        memo.insert("c", 0, 10, None);
        assert_eq!(memo.contents(), (vec!["c"], 10));
    }

    #[test]
    fn is_empty_tracks_whether_any_entry_is_held() {
        let memo = Memo::<&str, u8>::new(limits(4, 10));
        assert!(memo.is_empty());
        memo.insert("a", 0, 1, None);
        assert!(!memo.is_empty());
        memo.remove("a");
        assert!(memo.is_empty());
    }

    #[test]
    fn entries_expire_by_their_own_lifetime_and_hits_share_the_value() {
        let memo = Memo::<String, String, u8>::new(limits(8, 100));
        let start = Instant::now();
        memo.insert_full(
            "short".into(),
            "v".to_owned(),
            7,
            1,
            Some(Duration::from_secs(1)),
            start,
        );
        let kept = memo.insert("kept".into(), "w".to_owned(), 1, None);
        let hit = memo.get_at("short", start).expect("fresh");
        assert_eq!((hit.value.as_str(), hit.meta), ("v", 7));
        assert!(
            memo.get_at("short", start + Duration::from_secs(1))
                .is_none()
        );
        assert_eq!(memo.len(), 1, "an expired entry is dropped when touched");
        let hit = memo.get("kept").expect("no lifetime");
        assert!(Arc::ptr_eq(&hit.value, &kept));
    }

    #[test]
    fn a_poisoned_lock_is_recovered_not_skipped() {
        let memo = Arc::new(Memo::<u8, u8>::new(limits(4, 4)));
        memo.insert(1, 1, 1, None);
        let poisoner = Arc::clone(&memo);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.inner.lock().unwrap();
            panic!("poison the memo");
        })
        .join();
        assert!(memo.inner.is_poisoned());
        assert_eq!(memo.get(&1).map(|hit| *hit.value), Some(1));
        memo.insert(2, 2, 1, None);
        memo.remove(&1);
        assert_eq!(memo.contents().0, vec![2]);
    }
}
