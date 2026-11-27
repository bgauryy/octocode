use super::Spelled;

#[test]
fn a_span_holds_only_a_whole_spelling() {
    let spelled = Spelled::of("ab origin cd", "origin");
    assert!(spelled.within(3, 9));
    assert!(spelled.within(0, 12));
    assert!(!spelled.within(0, 8));
    assert!(!spelled.within(4, 12));
}

#[test]
fn overlapping_spellings_all_count() {
    let spelled = Spelled::of("aaa", "aa");
    assert!(spelled.within(1, 3));
    assert!(!spelled.within(2, 3));
}

#[test]
fn a_word_after_multibyte_text_is_found_at_its_bytes() {
    let spelled = Spelled::of("ééé origin", "origin");
    assert!(spelled.within(7, 13));
    assert!(Spelled::of("éé", "é").within(2, 4));
}

#[test]
fn every_span_holds_an_empty_word() {
    assert!(Spelled::of("abc", "").within(1, 1));
}
