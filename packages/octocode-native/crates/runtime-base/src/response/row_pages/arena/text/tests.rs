use super::*;
use octocode_test_support::json_fixture;
use std::collections::BTreeMap;

#[test]
fn string_paths_keep_unicode_escaped_segments_and_restore_the_parent() {
    let row = json_fixture(
        r#"{
            "é/🦀": {"~field": ["é", {"child": "🦀"}]},
            "z": "tail",
            "next": {"tool": "localFetch", "input": {"path": "indivisible"}}
        }"#,
    );
    let mut path = Vec::new();
    let mut found = Vec::new();
    strings(&row, &mut path, &mut found);
    assert!(path.is_empty());
    assert_eq!(
        found.into_iter().collect::<BTreeMap<_, _>>(),
        BTreeMap::from([
            ("/é~1🦀/~0field/0".to_owned(), 1),
            ("/é~1🦀/~0field/1/child".to_owned(), 2),
            ("/z".to_owned(), 4),
        ])
    );
    let mut found = Vec::new();
    strings(&json!("🦀"), &mut path, &mut found);
    assert_eq!(found, [(String::new(), 2)]);
}
