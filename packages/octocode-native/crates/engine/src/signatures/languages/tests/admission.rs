use super::*;
use crate::text::analysis::{AnalysisOperation as Op, SourceError};

fn provider(capabilities: crate::types::GrammarFactCapabilities) -> FactProvider {
    FactProvider {
        id: "fixture-provider",
        version: "1",
        name: "Fixture provider",
        extensions: &["fixture"],
        capabilities,
        extract: |_, _, _| Ok(None),
    }
}

#[test]
fn provider_admission_and_catalog_are_the_same_independent_operation_inventory() {
    let providers = [provider(crate::types::GrammarFactCapabilities {
        declarations: false,
        calls: false,
        imports: true,
        exports: false,
        heritage: false,
    })];
    let registry = RegistryView::new(&[], &providers).unwrap();
    let entry = registry.find("fixture").unwrap();
    assert!(entry.admits(Op::GraphFacts));
    for operation in [
        Op::Declarations,
        Op::SyntaxTree,
        Op::StructuralSearch,
        Op::SignatureOutline,
    ] {
        assert!(!entry.admits(operation));
        assert!(registry.extensions(operation).is_empty());
    }
    assert_eq!(registry.extensions(Op::GraphFacts), vec!["fixture"]);
    let capability = registry.capabilities().remove(0);
    assert!(capability.graph_facts && capability.facts.imports);
    assert!(
        !capability.facts.declarations
            && !capability.signature_outline
            && !capability.syntax_tree
            && !capability.structural_search
    );
    assert!(capability.region_adapters.is_empty());
    assert!(!capability.argument_references.supported);
    assert_eq!(
        registry.parser_for_path("Fixture provider", "a.fixture"),
        Some("fixture")
    );
    assert_eq!(
        registry.select_one("fixture").unwrap().name(),
        "Fixture provider"
    );
}

#[test]
fn provider_admission_rejects_ambiguous_extensions_and_empty_capability() {
    let capabilities = crate::types::GrammarFactCapabilities {
        declarations: true,
        calls: false,
        imports: false,
        exports: false,
        heritage: false,
    };
    let providers = [provider(capabilities), provider(capabilities)];
    assert!(matches!(
        RegistryView::new(&[], &providers),
        Err(SourceError::InvalidMap("ambiguous registry extension"))
    ));
    let providers = [provider(crate::types::GrammarFactCapabilities {
        declarations: false,
        ..capabilities
    })];
    assert!(matches!(
        RegistryView::new(&[], &providers),
        Err(SourceError::InvalidMap("invalid facts provider"))
    ));
}
