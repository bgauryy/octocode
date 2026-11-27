use super::*;

#[test]
fn publication_read_preflight_matches_actual_allocation_and_items() {
    let mut large_catalog = table();
    large_catalog.descriptors.snapshots[0].path = "long/path/".repeat(4096);
    large_catalog.descriptors.representations[0]
        .producer
        .version = "version".repeat(1000);
    for value in [SourceTable::default(), table(), large_catalog] {
        let encoded = encode(&value, &NeverCancel).unwrap();
        let mut admitted = budget::Budget::new(&NeverCancel, &LIMITS);
        value.admit_read(&mut admitted).unwrap();
        let mut actual = budget::Budget::new(&NeverCancel, &LIMITS);
        assert_eq!(decode_with_budget(&encoded, &mut actual).unwrap(), value);
        assert_eq!(
            (admitted.bytes, admitted.items),
            (actual.bytes, actual.items)
        );
        for (bytes, items) in [
            (actual.bytes, actual.items),
            (actual.bytes.saturating_sub(1), actual.items),
            (actual.bytes, actual.items.saturating_sub(1)),
        ] {
            let mut admission = budget::Budget::new(&NeverCancel, &bounded(bytes, items));
            let mut reader = budget::Budget::new(&NeverCancel, &bounded(bytes, items));
            assert_eq!(
                value.admit_read(&mut admission).is_ok(),
                decode_with_budget(&encoded, &mut reader).is_ok(),
                "bytes={bytes} items={items}"
            );
        }
    }
}

/// Each block of origins is admitted on its own, by the writer, the
/// publication preflight and the reader alike: its index entries and every
/// origin's decode. A block of one origin whose decode charges six items
/// charges seven, so it is refused at six and admitted at seven, whatever
/// the pool's other blocks charge.
#[test]
fn origin_blocks_are_admitted_on_their_own_at_tiny_boundary() {
    let (descriptors, origins) = fixture();
    let bound = |items| budget::Budget::new(&NeverCancel, &bounded(usize::MAX, items));
    let mut refused = pool::OriginPool::default();
    assert!(matches!(
        refused.extend(&[&origins[1]], &descriptors, &mut bound(6)),
        Err(SourceError::LimitExceeded(_))
    ));
    assert_eq!(
        refused,
        pool::OriginPool::default(),
        "a refused origin is not kept"
    );
    let mut pool = pool::OriginPool::default();
    pool.extend(&[&origins[1]], &descriptors, &mut bound(7))
        .unwrap();
    assert_eq!(pool.get(0).unwrap().items, 6);
    let mut encoded = Vec::new();
    pool.write(
        &mut encoded,
        &mut budget::Budget::new(&NeverCancel, &LIMITS),
    )
    .unwrap();
    let mut narrow = bound(6);
    assert!(matches!(
        pool::OriginPool::read(&mut budget::Input::new(&encoded, &mut narrow), &descriptors),
        Err(SourceError::LimitExceeded(_))
    ));
    // This is the preflight production validation calls before publish.
    assert!(matches!(
        pool.admit_read(&mut bound(6)),
        Err(SourceError::LimitExceeded(_))
    ));
    let mut admission = bound(7);
    pool.admit_read(&mut admission).unwrap();
    let mut reader = bound(7);
    pool::OriginPool::read(&mut budget::Input::new(&encoded, &mut reader), &descriptors).unwrap();
    assert_eq!(
        (admission.bytes, admission.items),
        (reader.bytes, reader.items)
    );
}
