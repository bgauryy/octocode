use std::sync::LazyLock;
use tree_sitter::Language;

pub struct LanguageEntry {
    /// Human-readable grammar family name exposed to tools and agent context.
    pub name: &'static str,
    /// Additional family selectors beyond `name` and `language_id`. This is
    /// used only when one user-facing family spans multiple parser entries
    /// (for example TypeScript also selecting the TSX grammar).
    pub selector_aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    /// Protocol language id used for LSP document setup and grammar-based syntax
    /// anchoring. This does not imply a built-in semantic server route: Assembly
    /// has an id for trusted custom servers but intentionally no default command.
    /// This is the single source the LSP grammar map derives from (no second table).
    pub language_id: Option<&'static str>,
    /// Pre-built `Language` handle. `Language` is `Clone + Send + Sync` but
    /// NOT `Copy` in tree-sitter 0.27 — always use `.clone()` at call sites.
    pub language: Language,
    /// Tree-sitter S-expression query whose `@body` captures are the nodes the
    /// signature extractor drops. Every first-class grammar has a body query;
    /// structural, signature, and graph capability inventories therefore agree.
    pub body_query: &'static str,
    pub comment_style: &'static str,
}

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

#[cfg(feature = "tree-sitter-cpp")]
const CPP_BODY_QUERY: &str = r#"[
  (function_definition  body: (compound_statement) @body)
  (lambda_expression    body: (compound_statement) @body)
]"#;

#[cfg(feature = "tree-sitter-cuda")]
const CUDA_BODY_QUERY: &str = r#"[
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
static LANGUAGE_TABLE: LazyLock<Vec<LanguageEntry>> = LazyLock::new(init_language_table);

fn init_language_table() -> Vec<LanguageEntry> {
    // Non-feature-gated entries: use vec! to satisfy clippy::vec_init_then_push.
    // `mut` is only exercised by the feature-gated cpp/c# pushes below; without
    // those grammars the binding is never mutated.
    #[allow(unused_mut)]
    let mut entries = vec![
        LanguageEntry {
            name: "TypeScript",
            selector_aliases: &[],
            // `.mts`/`.cts` are first-class TS (oxc + LSP already treat them so);
            // align signature/structural with that.
            extensions: &["ts", "mts", "cts"],
            language_id: Some("typescript"),
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            name: "TSX",
            selector_aliases: &["typescript"],
            extensions: &["tsx"],
            language_id: Some("typescriptreact"),
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            name: "JavaScript",
            selector_aliases: &[],
            extensions: &["js", "jsx", "mjs", "cjs"],
            language_id: Some("javascript"),
            language: tree_sitter_javascript::LANGUAGE.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            name: "Python",
            selector_aliases: &[],
            // `.pyi` stubs parse with the Python grammar (LSP already maps them).
            extensions: &["py", "pyi"],
            language_id: Some("python"),
            language: tree_sitter_python::LANGUAGE.into(),
            body_query: PY_BODY_QUERY,
            comment_style: "hash",
        },
        LanguageEntry {
            name: "Go",
            selector_aliases: &[],
            extensions: &["go"],
            language_id: Some("go"),
            language: tree_sitter_go::LANGUAGE.into(),
            body_query: GO_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            name: "Rust",
            selector_aliases: &[],
            extensions: &["rs"],
            language_id: Some("rust"),
            language: tree_sitter_rust::LANGUAGE.into(),
            body_query: RS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            name: "Java",
            selector_aliases: &[],
            extensions: &["java"],
            language_id: Some("java"),
            language: tree_sitter_java::LANGUAGE.into(),
            body_query: JAVA_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            name: "C",
            selector_aliases: &[],
            extensions: &["c", "h"],
            language_id: Some("c"),
            language: tree_sitter_c::LANGUAGE.into(),
            body_query: C_BODY_QUERY,
            comment_style: "c",
        },
        #[cfg(feature = "tree-sitter-asm")]
        LanguageEntry {
            name: "Assembly",
            selector_aliases: &[],
            extensions: &["asm", "assembly", "s"],
            // No built-in semantic server is claimed. The language id still
            // enables syntax anchoring when a trusted custom server is used.
            language_id: Some("asm"),
            language: tree_sitter_asm::LANGUAGE.into(),
            body_query: ASM_BODY_QUERY,
            comment_style: "asm",
        },
        // Scala: function bodies stripped; class/object/trait bodies kept so
        // member signatures remain visible (mirrors Java/TS behaviour).
        #[cfg(feature = "tree-sitter-scala")]
        LanguageEntry {
            name: "Scala",
            selector_aliases: &[],
            extensions: &["scala", "sc", "sbt"],
            language_id: Some("scala"),
            language: tree_sitter_scala::LANGUAGE.into(),
            body_query: SCALA_BODY_QUERY,
            comment_style: "c",
        },
    ];

    // Feature-gated grammars: conditional push after vec! creation is fine.
    #[cfg(feature = "tree-sitter-cpp")]
    entries.push(LanguageEntry {
        name: "C++",
        selector_aliases: &[],
        // Include the `.hh`/`.hxx` header variants the structural expando table
        // already anticipates.
        extensions: &["cpp", "hpp", "cc", "cxx", "hh", "hxx"],
        language_id: Some("cpp"),
        language: tree_sitter_cpp::LANGUAGE.into(),
        body_query: CPP_BODY_QUERY,
        comment_style: "c",
    });

    #[cfg(feature = "tree-sitter-cuda")]
    entries.push(LanguageEntry {
        name: "CUDA",
        selector_aliases: &[],
        extensions: &["cu", "cuh"],
        language_id: Some("cuda"),
        language: tree_sitter_cuda::LANGUAGE.into(),
        body_query: CUDA_BODY_QUERY,
        comment_style: "c",
    });

    #[cfg(feature = "tree-sitter-c-sharp")]
    entries.push(LanguageEntry {
        name: "C#",
        selector_aliases: &[],
        extensions: &["cs"],
        language_id: Some("csharp"),
        language: tree_sitter_c_sharp::LANGUAGE.into(),
        body_query: CS_BODY_QUERY,
        comment_style: "c",
    });

    entries
}

pub fn find_entry(ext: &str) -> Option<&'static LanguageEntry> {
    LANGUAGE_TABLE.iter().find(|e| e.extensions.contains(&ext))
}

/// The full registry — the single source of truth for grammar capabilities.
/// `lsp::grammar` derives its grammar map from this (entries with a
/// `language_id`) instead of maintaining a parallel table.
pub fn all_entries() -> &'static [LanguageEntry] {
    &LANGUAGE_TABLE
}

pub fn supported_extensions() -> Vec<&'static str> {
    LANGUAGE_TABLE
        .iter()
        .flat_map(|e| e.extensions.iter().copied())
        .collect()
}

/// Extensions that produce a signature outline. Every registry entry is a
/// first-class language with a non-empty body query.
pub fn signature_extensions() -> Vec<&'static str> {
    supported_extensions()
}

#[cfg(test)]
#[path = "languages_tests.rs"]
mod tests;
