use super::*;

#[test]
fn every_spec_code_has_one_name_and_others_have_none() {
    let names: Vec<_> = (1..=26)
        .map(|code| name(code).expect("spec kind"))
        .collect();
    let mut distinct = names.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 26);
    assert_eq!(
        (names[0], names[11], names[21]),
        ("file", "function", "enumMember")
    );
    assert_eq!(name(0), None);
    assert_eq!(name(27), None);
    assert_eq!(name(u64::MAX), None);
}
