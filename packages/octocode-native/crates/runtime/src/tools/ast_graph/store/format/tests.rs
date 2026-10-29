use super::writer::{put, zigzag};
use super::*;

#[test]
fn front_coded_strings_cannot_expand_past_decode_budget() {
    // Two identical strings occupy five payload bytes but expand to six.
    let bytes = [2, 0, 3, b'a', b'b', b'c', 3, 0];
    let mut tables = GraphTables::default();
    let mut reader = Reader::new(&bytes, "STRS");
    let limits = Limits {
        string_bytes: 5,
        ..LIMITS
    };
    assert!(read_strings(&mut reader, &mut tables, &limits).is_err());
}

#[test]
fn empty_front_coded_strings_cannot_bypass_item_budget() {
    let bytes = [3, 0, 0, 0, 0, 0, 0];
    let mut tables = GraphTables::default();
    let mut reader = Reader::new(&bytes, "STRS");
    let limits = Limits {
        string_bytes: 100,
        table_items: 2,
        ..LIMITS
    };
    assert!(read_strings(&mut reader, &mut tables, &limits).is_err());
}

#[test]
fn varints_and_zigzag_round_trip() {
    for value in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
        let mut out = Vec::new();
        put(&mut out, value);
        assert_eq!(Reader::new(&out, "t").var().unwrap(), value);
    }
    for value in [0i64, 1, -1, 1_000_000, -1_000_000] {
        assert_eq!(unzigzag(zigzag(value)), value);
    }
}
