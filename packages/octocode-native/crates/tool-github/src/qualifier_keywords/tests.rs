use super::*;

#[test]
fn a_range_value_becomes_the_object_its_field_takes() {
    for (value, want) in [
        (">1000", Some(r#"{"min":1001}"#)),
        (">=1000", Some(r#"{"min":1000}"#)),
        ("<10", Some(r#"{"max":9}"#)),
        ("<=10", Some(r#"{"max":10}"#)),
        ("10..50", Some(r#"{"min":10,"max":50}"#)),
        ("10..*", Some(r#"{"min":10}"#)),
        ("7", Some(r#"{"min":7,"max":7}"#)),
        ("many", None),
        ("<0", None),
    ] {
        assert_eq!(count_range(value).as_deref(), want, "{value}");
    }
}

#[test]
fn only_a_known_qualifier_with_a_value_gets_a_hint() {
    let table = [Qualifier {
        names: &["language", "lang"],
        field: |value| set_to("filters.language", value),
    }];
    let hints = hints(
        &["Language:rust", "parser", "lang:", "url:x", "stars:>1"],
        &table,
    );
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("filters.language"), "{hints:?}");
    assert!(hints[0].contains("`rust`"), "{hints:?}");
}
