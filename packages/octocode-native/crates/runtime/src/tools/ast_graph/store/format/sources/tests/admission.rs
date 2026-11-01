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

#[test]
fn index_items_are_admitted_before_publication_at_tiny_boundary() {
    let (descriptors, origins) = fixture();
    let mut pool = pool::OriginPool::default();
    let mut old_writer = budget::Budget::new(&NeverCancel, &bounded(usize::MAX, 6));
    pool.extend(&[&origins[1]], &descriptors, &mut old_writer)
        .unwrap();
    assert_eq!(old_writer.items, 6);
    let mut encoded = Vec::new();
    pool.write(
        &mut encoded,
        &mut budget::Budget::new(&NeverCancel, &LIMITS),
    )
    .unwrap();
    let mut old_reader = budget::Budget::new(&NeverCancel, &bounded(usize::MAX, 6));
    let mut input = budget::Input::new(&encoded, &mut old_reader);
    assert!(matches!(
        pool::OriginPool::read(&mut input, &descriptors),
        Err(SourceError::LimitExceeded(_))
    ));
    // This is the preflight now called by production validation before publish.
    let mut admission = budget::Budget::new(&NeverCancel, &bounded(usize::MAX, 6));
    assert!(matches!(
        pool.admit_read(&mut admission),
        Err(SourceError::LimitExceeded(_))
    ));
    let mut admission = budget::Budget::new(&NeverCancel, &bounded(usize::MAX, 7));
    pool.admit_read(&mut admission).unwrap();
    let mut reader = budget::Budget::new(&NeverCancel, &bounded(usize::MAX, 7));
    pool::OriginPool::read(&mut budget::Input::new(&encoded, &mut reader), &descriptors).unwrap();
    assert_eq!(
        (admission.bytes, admission.items),
        (reader.bytes, reader.items)
    );
}
