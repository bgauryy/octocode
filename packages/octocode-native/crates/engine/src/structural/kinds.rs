//! Grammar node-kind ids for the structural matchers.

use tree_sitter::Language;

/// Symbol id of a user-supplied **named** node kind, or `None` when no parsed
/// node can carry it.
///
/// `Language::id_for_node_kind` is a single table lookup, but it maps
/// supertypes (e.g. `expression`) to ids no concrete node carries and any
/// prefix of `ERROR` (including `""`) to the ERROR symbol. Both are rejected
/// here so a rule naming them fails loudly instead of silently matching
/// nothing or the wrong nodes. `ERROR` itself (tree-sitter's recovery node,
/// outside the grammar's symbol table) is accepted.
pub(super) fn named_kind_id(language: &Language, kind: &str) -> Option<u16> {
    if kind == "ERROR" {
        return Some(u16::MAX);
    }
    let id = language.id_for_node_kind(kind, true);
    if id == 0 || id == u16::MAX || language.node_kind_is_supertype(id) {
        None
    } else {
        Some(id)
    }
}

/// `names` that share the most `_`-separated words with `name`: what a caller
/// who spelled `name` likely meant in this grammar.
fn nearest<'a>(name: &str, names: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let words = name.split('_').collect::<Vec<_>>();
    let shared = |other: &str| other.split('_').filter(|word| words.contains(word)).count();
    let mut scored = names
        .map(|other| (shared(other), other))
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    let best = scored.iter().map(|(score, _)| *score).max().unwrap_or(0);
    scored.retain(|(score, _)| *score == best);
    let mut nearest = scored
        .into_iter()
        .map(|(_, other)| other)
        .collect::<Vec<_>>();
    nearest.sort_unstable();
    nearest.dedup();
    nearest
}

/// The named kinds of `language` nearest `kind`, as a sentence to append to an
/// error; empty when none shares a word with it.
pub(crate) fn near_kinds(language: &Language, kind: &str) -> String {
    let names = (0..language.node_kind_count())
        .filter_map(|id| u16::try_from(id).ok())
        .filter(|id| language.node_kind_is_named(*id) && language.node_kind_is_visible(*id))
        .filter_map(|id| language.node_kind_for_id(id));
    named_near("kinds", &nearest(kind, names))
}

/// The fields of `language` nearest `field`, as [`near_kinds`] says them.
pub(crate) fn near_fields(language: &Language, field: &str) -> String {
    let names = (1..=language.field_count())
        .filter_map(|id| u16::try_from(id).ok())
        .filter_map(|id| language.field_name_for_id(id));
    named_near("fields", &nearest(field, names))
}

fn named_near(what: &str, names: &[&str]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("; nearest {what}: {}", names.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::named_kind_id;

    #[test]
    fn rejects_supertypes_error_prefixes_and_unknown_kinds() {
        let ts: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        assert!(named_kind_id(&ts, "call_expression").is_some());
        assert_eq!(named_kind_id(&ts, "ERROR"), Some(u16::MAX));
        for rejected in ["", "E", "ERR", "expression", "statement", "not_a_kind", "("] {
            assert_eq!(named_kind_id(&ts, rejected), None, "{rejected:?}");
        }
    }
}
