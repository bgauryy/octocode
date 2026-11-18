use octocode_engine::signatures::languages::family_extensions;
use octocode_test_support::json_fixture;
fn language_include_globs(language: &str) -> Option<Vec<String>> {
    family_extensions(language).map(|extensions| {
        extensions
            .into_iter()
            .map(|extension| format!("*.{extension}"))
            .collect()
    })
}

#[test]
fn directory_language_globs_derive_from_the_canonical_grammar_registry() {
    let cuda_enabled = octocode_engine::signatures::languages::grammar_capabilities()
        .iter()
        .any(|capability| {
            capability.structural_search && capability.extensions.iter().any(|ext| ext == "cu")
        });
    assert_eq!(
        language_include_globs("cuda"),
        cuda_enabled.then(|| vec!["*.cu".to_owned(), "*.cuh".to_owned()])
    );
    assert_eq!(
        language_include_globs("assembly"),
        Some(vec![
            "*.asm".to_owned(),
            "*.assembly".to_owned(),
            "*.s".to_owned()
        ])
    );
    assert_eq!(
        language_include_globs("typescript"),
        Some(vec![
            "*.cts".to_owned(),
            "*.mts".to_owned(),
            "*.ts".to_owned(),
            "*.tsx".to_owned()
        ])
    );
    assert_eq!(language_include_globs("unsupported"), None);
}

#[test]
fn parser_repair_drops_private_progress_but_preserves_match_scope() {
    use super::{MatchQuery, gaps::language_repair};
    use octocode_contract_types::AstSearchQueryMatch0;
    use octocode_runtime_base::id::ToolId;

    let query: AstSearchQueryMatch0 = serde_json::from_value(json_fixture(
        r#"{
            "operation": "match",
            "path": "src",
            "rule": {"kind": "call_expression"},
            "page": 3,
            "scanOffset": 11,
            "snapshot": "saved",
            "include": ["**/*.ts"],
            "exclude": ["vendor/**"],
            "includeHidden": true
        }"#,
    ))
    .expect("saved match request");
    let repair = language_repair(
        MatchQuery::Content(&query),
        &["typescript".into(), "javascript".into()],
    );
    octocode_runtime_base::contracts::validate("astSearch", repair["input"].clone())
        .expect("fresh public repair");
    let rows = repair["input"]["queries"].as_array().expect("repair batch");
    assert_eq!(rows.len(), 2);
    for row in rows {
        for field in octocode_runtime_base::contracts::state_fields(ToolId::AstSearch) {
            assert!(row.get(field).is_none(), "{field}: {row}");
        }
        assert_eq!(row["rule"], json_fixture(r#"{"kind": "call_expression"}"#));
        assert_eq!(row["include"], json_fixture(r#"["**/*.ts"]"#));
        assert_eq!(row["exclude"], json_fixture(r#"["vendor/**"]"#));
        assert_eq!(row["includeHidden"], true);
    }
}
