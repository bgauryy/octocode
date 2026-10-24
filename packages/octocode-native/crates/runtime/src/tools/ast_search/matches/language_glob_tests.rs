use octocode_engine::signatures::languages::family_extensions;
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
    use crate::id::ToolId;
    use octocode_contract_types::AstSearchQueryMatch0;
    use serde_json::json;

    let query: AstSearchQueryMatch0 = serde_json::from_value(json!({
        "operation":"match","path":"src","rule":{"kind":"call_expression"},
        "page":3,"matchPage":2,"scanOffset":11,"snapshot":"saved",
        "include":["**/*.ts"],"exclude":["vendor/**"],"includeHidden":true
    }))
    .expect("saved match request");
    let repair = language_repair(
        MatchQuery::Content(&query),
        &["typescript".into(), "javascript".into()],
    );
    crate::contracts::validate("astSearch", repair["input"].clone()).expect("fresh public repair");
    let rows = repair["input"]["queries"].as_array().expect("repair batch");
    assert_eq!(rows.len(), 2);
    for row in rows {
        for field in crate::contracts::state_fields(ToolId::AstSearch) {
            assert!(row.get(field).is_none(), "{field}: {row}");
        }
        assert_eq!(row["rule"], json!({"kind":"call_expression"}));
        assert_eq!(row["include"], json!(["**/*.ts"]));
        assert_eq!(row["exclude"], json!(["vendor/**"]));
        assert_eq!(row["includeHidden"], true);
    }
}
