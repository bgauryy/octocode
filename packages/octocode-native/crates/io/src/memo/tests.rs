use super::{Memo, MemoryBudget};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A budget of `$bytes` of its own: tests run in parallel, so each test
/// that weighs entries owns one.
macro_rules! budget {
    ($bytes:expr) => {{
        fn limit() -> usize {
            $bytes
        }
        static BUDGET: MemoryBudget = MemoryBudget::new(limit);
        &BUDGET
    }};
}

/// Destructors can call back into a memo. Probe its lock without blocking, so
/// a regression fails deterministically instead of hanging the test process.
#[test]
fn removal_paths_drop_values_after_releasing_the_memo_lock() {
    use std::sync::{Mutex, Weak};

    struct Probe {
        memo: Weak<Memo<&'static str, Self>>,
        observations: Arc<Mutex<Vec<bool>>>,
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            if let Some(memo) = self.memo.upgrade() {
                let unlocked = memo.entries.inner.get().unwrap().try_lock().is_ok();
                self.observations.lock().unwrap().push(unlocked);
            }
        }
    }

    for operation in [
        "expired read",
        "replacement",
        "oversized replacement",
        "expired replacement",
        "remove",
        "memo clear",
        "budget clear",
        "budget eviction",
        "sweep",
    ] {
        let budget = budget!(8);
        let memo = Arc::new(Memo::new(budget));
        let observations = Arc::new(Mutex::new(Vec::new()));
        let probe = |observed| Probe {
            memo: if observed {
                Arc::downgrade(&memo)
            } else {
                Weak::new()
            },
            observations: Arc::clone(&observations),
        };
        let start = Instant::now();
        let ttl = Duration::from_secs(600);
        let (_, kept) = memo.insert_full("key", probe(true), (), 1, Some(ttl), start);
        assert!(kept);
        match operation {
            "expired read" => assert!(memo.get_at("key", start + ttl).is_none()),
            "replacement" => {
                memo.insert("key", probe(false), 1, None);
            }
            "oversized replacement" => {
                assert!(!memo.insert("key", probe(false), budget.limit() + 1, None).1);
            }
            "expired replacement" => {
                assert!(!memo.insert("key", probe(false), 1, Some(Duration::ZERO)).1);
            }
            "remove" => memo.remove("key"),
            "memo clear" => memo.clear(),
            "budget clear" => budget.clear(),
            "budget eviction" => {
                drop(budget.reserve(budget.limit()).unwrap());
            }
            "sweep" => {
                budget.sweep(start + ttl);
            }
            _ => unreachable!(),
        }
        assert_eq!(*observations.lock().unwrap(), [true], "{operation}");
        assert_eq!(memo.contents().1, budget.used(), "{operation}: accounting");
        memo.clear();
        assert_eq!(budget.used(), 0);
    }
}

#[test]
fn clear_empties_live_memos_and_dropped_ones_free_their_bytes() {
    let budget = budget!(10);
    let memo = Memo::<&str, u8>::new(budget);
    let (value, kept) = memo.insert("a", 1, 1, None);
    assert!(kept);
    budget.clear();
    assert_eq!(Arc::strong_count(&value), 1);
    assert!(memo.is_empty());
    assert_eq!(budget.used(), 0);
    memo.insert("b", 2, 3, None);
    assert_eq!(budget.used(), 3);
    drop(memo);
    assert_eq!(budget.used(), 0, "a dropped memo returns its bytes");
    budget.clear();
}

#[test]
fn already_expired_inserts_do_not_evict_live_entries() {
    let memo = Memo::<&str, u8>::new(budget!(1));
    memo.insert("live", 1, 1, None);
    let (_, kept) = memo.insert("expired", 2, 1, Some(Duration::ZERO));
    assert!(!kept);
    assert_eq!(*memo.get("live").expect("live value is retained").value, 1);
    assert!(memo.get("expired").is_none());
    assert_eq!(memo.contents(), (vec!["live"], 1));
}

#[test]
fn replacement_has_its_own_expiry_and_old_deadlines_do_not_remove_it() {
    let memo = Memo::<&str, u8>::new(budget!(2));
    let now = Instant::now();
    memo.insert_full("same", 1, (), 1, Some(Duration::from_secs(5)), now);
    memo.insert_full("same", 2, (), 1, Some(Duration::from_secs(20)), now);
    assert_eq!(
        *memo
            .get_at("same", now + Duration::from_secs(6))
            .unwrap()
            .value,
        2
    );
    assert!(memo.get_at("same", now + Duration::from_secs(20)).is_none());
    assert_eq!(memo.contents(), (vec![], 0));
}

#[test]
fn least_recently_used_entries_go_first_to_keep_the_budget() {
    let budget = budget!(10);
    let memo = Memo::<&str, u8>::new(budget);
    for key in ["a", "b", "c"] {
        memo.insert(key, 0, 3, None);
    }
    assert!(memo.get("a").is_some(), "touching makes `a` the newest");
    memo.insert("d", 0, 3, None);
    assert_eq!(memo.contents(), (vec!["c", "a", "d"], 9));
    memo.insert("big", 0, 8, None);
    assert_eq!(memo.contents(), (vec!["big"], 8));
    let (_, kept) = memo.insert("huge", 0, 11, None);
    assert!(!kept);
    assert_eq!(memo.contents(), (vec!["big"], 8), "over budget: not kept");
    memo.insert("big", 0, 11, None);
    assert!(
        memo.is_empty(),
        "a too-heavy replacement still drops the old value"
    );
    assert_eq!(budget.used(), 0);
}

/// The budget is shared: an insertion into one memo evicts the oldest entry
/// of any memo on the same budget, whatever their value types.
#[test]
fn memos_on_one_budget_evict_each_others_oldest_entries_first() {
    let budget = budget!(10);
    let words = Memo::<&str, String>::new(budget);
    let numbers = Memo::<u8, u64>::new(budget);
    words.insert("old", "w".to_owned(), 4, None);
    numbers.insert(1, 1, 4, None);
    assert!(words.get("old").is_some(), "a read makes `old` the newest");
    numbers.insert(2, 2, 4, None);
    assert_eq!(numbers.contents(), (vec![2], 4), "1 was the oldest overall");
    assert_eq!(words.contents(), (vec!["old"], 4));
    words.insert("new", "n".to_owned(), 6, None);
    assert_eq!(words.contents(), (vec!["new"], 6), "`old` went first");
    assert_eq!(numbers.contents(), (vec![2], 4));
    words.insert("more", "m".to_owned(), 1, None);
    assert_eq!(words.contents(), (vec!["new", "more"], 7));
    assert!(numbers.is_empty(), "another memo's oldest entry made room");
    assert_eq!(budget.used(), 7);
}

#[test]
fn an_entry_that_exactly_fits_the_budget_is_kept() {
    let memo = Memo::<&str, u8>::new(budget!(10));
    memo.insert("exact", 0, 10, None);
    assert_eq!(memo.contents(), (vec!["exact"], 10), "alone at the cap");

    let memo = Memo::<&str, u8>::new(budget!(10));
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
    let budget = budget!(10);
    let memo = Memo::<&str, u8>::new(budget);
    memo.insert("a", 0, 6, None);
    memo.insert("b", 0, 4, None);
    memo.clear();
    assert_eq!(memo.len(), 0);
    assert!(memo.get("a").is_none());
    assert_eq!(memo.contents(), (Vec::<&str>::new(), 0));
    assert_eq!(budget.used(), 0);
    memo.insert("c", 0, 10, None);
    assert_eq!(memo.contents(), (vec!["c"], 10));
}

#[test]
fn is_empty_tracks_whether_any_entry_is_held() {
    let memo = Memo::<&str, u8>::new(budget!(10));
    assert!(memo.is_empty());
    memo.insert("a", 0, 1, None);
    assert!(!memo.is_empty());
    memo.remove("a");
    assert!(memo.is_empty());
}

#[test]
fn entries_expire_by_their_own_lifetime_and_hits_share_the_value() {
    let budget = budget!(100);
    let memo = Memo::<String, String, u8>::new(budget);
    let start = Instant::now();
    memo.insert_full(
        "short".into(),
        "v".to_owned(),
        7,
        1,
        Some(Duration::from_secs(1)),
        start,
    );
    let (kept, _) = memo.insert("kept".into(), "w".to_owned(), 1, None);
    let hit = memo.get_at("short", start).expect("fresh");
    assert_eq!((hit.value.as_str(), hit.meta), ("v", 7));
    assert!(
        memo.get_at("short", start + Duration::from_secs(1))
            .is_none()
    );
    assert_eq!(memo.len(), 1, "an expired entry is dropped when touched");
    assert_eq!(budget.used(), 1, "and returns its bytes");
    let hit = memo.get("kept").expect("no lifetime");
    assert!(Arc::ptr_eq(&hit.value, &kept));
}

#[test]
fn a_poisoned_lock_is_recovered_not_skipped() {
    let memo = Arc::new(Memo::<u8, u8>::new(budget!(4)));
    memo.insert(1, 1, 1, None);
    let poisoner = Arc::clone(&memo);
    let _ = std::thread::spawn(move || {
        let _guard = poisoner.entries.inner.get().unwrap().lock().unwrap();
        panic!("poison the memo");
    })
    .join();
    assert!(memo.entries.inner.get().unwrap().is_poisoned());
    assert_eq!(memo.get(&1).map(|hit| *hit.value), Some(1));
    memo.insert(2, 2, 1, None);
    memo.remove(&1);
    assert_eq!(memo.contents().0, vec![2]);
}

/// No call reaches either memo after its inserts: the sweep alone drops what
/// expired, in every memo on the budget, and the values are really gone.
#[test]
fn a_sweep_releases_what_expired_in_every_idle_memo() {
    let budget = budget!(100);
    let words = Memo::<&str, String>::new(budget);
    let numbers = Memo::<u8, u64>::new(budget);
    let start = Instant::now();
    let ttl = Some(Duration::from_secs(600));
    let (word, _) = words.insert_full("old", "w".to_owned(), (), 40, ttl, start);
    let (number, _) = numbers.insert_full(1, 1, (), 30, ttl, start);
    let (kept, _) = numbers.insert(2, 2, 7, None);
    assert_eq!(budget.used(), 77);

    assert_eq!(budget.sweep(start + Duration::from_secs(600)), None);

    assert_eq!(Arc::strong_count(&word), 1, "dropped by the sweep");
    assert_eq!(Arc::strong_count(&number), 1, "in the other memo too");
    assert_eq!(Arc::strong_count(&kept), 2, "no lifetime, not swept");
    assert_eq!(budget.used(), 7);
    assert_eq!(words.contents(), (vec![], 0));
    assert_eq!(numbers.contents(), (vec![2], 7));
}

/// What the sweeper sleeps on: the next deadline, or nothing at all when no
/// entry can come due.
#[test]
fn a_sweep_names_the_next_deadline_and_none_when_nothing_can_come_due() {
    let budget = budget!(100);
    let memo = Memo::<&str, u8>::new(budget);
    let start = Instant::now();
    assert_eq!(budget.sweep(start), None, "no memo has an entry");
    memo.insert("forever", 0, 1, None);
    assert_eq!(budget.sweep(start), None, "no entry has a deadline");

    let soon = Duration::from_secs(5);
    let later = Duration::from_secs(9);
    let other = Memo::<u8, u8>::new(budget);
    memo.insert_full("later", 0, (), 1, Some(later), start);
    other.insert_full(1, 0, (), 1, Some(soon), start);
    assert_eq!(budget.sweep(start), Some(start + soon));
    assert_eq!(budget.used(), 3, "nothing is due yet");
    assert_eq!(budget.sweep(start + soon), Some(start + later));
    assert_eq!(budget.used(), 2);
}

/// A read restarts a value's lifetime: the sweep of its old deadline keeps it,
/// and next comes due at the new one.
#[test]
fn a_touched_entry_outlives_the_sweep_of_its_old_deadline() {
    let budget = budget!(100);
    let memo = Memo::<&str, u8>::new(budget);
    let start = Instant::now();
    memo.insert_full("paged", 7, (), 1, Some(Duration::from_secs(5)), start);
    memo.touch("paged", Duration::from_secs(3600));

    let next = budget
        .sweep(start + Duration::from_secs(6))
        .expect("still has a deadline");

    assert!(next >= start + Duration::from_secs(3600));
    assert_eq!(*memo.get("paged").expect("still served").value, 7);
    assert_eq!(budget.sweep(next), None);
    assert!(memo.is_empty());
}

/// The running sweeper, on a real clock: a deadline earlier than the one it
/// sleeps on wakes it, and nothing touches the memo again.
#[test]
fn the_running_sweeper_releases_an_idle_memo_by_itself() {
    let budget = budget!(100);
    budget.sweep_expired_while_running();
    budget.sweep_expired_while_running();
    let memo = Memo::<&str, u8>::new(budget);
    let (far, _) = memo.insert("far", 0, 1, Some(Duration::from_secs(3600)));
    let (near, _) = memo.insert("near", 0, 1, Some(Duration::from_millis(20)));
    let give_up = Instant::now() + Duration::from_secs(60);
    while Arc::strong_count(&near) > 1 {
        assert!(Instant::now() < give_up, "the sweeper never ran");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(Arc::strong_count(&far), 2);
    assert_eq!(budget.used(), 1);
}

/// Charged bytes count toward the entry, the memo and the budget alike.
#[test]
fn a_charge_grows_the_entry_memo_and_budget_together() {
    let budget = budget!(100);
    let memo = Memo::<&str, u8>::new(budget);
    memo.insert("a", 1, 10, None);
    for _ in 0..3 {
        assert!(memo.charge("a", 7));
    }
    assert_eq!(memo.contents().1, 31);
    assert_eq!(budget.used(), 31);
    assert!(!memo.charge("missing", 1), "no entry, no charge");
    assert_eq!(budget.used(), 31);
}

/// A charge the budget cannot hold without reclaiming is refused whole,
/// and the entry keeps its weight: reclaiming could evict that entry.
#[test]
fn a_charge_past_the_budget_is_refused_and_changes_nothing() {
    let budget = budget!(20);
    let memo = Memo::<&str, u8>::new(budget);
    memo.insert("a", 1, 20, None);
    assert!(!memo.charge("a", 1));
    assert_eq!((memo.contents().1, budget.used()), (20, 20));
    assert!(memo.get("a").is_some(), "the charged entry stays");
}

/// Concurrent charges are counted exactly once each.
#[test]
fn concurrent_charges_keep_totals_exact() {
    let budget = budget!(1_000_000);
    let memo = Arc::new(Memo::<&str, u8>::new(budget));
    memo.insert("a", 1, 1, None);
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let memo = Arc::clone(&memo);
            std::thread::spawn(move || (0..500).filter(|_| memo.charge("a", 3)).count())
        })
        .collect();
    let charged: usize = threads.into_iter().map(|t| t.join().expect("thread")).sum();
    assert_eq!(charged, 4000);
    assert_eq!(memo.contents().1, 1 + 3 * charged);
    assert_eq!(budget.used(), 1 + 3 * charged);
}

/// Different memos hold different locks. Synchronize their entry lookups so
/// both ask for the budget's last bytes together, rather than relying on a
/// scheduler to expose a check-then-charge race.
#[test]
fn concurrent_charges_on_distinct_memos_cannot_exceed_the_shared_budget() {
    use std::cell::RefCell;
    use std::hash::{Hash, Hasher};
    use std::sync::Barrier;

    thread_local! {
        static LOOKUP_GATE: RefCell<Option<Arc<Barrier>>> = const { RefCell::new(None) };
    }
    #[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
    struct Key;
    impl Hash for Key {
        fn hash<H: Hasher>(&self, state: &mut H) {
            LOOKUP_GATE.with_borrow(|gate| {
                if let Some(gate) = gate {
                    gate.wait();
                }
            });
            0_u8.hash(state);
        }
    }

    let budget = budget!(10);
    let first = Memo::<Key, u8>::new(budget);
    let second = Memo::<Key, u8>::new(budget);
    first.insert(Key, 1, 1, None);
    second.insert(Key, 2, 1, None);
    let extra = budget.limit() - budget.used();
    let gate = Arc::new(Barrier::new(2));
    let charge = |memo: &Memo<Key, u8>| {
        LOOKUP_GATE.with_borrow_mut(|slot| *slot = Some(Arc::clone(&gate)));
        memo.charge(&Key, extra)
    };
    let accepted = std::thread::scope(|scope| {
        let a = scope.spawn(|| charge(&first));
        let b = scope.spawn(|| charge(&second));
        usize::from(a.join().expect("first charge")) + usize::from(b.join().expect("second charge"))
    });
    assert_eq!(accepted, 1, "only one memo can claim the remaining bytes");
    assert_eq!(budget.used(), budget.limit());
    assert_eq!(first.contents().1 + second.contents().1, budget.used());
}

#[test]
fn an_insert_evicted_by_live_reservations_reports_that_it_was_not_kept() {
    let budget = budget!(10);
    let memo = Memo::<&str, u8>::new(budget);
    let held = budget
        .reserve(budget.limit())
        .expect("reserve whole budget");
    let (_, kept) = memo.insert("evicted", 1, 1, None);
    assert!(!kept, "an insertion evicted during reclaim was not kept");
    assert!(memo.get("evicted").is_none());
    assert_eq!(budget.used(), held.bytes());
}

#[test]
fn a_reservation_counts_against_the_budget_and_returns_on_drop() {
    let budget = budget!(10);
    let mut held = budget.reserve(4).expect("fits");
    assert_eq!(budget.used(), 4);
    assert!(held.grow(6), "the budget's last bytes are granted");
    assert_eq!(budget.used(), 10);
    assert!(!held.grow(1), "past the limit is refused");
    assert_eq!(budget.used(), 10, "a refusal holds nothing");
    drop(held);
    assert_eq!(budget.used(), 0);
}

#[test]
fn a_reservation_evicts_memo_entries_to_fit_but_never_past_the_limit() {
    let budget = budget!(10);
    let memo = Memo::<&str, u8>::new(budget);
    memo.insert("old", 1, 6, None);
    let held = budget.reserve(8).expect("evicting the memo makes room");
    assert!(memo.get("old").is_none());
    assert_eq!(budget.used(), 8);
    assert!(budget.reserve(3).is_none(), "nothing left to evict");
    assert_eq!(budget.used(), 8);
    drop(held);
    assert!(budget.reserve(budget.limit() + 1).is_none());
    assert_eq!(budget.used(), 0);
}

/// A hit's age counts from when its entry was stored; a touch restarts it.
/// Touching a key with no entry stores nothing.
#[test]
fn a_hit_reports_its_age_and_a_touch_restarts_it() {
    let memo = Memo::<&str, u8>::new(budget!(10));
    let start = Instant::now();
    memo.insert_full("a", 1, (), 1, None, start);
    let hit = memo
        .get_at("a", start + Duration::from_secs(5))
        .expect("stored");
    assert_eq!(hit.age, Duration::from_secs(5));

    let before = Instant::now();
    memo.touch("a", Duration::from_secs(60));
    let after = Instant::now();
    let read_at = after + Duration::from_secs(3);
    let age = memo.get_at("a", read_at).expect("touched").age;
    assert!(age >= Duration::from_secs(3), "{age:?}");
    assert!(age <= read_at - before, "{age:?}");

    memo.touch("missing", Duration::from_secs(60));
    assert_eq!(memo.contents(), (vec!["a"], 1));
}

/// Memos with one key type keep their own entries: the same key in each
/// answers with that memo's value and metadata.
#[test]
fn memos_with_one_key_type_keep_their_own_values_and_metadata() {
    let budget = budget!(100);
    let words = Memo::<&str, String, u8>::new(budget);
    let numbers = Memo::<&str, u64, String>::new(budget);
    let now = Instant::now();
    words.insert_full("k", "word".to_owned(), 7, 2, None, now);
    numbers.insert_full("k", 42, "meta".to_owned(), 3, None, now);
    for _ in 0..2 {
        let word = words.get("k").expect("word");
        assert_eq!((word.value.as_str(), word.meta), ("word", 7));
        let number = numbers.get("k").expect("number");
        assert_eq!((*number.value, number.meta.as_str()), (42, "meta"));
    }
    words.remove("k");
    assert!(words.get("k").is_none());
    assert_eq!(*numbers.get("k").expect("still held").value, 42);
    assert_eq!(budget.used(), 3);
}

/// An entry past its deadline takes no charge, though it is still held
/// until something releases it.
#[test]
fn a_charge_to_an_expired_entry_is_refused() {
    let budget = budget!(100);
    let memo = Memo::<&str, u8>::new(budget);
    let ttl = Duration::from_millis(20);
    let (_, kept) = memo.insert("short", 1, 1, Some(ttl));
    assert!(kept);
    std::thread::sleep(ttl + Duration::from_millis(10));
    assert!(!memo.charge("short", 5));
    assert_eq!((memo.contents().1, budget.used()), (1, 1));
}

/// Metadata is released with its entry, after the memo's lock, like the value.
#[test]
fn metadata_is_dropped_with_its_entry_outside_the_memo_lock() {
    use std::sync::{Mutex, Weak};

    #[derive(Clone)]
    struct Probe {
        memo: Weak<Memo<&'static str, u8, Self>>,
        observations: Arc<Mutex<Vec<bool>>>,
    }
    impl Drop for Probe {
        fn drop(&mut self) {
            if let Some(memo) = self.memo.upgrade() {
                let unlocked = memo.entries.inner.get().unwrap().try_lock().is_ok();
                self.observations.lock().unwrap().push(unlocked);
            }
        }
    }

    let memo = Arc::new(Memo::new(budget!(8)));
    let observations = Arc::new(Mutex::new(Vec::new()));
    let probe = Probe {
        memo: Arc::downgrade(&memo),
        observations: Arc::clone(&observations),
    };
    memo.insert_full("key", 1, probe, 1, None, Instant::now());
    let hit = memo.get("key").expect("stored");
    drop(hit);
    assert_eq!(*observations.lock().unwrap(), [true], "a hit's copy");
    memo.remove("key");
    assert_eq!(
        *observations.lock().unwrap(),
        [true, true],
        "the entry's own"
    );
}
