//! The grammar registry: every linked tree-sitter grammar with its
//! extensions, comment style, body-elision query and capabilities.

use std::sync::LazyLock;

use crate::text::JS_EXTENSIONS;
use crate::types::GrammarFactCapabilities;

use super::{CommentStyle, LanguageEntry, PatternSyntax};
use crate::signatures::{calls::CallAdapter, visibility::VisibilityAdapter};

const MODULE_FACTS: GrammarFactCapabilities = GrammarFactCapabilities {
    declarations: true,
    calls: true,
    imports: true,
    exports: true,
    heritage: false,
};
const HERITAGE_FACTS: GrammarFactCapabilities = GrammarFactCapabilities {
    heritage: true,
    ..MODULE_FACTS
};
#[cfg(feature = "tree-sitter-asm")]
const LABEL_FACTS: GrammarFactCapabilities = GrammarFactCapabilities {
    declarations: true,
    calls: false,
    imports: false,
    exports: false,
    heritage: false,
};

const JS_TS_BODY_QUERY: &str = r#"[
  (function_declaration        body: (statement_block) @body)
  (function_expression         body: (statement_block) @body)
  (arrow_function              body: (statement_block) @body)
  (generator_function_declaration body: (statement_block) @body)
  (generator_function          body: (statement_block) @body)
  (method_definition           body: (statement_block) @body)
]"#;

const PY_BODY_QUERY: &str = r#"[
  (function_definition body: (block) @body)
]"#;

const GO_BODY_QUERY: &str = r#"[
  (function_declaration body: (block) @body)
  (method_declaration   body: (block) @body)
  (func_literal         body: (block) @body)
]"#;

const RS_BODY_QUERY: &str = r#"[
  (function_item    body: (block) @body)
  (closure_expression body: (block) @body)
]"#;

const JAVA_BODY_QUERY: &str = r#"[
  (method_declaration      body: (block) @body)
  (constructor_declaration body: (constructor_body) @body)
  (lambda_expression       body: (block) @body)
]"#;

const C_BODY_QUERY: &str = r#"
  (function_definition body: (compound_statement) @body)
"#;

/// C++ and CUDA (whose grammar extends C++'s): function and lambda bodies.
#[cfg(any(feature = "tree-sitter-cpp", feature = "tree-sitter-cuda"))]
const CPP_BODY_QUERY: &str = r#"[
  (function_definition  body: (compound_statement) @body)
  (lambda_expression    body: (compound_statement) @body)
]"#;

// Assembly outlines retain labels, constants, and directives while eliding the
// instruction stream. This gives callers stable navigation anchors without
// pretending that a generic multi-dialect grammar has high-level functions.
#[cfg(feature = "tree-sitter-asm")]
const ASM_BODY_QUERY: &str = r#"
  (instruction) @body
"#;

#[cfg(feature = "tree-sitter-c-sharp")]
const CS_BODY_QUERY: &str = r#"[
  (method_declaration        body: (block) @body)
  (constructor_declaration   body: (block) @body)
  (accessor_declaration      body: (block) @body)
  (local_function_statement  body: (block) @body)
  (lambda_expression         body: (block) @body)
]"#;

/// Scala: strip function/method bodies. Class/object/trait bodies are intentionally NOT
/// dropped so method signatures inside them remain visible (mirrors Java/TS behaviour).
/// `body:` is a named field in both node types.
#[cfg(feature = "tree-sitter-scala")]
const SCALA_BODY_QUERY: &str = r#"[
  (function_definition body: (block) @body)
]"#;

/// Language objects are pre-built once at first use and reused on every
/// subsequent `find_entry` call. `Language` is `Clone + Send + Sync`, so
/// storing it in a `LazyLock<Vec>` is safe and avoids repeated FFI calls
/// per signature extraction.
pub(super) static LANGUAGE_TABLE: LazyLock<Vec<LanguageEntry>> = LazyLock::new(init_language_table);

mod features;
use features::feature_languages;

fn init_language_table() -> Vec<LanguageEntry> {
    built_in_languages()
        .into_iter()
        .chain(feature_languages())
        .collect()
}

/// The grammars every build carries.
fn built_in_languages() -> Vec<LanguageEntry> {
    javascript_languages()
        .into_iter()
        .chain(other_languages())
        .collect()
}

fn javascript_languages() -> Vec<LanguageEntry> {
    vec![
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "TypeScript",
            call_adapter: Some(CallAdapter::Ordinary),
            visibility_adapter: VisibilityAdapter::TopLevel,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::JavaScript),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax::DOLLAR,
            selector_aliases: &[],
            // `.mts`/`.cts` are first-class TS (oxc + LSP already treat them so);
            // align signature/structural with that.
            extensions: &["ts", "mts", "cts"],
            also_parses: &[],
            language_id: Some("typescript"),
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "TSX",
            call_adapter: Some(CallAdapter::Ordinary),
            visibility_adapter: VisibilityAdapter::TopLevel,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::JavaScript),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax::DOLLAR,
            selector_aliases: &["typescript"],
            extensions: &["tsx"],
            also_parses: &[],
            language_id: Some("typescriptreact"),
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "JavaScript",
            call_adapter: Some(CallAdapter::Ordinary),
            visibility_adapter: VisibilityAdapter::TopLevel,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::JavaScript),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax::DOLLAR,
            selector_aliases: &[],
            extensions: JS_EXTENSIONS,
            also_parses: &[],
            language_id: Some("javascript"),
            language: tree_sitter_javascript::LANGUAGE.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
    ]
}

fn other_languages() -> Vec<LanguageEntry> {
    vec![
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "Python",
            call_adapter: Some(CallAdapter::Python),
            visibility_adapter: VisibilityAdapter::TopLevel,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::Python),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax::DEFAULT,
            selector_aliases: &[],
            // `.pyi` stubs parse with the Python grammar (LSP already maps them).
            extensions: &["py", "pyi"],
            also_parses: &[],
            language_id: Some("python"),
            language: tree_sitter_python::LANGUAGE.into(),
            body_query: PY_BODY_QUERY,
            comment_style: CommentStyle::Hash,
        },
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "Go",
            call_adapter: Some(CallAdapter::Ordinary),
            visibility_adapter: VisibilityAdapter::Go,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::Go),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax {
                terminated_fragment_kind: Some("call_expression"),
                ..PatternSyntax::DEFAULT
            },
            selector_aliases: &[],
            extensions: &["go"],
            also_parses: &[],
            language_id: Some("go"),
            language: tree_sitter_go::LANGUAGE.into(),
            body_query: GO_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "Rust",
            call_adapter: Some(CallAdapter::Ordinary),
            visibility_adapter: VisibilityAdapter::Rust,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::Rust),
            test_frameworks: &["rust"],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax {
                terminated_fragment_kind: Some("call_expression"),
                ..PatternSyntax::DEFAULT
            },
            selector_aliases: &[],
            extensions: &["rs"],
            also_parses: &[],
            language_id: Some("rust"),
            language: tree_sitter_rust::LANGUAGE.into(),
            body_query: RS_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "Java",
            call_adapter: Some(CallAdapter::Java),
            visibility_adapter: VisibilityAdapter::Java,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::Java),
            test_frameworks: &[],
            fact_capabilities: HERITAGE_FACTS,
            pattern_syntax: PatternSyntax {
                terminated_fragment_kind: Some("method_invocation"),
                ..PatternSyntax::DOLLAR
            },
            selector_aliases: &[],
            extensions: &["java"],
            also_parses: &[],
            language_id: Some("java"),
            language: tree_sitter_java::LANGUAGE.into(),
            body_query: JAVA_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        LanguageEntry {
            structural_operation: true,
            syntax_tree_operation: true,
            outline_operation: true,
            name: "C",
            call_adapter: Some(CallAdapter::Ordinary),
            visibility_adapter: VisibilityAdapter::CFamily,
            export_forwarding_node: None,
            argument_adapter: Some(crate::signatures::arguments::ArgumentAdapter::C),
            test_frameworks: &[],
            fact_capabilities: MODULE_FACTS,
            pattern_syntax: PatternSyntax::C_FAMILY,
            selector_aliases: &[],
            extensions: &["c", "h"],
            also_parses: &[],
            language_id: Some("c"),
            language: tree_sitter_c::LANGUAGE.into(),
            body_query: C_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
    ]
}
