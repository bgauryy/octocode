use super::*;
#[test]
fn typed_values_cannot_open_a_second_scope() {
    let mut terms = SearchTerms::new(["parser repo:other/project"]);
    terms.scope(Some("owner"), Some("repo"));
    terms.text("label", Some("bug label:critical"));
    assert_eq!(
        terms.finish(),
        "\"parser repo:other/project\" repo:owner/repo label:\"bug label:critical\""
    );
}
