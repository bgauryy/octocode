use super::*;
use crate::cache::disk::now_unix;

/// An entry file of the retired layout (a JSON envelope) is never read, and
/// the next prune removes it; a v4 entry beside it stays.
#[test]
fn retired_json_entries_are_removed_by_the_next_prune() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let retired = dir.path().join(format!("{}.json", "a".repeat(64)));
    fs::write(
        &retired,
        serde_json::to_vec(&serde_json::json!({
            "class": "immutable", "storedAtUnix": now_unix(), "value": "old"
        }))
        .unwrap(),
    )
    .unwrap();
    store.put_confirmed(key("kept", "x"), "new".into(), 3, CacheClass::Immutable);
    store.disk.as_ref().unwrap().prune(u64::MAX);
    assert!(!retired.exists(), "the retired layout is discarded");
    assert_eq!(entry_files(dir.path()).len(), 1);
    let reader = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    assert_eq!(
        reader
            .get(&key("kept", "x"))
            .map(|hit| (*hit.value).clone()),
        Some("new".to_owned())
    );
}

#[test]
fn retained_entries_expire_only_by_their_declared_lifetime() {
    let store = Store::<u32>::new(CacheConfig::derived(), None);
    store.put_confirmed(key("pinned", "x"), 1, 4, CacheClass::Immutable);
    store.put_confirmed(key("expired", "x"), 2, 4, volatile(0));
    store.put_confirmed(key("active", "x"), 3, 4, volatile(60));
    assert_eq!(*store.get(&key("pinned", "x")).unwrap().value, 1);
    assert!(store.get(&key("expired", "x")).is_none());
    assert_eq!(*store.get(&key("active", "x")).unwrap().value, 3);
}

/// The memory tier counts against the one process budget (whose eviction
/// order `octocode_io::memo` tests); only an entry heavier than the whole
/// budget is refused outright.
#[test]
fn memory_draws_on_the_process_budget_and_refuses_oversized_entries() {
    let store = Store::<u32>::new(CacheConfig::derived(), None);
    let before = octocode_engine::memory::MEMORY.used();
    assert!(
        store
            .put_confirmed(key("a", "x"), 1, 4096, CacheClass::Immutable)
            .1
    );
    assert!(octocode_engine::memory::MEMORY.used() >= before + 4096);
    assert!(store.get(&key("a", "x")).is_some());
    let (_, kept) = store.put_confirmed(
        key("big", "x"),
        4,
        octocode_engine::memory::MEMORY.limit() + 1,
        CacheClass::Immutable,
    );
    assert!(!kept, "heavier than the whole budget is refused");
    assert!(store.get(&key("big", "x")).is_none());
}

#[test]
fn partitions_never_share_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(CacheConfig::derived(), Some(dir.path().into()));
    store.put_confirmed(key("a", "first"), 1, 4, CacheClass::Immutable);
    let fresh = Store::<u32>::new(CacheConfig::derived(), Some(dir.path().into()));
    assert_eq!(*fresh.get(&key("a", "first")).unwrap().value, 1);
    assert!(fresh.get(&key("a", "second")).is_none());
    assert!(store.get(&key("a", "second")).is_none());
}

/// An idle entry lives while it is read: each read restarts its lifetime,
/// and it expires only after a full lifetime without one. A volatile entry
/// stored at the same moment expires on schedule whatever its reads.
#[test]
fn idle_entries_live_while_read_and_expire_when_unused() {
    let ttl = Duration::from_millis(400);
    let store = Store::<u32>::new(CacheConfig::derived(), None);
    store.put_confirmed(key("walk", "x"), 1, 4, CacheClass::Idle { ttl });
    store.put_confirmed(key("fixed", "x"), 2, 4, CacheClass::Volatile { ttl });
    for _ in 0..4 {
        std::thread::sleep(ttl / 2);
        assert!(
            store.get(&key("walk", "x")).is_some(),
            "read within its lifetime"
        );
    }
    assert!(
        store.get(&key("fixed", "x")).is_none(),
        "a volatile entry ages from its store time"
    );
    std::thread::sleep(ttl + ttl / 4);
    assert!(
        store.get(&key("walk", "x")).is_none(),
        "unused for its lifetime"
    );
}

/// No literal entry cap: under the derived policy an envelope far larger than
/// the retired 32 MiB cap is retained in memory and on disk, and the disk
/// budget follows the memory budget.
#[test]
fn derived_policy_retains_entries_over_the_retired_fixed_cap() {
    const RETIRED_CAP: usize = 32 * 1024 * 1024;
    let size = RETIRED_CAP + 8 * 1024 * 1024;
    assert!(size <= octocode_engine::memory::MIN_MEMORY_BUDGET);
    assert_eq!(
        CacheConfig::derived().disk_bytes,
        octocode_engine::memory::MEMORY.limit() as u64
    );
    let memory = Store::<u8>::new(CacheConfig::derived(), None);
    assert!(
        memory
            .put_confirmed(key("big", "x"), 1, size, CacheClass::Immutable)
            .1
    );
    let dir = tempfile::tempdir().unwrap();
    let disk = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let (_, saved) = disk.put_confirmed(
        key("big", "x"),
        "x".repeat(size),
        size,
        CacheClass::Immutable,
    );
    assert!(saved);
    assert_eq!(disk.get(&key("big", "x")).unwrap().value.len(), size);
}
