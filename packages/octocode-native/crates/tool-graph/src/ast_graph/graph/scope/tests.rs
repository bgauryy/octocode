use super::*;

#[test]
fn suggestions_descend_through_a_directory_holding_nearly_every_file() {
    let files = [
        "packages/native/a.rs",
        "packages/native/b.rs",
        "packages/native/c.rs",
        "packages/web/a.ts",
        "packages/web/b.ts",
        "packages/cli/a.ts",
        "packages/x.ts",
        "README.md",
    ];
    assert_eq!(
        narrower_scopes(files),
        [
            ("packages/native".to_owned(), 3),
            ("packages/web".to_owned(), 2),
            ("packages/cli".to_owned(), 1),
        ]
    );
}

#[test]
fn suggestions_stop_at_the_level_where_files_divide() {
    let files = ["a/1.ts", "a/2.ts", "b/1.ts", "b/2.ts", "c/1.ts", "top.ts"];
    assert_eq!(
        narrower_scopes(files),
        [
            ("a".to_owned(), 2),
            ("b".to_owned(), 2),
            ("c".to_owned(), 1)
        ]
    );
}

#[test]
fn a_flat_tree_has_no_narrower_directory() {
    assert!(narrower_scopes(["a.ts", "b.ts"]).is_empty());
    let hints = narrower_scope_hints(Path::new("/w"), &BuildScope::default(), ["a.ts"]);
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("exclude "), "{hints:?}");
}

#[test]
fn narrower_scope_hints_join_the_hints_an_error_already_carries() {
    let files = ["a/1.ts", "a/2.ts", "b/1.ts"];
    let mut row = serde_json::json!({
        "errorCode": octocode_contract_types::ErrorCode::ScopeTooBroad,
        "hints": ["earlier hint"],
    });
    let scope = BuildScope::default();
    hint_narrower_scopes(&mut row, Path::new("/w"), &scope, files);
    hint_narrower_scopes(&mut row, Path::new("/w"), &scope, files);
    let hints = row["hints"].as_array().expect("hints");
    let texts = hints.iter().filter_map(|h| h.as_str()).collect::<Vec<_>>();
    assert_eq!(texts[0], "earlier hint", "{texts:?}");
    for hint in narrower_scope_hints(Path::new("/w"), &scope, files) {
        assert_eq!(
            texts.iter().filter(|text| **text == hint).count(),
            1,
            "{hint}: {texts:?}"
        );
    }
}
