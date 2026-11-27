use super::Strings;

#[test]
fn front_coded_entries_rebuild_each_string() {
    let mut strings = Strings::default();
    strings.push_front_coded(None, 0, b"alpha").unwrap();
    strings.push_front_coded(Some(0), 3, b"ine").unwrap();
    // "è" (c3 a8) then "é" (c3 a9): only the leading byte is shared.
    strings
        .push_front_coded(Some(1), 0, "è".as_bytes())
        .unwrap();
    strings.push_front_coded(Some(2), 1, &[0xa9]).unwrap();
    assert_eq!(
        strings.iter().collect::<Vec<_>>(),
        ["alpha", "alpine", "è", "é"]
    );
    assert!(strings.push_front_coded(Some(3), 3, b"x").is_err());
    assert!(strings.push_front_coded(Some(3), 1, &[0xff]).is_err());
    assert!(strings.push_front_coded(Some(9), 0, b"x").is_err());
    assert_eq!(strings.len(), 4);
}

/// A crafted entry that keeps only the leading byte of a multi-byte
/// character and appends a byte no character continues with is refused,
/// never a panic; the same holds for a whole section read.
#[test]
fn an_entry_splitting_a_character_is_refused() {
    let mut strings = Strings::default();
    strings.push_front_coded(None, 0, "é".as_bytes()).unwrap();
    assert_eq!(
        strings.push_front_coded(Some(0), 1, &[0xff]),
        Err("graph string table is not UTF-8".into())
    );
    assert_eq!(strings.iter().collect::<Vec<_>>(), ["é"]);
    // "é", then one shared byte and 0xff: the same entry in a section.
    let section = [2, 0, 2, 0xc3, 0xa9, 1, 1, 0xff];
    let mut t = super::super::GraphTables::default();
    assert_eq!(
        super::super::read_strings(
            &mut super::super::Reader::new(&section, "STRS"),
            &mut t,
            &super::super::LIMITS
        ),
        Err("graph string table is not UTF-8".into())
    );
}

/// A section is strictly sorted: string ids order its strings.
#[test]
fn an_entry_not_after_its_predecessor_is_refused() {
    let mut strings = Strings::default();
    strings.push_front_coded(None, 0, b"").unwrap();
    strings.push_front_coded(Some(0), 0, b"beta").unwrap();
    for (shared, suffix) in [(0, &b"alpha"[..]), (4, b""), (1, b"a"), (2, b"t")] {
        assert_eq!(
            strings.push_front_coded(Some(1), shared, suffix),
            Err("graph string table is not sorted".into()),
            "{shared} {suffix:?}"
        );
    }
    strings.push_front_coded(Some(1), 4, b"x").unwrap();
    assert_eq!(strings.iter().collect::<Vec<_>>(), ["", "beta", "betax"]);
}

/// A build holds each string's bytes once: the builder's strings are
/// moved in, and a replacement edits only its own string.
#[test]
fn built_strings_are_moved_in_and_replaced_in_place() {
    let values = vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()];
    let pointers = values
        .iter()
        .map(|value| value.as_ptr())
        .collect::<Vec<_>>();
    let mut strings = Strings::from(values);
    assert_eq!(
        strings.iter().map(str::as_ptr).collect::<Vec<_>>(),
        pointers
    );
    strings.replace([(1, "b".to_owned())]);
    assert_eq!(strings[0].as_ptr(), pointers[0]);
    assert_eq!(strings[2].as_ptr(), pointers[2]);
    assert_eq!(strings.iter().collect::<Vec<_>>(), ["alpha", "b", "gamma"]);
}

/// Several replacements of one id: the last one given wins, in both
/// representations.
#[test]
fn the_last_replacement_of_an_id_wins() {
    let built = Strings::from(vec!["a".to_owned(), "b".to_owned()]);
    let decoded = ["a", "b"].into_iter().collect::<Strings>();
    for mut strings in [built, decoded] {
        strings.replace((0..64).map(|round| (0, format!("x{round}"))));
        assert_eq!(strings.iter().collect::<Vec<_>>(), ["x63", "b"]);
    }
}

#[test]
fn replacements_keep_every_other_string() {
    let mut strings = ["a", "bb", "c"].into_iter().collect::<Strings>();
    strings.replace([(2, "z".to_owned()), (0, "yy".to_owned())]);
    assert_eq!(strings.iter().collect::<Vec<_>>(), ["yy", "bb", "z"]);
    assert_eq!(strings, ["yy", "bb", "z"].into_iter().collect::<Strings>());
}
