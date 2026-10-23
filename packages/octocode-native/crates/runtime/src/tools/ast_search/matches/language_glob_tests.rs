use crate::tools::ast_rule::language_extensions;
fn language_include_globs(language: &str) -> Option<Vec<String>> {
    language_extensions(language).map(|extensions| {
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
