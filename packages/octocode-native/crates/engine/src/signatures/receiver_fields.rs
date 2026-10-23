//! The field table both receiver lanes (tree-sitter and oxc) build per
//! owner type: a field name to its type, `None` once two writes disagree.
use std::collections::HashMap;

/// Field name → its type; `None` where the type is unknown or conflicting.
pub(crate) type Fields = HashMap<String, Option<String>>;

/// Record `ty` for `name`; a second, different type makes it unknown.
pub(crate) fn merge(fields: &mut Fields, name: &str, ty: Option<String>) {
    match fields.get_mut(name) {
        Some(existing) if *existing != ty => *existing = None,
        Some(_) => {}
        None => {
            fields.insert(name.to_owned(), ty);
        }
    }
}

#[cfg(test)]
mod tests {
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
}
