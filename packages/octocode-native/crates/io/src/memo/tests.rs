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
fn registry_clears_live_memos_without_retaining_dropped_ones() {
    let registry = super::Registry::default();
    let memo = Memo::<&str, u8>::new(limits(2, 2));
    let value = memo.insert("a", 1, 1, None);
    registry.register(memo.inner.get().unwrap().clone());
    registry.clear();
    assert_eq!(Arc::strong_count(&value), 1);
    assert!(memo.is_empty());
    drop(memo);
    assert!(registry.0.lock().unwrap()[0].upgrade().is_none());
}

#[test]
fn already_expired_inserts_do_not_evict_live_entries() {
    let memo = Memo::<&str, u8>::new(limits(1, 1));
    memo.insert("live", 1, 1, None);
    memo.insert("expired", 2, 1, Some(Duration::ZERO));
    assert_eq!(*memo.get("live").expect("live value is retained").value, 1);
    assert!(memo.get("expired").is_none());
    assert_eq!(memo.contents(), (vec!["live"], 1));
}

#[test]
fn replacement_has_its_own_expiry_and_old_deadlines_do_not_remove_it() {
    let memo = Memo::<&str, u8>::new(limits(2, 2));
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
        let _guard = poisoner.inner.get().unwrap().lock().unwrap();
        panic!("poison the memo");
    })
    .join();
    assert!(memo.inner.get().unwrap().is_poisoned());
    assert_eq!(memo.get(&1).map(|hit| *hit.value), Some(1));
    memo.insert(2, 2, 1, None);
    memo.remove(&1);
    assert_eq!(memo.contents().0, vec![2]);
}
