use super::*;

#[test]
fn current_reads_other_process_progress_and_does_not_revive_evicted_memory() {
    let directory = tempfile::tempdir().expect("cache directory");
    let first = Store::<String>::new(CacheConfig::default(), Some(directory.path().to_owned()));
    let second = Store::<String>::new(CacheConfig::default(), Some(directory.path().to_owned()));
    let key = CacheKey {
        namespace: "assessment".into(),
        scope: "same-access".into(),
        resource: "plan".into(),
    };
    first.put(key.clone(), "pending".into(), 16, CacheClass::Immutable);
    second.put(key.clone(), "complete".into(), 16, CacheClass::Immutable);
    assert_eq!(
        first.get(&key).expect("old memory receipt").value.as_str(),
        "pending"
    );
    assert_eq!(
        first
            .get_current(&key)
            .expect("latest committed progress")
            .value
            .as_str(),
        "complete"
    );
    second.remove(&key);
    assert!(
        first.get_current(&key).is_none(),
        "disk eviction is authoritative"
    );
    assert!(first.get(&key).is_none(), "stale memory was removed");
}

#[test]
fn current_memory_reads_keep_the_memory_only_policy() {
    let store = Store::<String>::new(CacheConfig::default(), None);
    let key = CacheKey {
        namespace: "assessment".into(),
        scope: "session".into(),
        resource: "plan".into(),
    };
    store.put(key.clone(), "retained".into(), 16, CacheClass::Immutable);
    assert_eq!(
        store
            .get_current(&key)
            .expect("in-process progress")
            .value
            .as_str(),
        "retained"
    );
}
