use super::*;
use octocode_engine::signatures::languages::{FactProvider, RegistryView};
use octocode_engine::text::analysis::AnalysisOperation;
#[test]
fn a_known_provider_without_the_requested_operation_is_distinct_from_an_unknown_language() {
    let providers = [FactProvider {
        id: "heldout-declarations",
        version: "1",
        name: "declfixture",
        extensions: &["declfixture"],
        capabilities: octocode_engine::types::GrammarFactCapabilities {
            declarations: true,
            calls: false,
            imports: false,
            exports: false,
            heritage: false,
        },
        extract: |_, _, _| Ok(None),
    }];
    let registry = RegistryView::new(&[], &providers).expect("registry");
    let path = std::path::Path::new("example.declfixture");
    for selector in [None, Some("declfixture")] {
        assert!(
            validate_registered_language(path, selector, AnalysisOperation::Declarations, registry)
                .is_ok()
        );
        for operation in [
            AnalysisOperation::SyntaxTree,
            AnalysisOperation::StructuralSearch,
            AnalysisOperation::SignatureOutline,
        ] {
            assert_eq!(
                validate_registered_language(path, selector, operation, registry)
                    .expect_err("unsupported operation")
                    .code,
                ErrorCode::CapabilityUnavailable
            );
        }
    }
    assert_eq!(
        validate_registered_language(
            path,
            Some("missing"),
            AnalysisOperation::Declarations,
            registry
        )
        .expect_err("unknown provider")
        .code,
        ErrorCode::LanguageUnsupported
    );
}
