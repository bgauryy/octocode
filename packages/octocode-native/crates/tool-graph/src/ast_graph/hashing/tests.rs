use super::*;
use std::collections::{HashMap, HashSet};

#[test]
fn strings_that_differ_only_in_length_or_a_trailing_zero_hash_apart() {
    let hashes = (0..40)
        .flat_map(|len| ["a", "\0"].map(|fill| fill.repeat(len)))
        .map(|text| hash_bytes(text.as_bytes()))
        .collect::<HashSet<_>>();
    // "" and "\0"*0 are one text; every other of the 79 differs.
    assert_eq!(hashes.len(), 79);
}

#[test]
fn maps_on_the_fast_hasher_find_what_they_hold() {
    let mut map: HashMap<String, usize, FastState> = HashMap::default();
    for n in 0..5000 {
        map.insert(format!("declaration:src/lib.rs#name_{n}@12:function"), n);
    }
    for n in 0..5000 {
        assert_eq!(
            map.get(format!("declaration:src/lib.rs#name_{n}@12:function").as_str()),
            Some(&n)
        );
    }
    let mut ids: HashMap<u64, u32, IdentityState> = HashMap::default();
    ids.insert(7, 1);
    assert_eq!(ids.get(&7), Some(&1));
}
