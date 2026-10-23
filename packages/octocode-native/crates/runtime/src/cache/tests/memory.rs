use super::*;
use crate::cache::disk::now_unix;

/// An entry file of the retired layout (a JSON envelope) is never read, and
/// the next prune removes it; a v4 entry beside it stays.
#[test]
fn retired_json_entries_are_removed_by_the_next_prune() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::default(), Some(dir.path().into()));
    let retired = dir.path().join(format!("{}.json", "a".repeat(64)));
    fs::write(
        &retired,
        serde_json::to_vec(&serde_json::json!({
            "class": "immutable", "storedAtUnix": now_unix(), "value": "old"
        }))
        .unwrap(),
    )
    .unwrap();
    store.put(key("kept", "x"), "new".into(), 3, CacheClass::Immutable);
    store.disk.as_ref().unwrap().prune(u64::MAX);
    assert!(!retired.exists(), "the retired layout is discarded");
    assert_eq!(entry_files(dir.path()).len(), 1);
    let reader = Store::<String>::new(CacheConfig::default(), Some(dir.path().into()));
    assert_eq!(
        reader
            .get(&key("kept", "x"))
            .map(|hit| (*hit.value).clone()),
        Some("new".to_owned())
    );
}

#[test]
fn classes_age_differently_in_memory() {
    let store = Store::<u32>::new(CacheConfig::default(), None);
    store.put(key("pinned", "x"), 1, 4, CacheClass::Immutable);
    store.put(key("listing", "x"), 2, 4, revalidate(0));
    store.put(key("search", "x"), 3, 4, volatile(0));
    let pinned = store
        .get(&key("pinned", "x"))
        .expect("immutable never expires");
    assert!(pinned.fresh);
    let listing = store
        .get(&key("listing", "x"))
        .expect("revalidate entries are kept past the fresh window");
    assert!(
        !listing.fresh,
        "past the fresh window the caller revalidates"
    );
    assert!(store.get(&key("search", "x")).is_none(), "volatile expires");
}

#[test]
fn revalidate_entries_are_fresh_inside_the_window() {
    let store = Store::<u32>::new(CacheConfig::default(), None);
    store.put(key("listing", "x"), 2, 4, revalidate(60));
    store.put(key("search", "x"), 3, 4, volatile(60));
    assert!(store.get(&key("listing", "x")).unwrap().fresh);
    assert!(store.get(&key("search", "x")).unwrap().fresh);
}

#[test]
fn memory_is_a_byte_and_entry_bounded_lru() {
    let store = Store::<u32>::new(
        CacheConfig {
            max_entries: 2,
            max_bytes: 5,
            ..CacheConfig::default()
        },
        None,
    );
    store.put(key("a", "x"), 1, 2, CacheClass::Immutable);
    store.put(key("b", "x"), 2, 2, CacheClass::Immutable);
    assert!(store.get(&key("a", "x")).is_some());
    store.put(key("c", "x"), 3, 2, CacheClass::Immutable);
    assert!(
        store.get(&key("b", "x")).is_none(),
        "least recently used goes"
    );
    assert!(store.get(&key("a", "x")).is_some());
    assert!(store.get(&key("c", "x")).is_some());
    store.put(key("big", "x"), 4, 6, CacheClass::Immutable);
    assert!(
        store.get(&key("big", "x")).is_none(),
        "over budget is refused"
    );
}

#[test]
fn zero_entries_disables_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(
        CacheConfig {
            max_entries: 0,
            ..CacheConfig::default()
        },
        Some(dir.path().into()),
    );
    store.put(key("a", "x"), 1, 4, CacheClass::Immutable);
    assert!(store.get(&key("a", "x")).is_none());
    assert!(entry_files(dir.path()).is_empty());
}

#[test]
fn partitions_never_share_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    store.put(key("a", "first"), 1, 4, CacheClass::Immutable);
    let fresh = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    assert_eq!(*fresh.get(&key("a", "first")).unwrap().value, 1);
    assert!(fresh.get(&key("a", "second")).is_none());
    assert!(store.get(&key("a", "second")).is_none());
}
