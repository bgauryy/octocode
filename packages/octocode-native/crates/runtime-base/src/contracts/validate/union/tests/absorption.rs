//! A requirement names each form once and no form that only adds a change to
//! another form it names.
use super::super::selectors::merge_alternatives;
use serde_json::{Value, json};

#[test]
fn a_form_that_adds_a_change_to_another_is_not_named() {
    let (search, crates, maven) = (json!("search"), json!("crates"), json!("maven"));
    let alternatives = vec![
        vec![("operation", vec![&search])],
        vec![
            ("ecosystem", vec![&crates, &maven]),
            ("operation", vec![&search]),
        ],
    ];
    assert_eq!(
        merge_alternatives(alternatives).as_deref(),
        Some(r#"operation:"search""#)
    );
}

#[test]
fn forms_that_do_not_absorb_each_other_are_both_named() {
    let (search, resolve, crates) = (json!("search"), json!("resolve"), json!("crates"));
    // Neither form's selectors are a part of the other's: `operation` asks a
    // different literal in each.
    let alternatives: Vec<Vec<(&str, Vec<&Value>)>> = vec![
        vec![("operation", vec![&search])],
        vec![("ecosystem", vec![&crates]), ("operation", vec![&resolve])],
    ];
    assert_eq!(
        merge_alternatives(alternatives).as_deref(),
        Some(r#"operation:"search" or ecosystem:"crates" and operation:"resolve""#)
    );
    // A form whose selector allows fewer literals than the larger form needs
    // does not absorb it: a row meeting the larger form may miss it.
    let (a, b) = (json!("a"), json!("b"));
    let alternatives: Vec<Vec<(&str, Vec<&Value>)>> = vec![
        vec![("operation", vec![&a])],
        vec![("operation", vec![&a, &b]), ("view", vec![&search])],
    ];
    assert_eq!(
        merge_alternatives(alternatives).as_deref(),
        Some(r#"operation:"a" or operation one of "a", "b" and view:"search""#)
    );
}
