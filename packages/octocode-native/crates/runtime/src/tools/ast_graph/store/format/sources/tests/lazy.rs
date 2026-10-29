//! Accessed admission: the persisted origin index loads without decoding
//! records, and each origin is decoded and validated when it is read.
use super::*;

fn accessed(bytes: &[u8], cancel: &dyn CancellationCheck) -> Result<SourceTable, SourceError> {
    decode_admitted(
        bytes,
        &mut budget::Budget::new(cancel, &LIMITS),
        Admission::Accessed,
    )
}

fn read(table: &SourceTable, reference: u32) -> Result<OriginProjection, String> {
    table
        .origin(reference, &mut ProjectionBudget::default(), &NeverCancel)
        .map_err(|error| error.message)
}

/// Byte offset of `reference`'s encoded record: the records end the section.
fn record(encoded: &[u8], table: &SourceTable, reference: u32) -> usize {
    encoded.len() - table.origins.storage_bytes().0
        + table.origins.get(reference).unwrap().offset as usize
}

#[test]
fn accessed_admission_validates_only_the_origins_read() {
    let value = table();
    let encoded = encode(&value, &NeverCancel).unwrap();
    assert_eq!(accessed(&encoded, &NeverCancel).unwrap(), value);
    // Origin 1's copied raw end (one byte per fixture field, see
    // `truncation_trailing_unknown_tags_and_unselected_corruption_fail`).
    let mut malformed = encoded.clone();
    malformed[record(&encoded, &value, 1) + 9] = 17;
    assert!(
        decode(&malformed, &NeverCancel).is_err(),
        "full admission validates every origin"
    );
    let lazy = accessed(&malformed, &NeverCancel).unwrap();
    assert_eq!(read(&lazy, 0).unwrap(), fixture().1[0]);
    assert!(read(&lazy, 1).is_err(), "a read origin is validated");
    let mut full = budget::Budget::new(&NeverCancel, &LIMITS);
    decode_with_budget(&encoded, &mut full).unwrap();
    let mut index = budget::Budget::new(&NeverCancel, &LIMITS);
    decode_admitted(&encoded, &mut index, Admission::Accessed).unwrap();
    assert!(index.items < full.items && index.bytes < full.bytes);
}

#[test]
fn a_tampered_index_entry_is_rejected_when_its_origin_is_read() {
    for field in 0..4 {
        let mut tampered = table();
        let entry = &mut tampered.origins.entries_mut()[1];
        match field {
            0 => entry.range = ByteRange::new(16, 17),
            1 => entry.bytes += 1,
            2 => entry.items += 1,
            _ => entry.offset -= 1,
        }
        let encoded = encode(&tampered, &NeverCancel).unwrap();
        assert!(decode(&encoded, &NeverCancel).is_err(), "field {field}");
        let lazy = accessed(&encoded, &NeverCancel).unwrap();
        let error = read(&lazy, 1).unwrap_err();
        // A shifted offset also truncates origin 0; the others leave it intact.
        if field < 3 {
            assert!(error.contains("disagrees"), "field {field}: {error}");
            assert_eq!(read(&lazy, 0).unwrap(), fixture().1[0]);
        }
    }
}

#[test]
fn the_index_is_checked_structurally_on_load() {
    let value = table();
    let encoded = encode(&value, &NeverCancel).unwrap();
    for length in 0..encoded.len() {
        assert!(
            accessed(&encoded[..length], &NeverCancel).is_err(),
            "prefix {length}"
        );
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert!(accessed(&trailing, &NeverCancel).is_err());
    // An empty record and a unit outside the catalog are structural errors.
    for field in 0..2 {
        let mut tampered = table();
        let entries = tampered.origins.entries_mut();
        if field == 0 {
            entries[1].offset = entries[0].offset;
        } else {
            entries[0].unit = 1;
        }
        let encoded = encode(&tampered, &NeverCancel).unwrap();
        assert!(accessed(&encoded, &NeverCancel).is_err(), "field {field}");
    }
}

#[test]
fn empty_pools_and_cancellation() {
    let empty = SourceTable::default();
    let encoded = encode(&empty, &NeverCancel).unwrap();
    assert_eq!(accessed(&encoded, &NeverCancel).unwrap(), empty);
    assert_eq!(decode(&encoded, &NeverCancel).unwrap(), empty);
    struct Stop(std::sync::atomic::AtomicUsize);
    impl CancellationCheck for Stop {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) <= 1 {
                Err("test stop".into())
            } else {
                Ok(())
            }
        }
    }
    let encoded = encode(&table(), &NeverCancel).unwrap();
    for polls in [1, 5, 20] {
        let stop = Stop(std::sync::atomic::AtomicUsize::new(polls));
        assert!(matches!(
            accessed(&encoded, &stop),
            Err(SourceError::Cancelled)
        ));
    }
    let lazy = accessed(&encoded, &NeverCancel).unwrap();
    let stop = Stop(std::sync::atomic::AtomicUsize::new(1));
    assert!(
        lazy.origin(0, &mut ProjectionBudget::default(), &stop)
            .is_err()
    );
}
