use super::*;

/// The grammars behind cargo features, each in the literal behind its
/// `#[cfg]`, so the list is one expression with no mutable binding.
pub(super) fn feature_languages() -> Vec<LanguageEntry> {
    vec![
        #[cfg(feature = "tree-sitter-asm")]
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "Assembly",
            call_adapter: None,
            visibility_adapter: VisibilityAdapter::None,
            export_forwarding_node: None,
            argument_adapter: None,
            test_frameworks: &[],
            fact_capabilities: LABEL_FACTS,
            pattern_syntax: PatternSyntax {
                expando: 'Q',
                ..PatternSyntax::DEFAULT
            },
            selector_aliases: &[],
            extensions: &["asm", "assembly", "s"],
            also_parses: &[],
            // No built-in semantic server is claimed. The language id still
            // enables syntax anchoring when a trusted custom server is used.
            language_id: Some("asm"),
            language: tree_sitter_asm::LANGUAGE.into(),
            body_query: ASM_BODY_QUERY,
            comment_style: CommentStyle::Asm,
        },
        // Scala: function bodies stripped; class/object/trait bodies kept so
        // member signatures remain visible (mirrors Java/TS behaviour).
        #[cfg(feature = "tree-sitter-scala")]
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "Scala",
            call_adapter: Some(CallAdapter::Scala),
            visibility_adapter: VisibilityAdapter::Scala,
            export_forwarding_node: Some("export_declaration"),
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::Scala),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax::DEFAULT,
            selector_aliases: &[],
            extensions: &["scala", "sc", "sbt"],
            also_parses: &[],
            language_id: Some("scala"),
            language: tree_sitter_scala::LANGUAGE.into(),
            body_query: SCALA_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        #[cfg(feature = "tree-sitter-cpp")]
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "C++",
            call_adapter: Some(CallAdapter::CFamily),
            visibility_adapter: VisibilityAdapter::CFamily,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::CFamily),
            test_frameworks: &["googletest"],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax::C_FAMILY,
            selector_aliases: &[],
            // Include the `.hh`/`.hxx` header variants the structural expando table
            // already anticipates.
            extensions: &["cpp", "hpp", "cc", "cxx", "hh", "hxx"],
            also_parses: &["h"],
            language_id: Some("cpp"),
            language: tree_sitter_cpp::LANGUAGE.into(),
            body_query: CPP_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        #[cfg(feature = "tree-sitter-cuda")]
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "CUDA",
            call_adapter: Some(CallAdapter::CFamily),
            visibility_adapter: VisibilityAdapter::CFamily,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::CFamily),
            test_frameworks: &["googletest"],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax::C_FAMILY,
            selector_aliases: &[],
            extensions: &["cu", "cuh"],
            also_parses: &[],
            language_id: Some("cuda"),
            language: tree_sitter_cuda::LANGUAGE.into(),
            body_query: CPP_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        #[cfg(feature = "tree-sitter-c-sharp")]
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "C#",
            call_adapter: Some(CallAdapter::CSharp),
            visibility_adapter: VisibilityAdapter::CSharp,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::CSharp),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax {
                member_context: true,
                ..PatternSyntax::DEFAULT
            },
            selector_aliases: &[],
            extensions: &["cs"],
            also_parses: &[],
            language_id: Some("csharp"),
            language: tree_sitter_c_sharp::LANGUAGE.into(),
            body_query: CS_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
    ]
}
