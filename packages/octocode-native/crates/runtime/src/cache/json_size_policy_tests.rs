#[test]
fn invalid_cache_json_exceeds_every_budget() {
    let invalid = std::collections::BTreeMap::from([((1, 2), "value".to_owned())]);
    let weight = octocode_engine::json_size::bytes(&invalid).unwrap_or(usize::MAX);
    assert_eq!(weight, usize::MAX);
    let store = super::Store::<_>::new(super::CacheConfig::default(), None);
    let key = super::CacheKey {
        namespace: "test".into(),
        scope: "test".into(),
        resource: "invalid".into(),
    };
    let (_, retained) =
        store.put_confirmed(key.clone(), invalid, weight, super::CacheClass::Immutable);
    assert!(!retained);
    assert!(store.get(&key).is_none());
}
