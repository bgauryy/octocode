use std::sync::LazyLock;
use tree_sitter::Language;

pub struct LanguageEntry {
    pub extensions: &'static [&'static str],
    /// Pre-built `Language` handle. `Language` is `Clone + Send + Sync` but
    /// NOT `Copy` in tree-sitter 0.26 — always use `.clone()` at call sites.
    pub language: Language,
    pub body_query: &'static str,
    pub comment_style: &'static str,
}

const TS_BODY_QUERY: &str = r#"[
  (function_declaration        body: (statement_block) @body)
  (function_expression         body: (statement_block) @body)
  (arrow_function              body: (statement_block) @body)
  (generator_function_declaration body: (statement_block) @body)
  (generator_function          body: (statement_block) @body)
  (method_definition           body: (statement_block) @body)
]"#;

const JS_BODY_QUERY: &str = TS_BODY_QUERY;

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
  (constructor_declaration body: (block) @body)
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

#[cfg(feature = "tree-sitter-c-sharp")]
const CS_BODY_QUERY: &str = r#"[
  (method_declaration        body: (block) @body)
  (constructor_declaration   body: (block) @body)
  (accessor_declaration      body: (block) @body)
  (local_function_statement  body: (block) @body)
  (lambda_expression         body: (block) @body)
]"#;

const BASH_BODY_QUERY: &str = r#"
  (function_definition body: (compound_statement) @body)
"#;

/// Language objects are pre-built once at first use and reused on every
/// subsequent `find_entry` call. `Language` is `Clone + Send + Sync`, so
/// storing it in a `LazyLock<Vec>` is safe and avoids repeated FFI calls
/// per signature extraction.
static LANGUAGE_TABLE: LazyLock<Vec<LanguageEntry>> = LazyLock::new(init_language_table);

fn init_language_table() -> Vec<LanguageEntry> {
    // Non-feature-gated entries: use vec! to satisfy clippy::vec_init_then_push.
    let mut entries = vec![
        LanguageEntry {
            extensions: &["ts"],
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            body_query: TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["tsx"],
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            body_query: TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["js", "jsx", "mjs", "cjs"],
            language: tree_sitter_javascript::LANGUAGE.into(),
            body_query: JS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["py"],
            language: tree_sitter_python::LANGUAGE.into(),
            body_query: PY_BODY_QUERY,
            comment_style: "hash",
        },
        LanguageEntry {
            extensions: &["go"],
            language: tree_sitter_go::LANGUAGE.into(),
            body_query: GO_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["rs"],
            language: tree_sitter_rust::LANGUAGE.into(),
            body_query: RS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["java"],
            language: tree_sitter_java::LANGUAGE.into(),
            body_query: JAVA_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["c", "h"],
            language: tree_sitter_c::LANGUAGE.into(),
            body_query: C_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["sh", "bash", "zsh"],
            language: tree_sitter_bash::LANGUAGE.into(),
            body_query: BASH_BODY_QUERY,
            comment_style: "hash",
        },
    ];

    // Feature-gated grammars: conditional push after vec! creation is fine.
    #[cfg(feature = "tree-sitter-cpp")]
    entries.push(LanguageEntry {
        extensions: &["cpp", "hpp", "cc", "cxx"],
        language: tree_sitter_cpp::LANGUAGE.into(),
        body_query: CPP_BODY_QUERY,
        comment_style: "c",
    });

    #[cfg(feature = "tree-sitter-c-sharp")]
    entries.push(LanguageEntry {
        extensions: &["cs"],
        language: tree_sitter_c_sharp::LANGUAGE.into(),
        body_query: CS_BODY_QUERY,
        comment_style: "c",
    });

    entries
}

pub fn find_entry(ext: &str) -> Option<&'static LanguageEntry> {
    LANGUAGE_TABLE.iter().find(|e| e.extensions.contains(&ext))
}

pub fn supported_extensions() -> Vec<&'static str> {
    LANGUAGE_TABLE
        .iter()
        .flat_map(|e| e.extensions.iter().copied())
        .collect()
}
