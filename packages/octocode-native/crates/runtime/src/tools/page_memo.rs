//! A short-lived store of results kept for continuation pages, keyed by the
//! result's snapshot and the path policy that produced it. A hit is only a
//! candidate: each owner re-validates what it reads before serving a page.
use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

struct Slot<V> {
    snapshot: String,
    policy: String,
    stored: Instant,
    weight: usize,
    value: V,
}

pub(crate) struct PageMemo<V> {
    slots: Mutex<VecDeque<Slot<V>>>,
    ttl: Duration,
    max_entries: usize,
    max_weight: usize,
}

impl<V> PageMemo<V> {
    /// A store that keeps a value for `ttl`, at most `max_entries` values,
    /// and at most `max_weight` total weight; the oldest go first.
    pub(crate) const fn new(ttl: Duration, max_entries: usize, max_weight: usize) -> Self {
        Self {
            slots: Mutex::new(VecDeque::new()),
            ttl,
            max_entries,
            max_weight,
        }
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<Slot<V>>> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `read` of the unexpired value stored for `snapshot` under `policy`.
    pub(crate) fn get<T>(
        &self,
        snapshot: &str,
        policy: &str,
        read: impl FnOnce(&V) -> T,
    ) -> Option<T> {
        let mut slots = self.lock();
        slots.retain(|slot| slot.stored.elapsed() < self.ttl);
        slots
            .iter()
            .find(|slot| slot.snapshot == snapshot && slot.policy == policy)
            .map(|slot| read(&slot.value))
    }

    /// Store `value`, first dropping expired values, an older value with the
    /// same snapshot, and the oldest values beyond the entry and weight
    /// budgets.
    pub(crate) fn put(&self, snapshot: String, policy: String, weight: usize, value: V) {
        let mut slots = self.lock();
        slots.retain(|kept| kept.stored.elapsed() < self.ttl && kept.snapshot != snapshot);
        let mut total = slots.iter().map(|kept| kept.weight).sum::<usize>();
        while slots.len() >= self.max_entries || total + weight > self.max_weight {
            let Some(oldest) = slots.pop_front() else {
                break;
            };
            total -= oldest.weight;
        }
        slots.push_back(Slot {
            snapshot,
            policy,
            stored: Instant::now(),
            weight,
            value,
        });
    }

    /// Drop the value stored for `snapshot`, as expiry or eviction would.
    pub(crate) fn evict(&self, snapshot: &str) {
        self.lock().retain(|slot| slot.snapshot != snapshot);
    }

    /// Stored snapshots, oldest first, and their total weight.
    #[cfg(test)]
    pub(crate) fn contents(&self) -> (Vec<String>, usize) {
        let slots = self.lock();
        (
            slots.iter().map(|slot| slot.snapshot.clone()).collect(),
            slots.iter().map(|slot| slot.weight).sum(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oldest_values_go_first_to_keep_both_budgets() {
        let memo = PageMemo::new(Duration::from_secs(60), 3, 10);
        for tag in 0..5 {
            memo.put(format!("s{tag}"), "p".into(), 3, ());
        }
        assert_eq!(
            memo.contents(),
            (vec!["s2".into(), "s3".into(), "s4".into()], 9)
        );
        memo.put("big".into(), "p".into(), 8, ());
        assert_eq!(memo.contents(), (vec!["big".into()], 8));
        // A value over the weight budget alone is still kept.
        memo.put("huge".into(), "p".into(), 11, ());
        assert_eq!(memo.contents(), (vec!["huge".into()], 11));
    }

    #[test]
    fn a_value_is_served_under_its_own_policy_until_replaced_or_evicted() {
        let memo = PageMemo::new(Duration::from_secs(60), 4, 100);
        memo.put("s".into(), "a".into(), 1, 1);
        assert_eq!(memo.get("s", "a", |v| *v), Some(1));
        assert_eq!(memo.get("s", "b", |v| *v), None);
        memo.put("s".into(), "a".into(), 1, 2);
        assert_eq!(memo.get("s", "a", |v| *v), Some(2));
        assert_eq!(memo.contents().0.len(), 1);
        memo.evict("s");
        assert_eq!(memo.get("s", "a", |v| *v), None);
    }

    #[test]
    fn an_expired_value_is_not_served() {
        let memo = PageMemo::new(Duration::ZERO, 4, 100);
        memo.put("s".into(), "a".into(), 1, ());
        assert_eq!(memo.get("s", "a", |()| ()), None);
    }
}
