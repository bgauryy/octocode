use super::*;

#[test]
fn a_second_different_type_makes_the_field_unknown() {
    let mut fields = Fields::new();
    merge(&mut fields, "a", Some("A".into()));
    merge(&mut fields, "a", Some("A".into()));
    assert_eq!(fields["a"].as_deref(), Some("A"));
    merge(&mut fields, "a", Some("B".into()));
    assert_eq!(fields["a"], None);
    merge(&mut fields, "a", Some("A".into()));
    assert_eq!(fields["a"], None, "an unknown field stays unknown");
    merge(&mut fields, "b", None);
    merge(&mut fields, "b", Some("B".into()));
    assert_eq!(fields["b"], None);
}
