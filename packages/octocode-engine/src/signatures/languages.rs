use std::sync::LazyLock;
use tree_sitter::Language;

pub struct LanguageEntry {
    pub extensions: &'static [&'static str],
    /// LSP server language id (e.g. `"typescript"`, `"css"`). `None` for grammars
    /// with no configured language server (e.g. Scala) — those still do structural
    /// search and signatures, but `lsp::grammar::grammar_for_file` skips them. This
    /// is the single source the LSP grammar map derives from (no second table).
    pub language_id: Option<&'static str>,
    /// Pre-built `Language` handle. `Language` is `Clone + Send + Sync` but
    /// NOT `Copy` in tree-sitter 0.27 — always use `.clone()` at call sites.
    pub language: Language,
    /// Tree-sitter S-expression query whose `@body` captures are the nodes the
    /// signature extractor drops. An **empty** string is a sentinel meaning
    /// "this grammar is wired in for *structural search only*" — the signature
    /// path (`extract_by_ext` / `extract_boundary_lines_inner`) skips the
    /// tree-sitter route for it and returns no outline (tree-sitter is the only
    /// signature path; there is no regex/heuristic fallback). Used by
    /// markup/style grammars (HTML/CSS/SCSS) that have no function-body
    /// concept to strip.
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

#[cfg(feature = "tree-sitter-c-sharp")]
const CS_BODY_QUERY: &str = r#"[
  (method_declaration        body: (block) @body)
  (constructor_declaration   body: (block) @body)
  (accessor_declaration      body: (block) @body)
  (local_function_statement  body: (block) @body)
  (lambda_expression         body: (block) @body)
]"#;

const RUBY_BODY_QUERY: &str = r#"[
  (method body: (body_statement) @body)
  (singleton_method body: (body_statement) @body)
]"#;

const PHP_BODY_QUERY: &str = r#"[
  (function_definition body: (compound_statement) @body)
  (method_declaration body: (compound_statement) @body)
]"#;

const KOTLIN_BODY_QUERY: &str = r#"[
  (function_declaration (function_body) @body)
  (anonymous_function (function_body) @body)
]"#;

#[cfg(feature = "tree-sitter-swift")]
const SWIFT_BODY_QUERY: &str = r#"
  (function_declaration body: (function_body) @body)
"#;

/// Scala: strip function/method bodies. Class/object/trait bodies are intentionally NOT
/// dropped so method signatures inside them remain visible (mirrors Java/TS behaviour).
/// `body:` is a named field in both node types.
#[cfg(feature = "tree-sitter-extended")]
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
            // `.mts`/`.cts` are first-class TS (oxc + LSP already treat them so);
            // align signature/structural with that.
            extensions: &["ts", "mts", "cts"],
            language_id: Some("typescript"),
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["tsx"],
            language_id: Some("typescriptreact"),
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["js", "jsx", "mjs", "cjs"],
            language_id: Some("javascript"),
            language: tree_sitter_javascript::LANGUAGE.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            // `.pyi` stubs parse with the Python grammar (LSP already maps them).
            extensions: &["py", "pyi"],
            language_id: Some("python"),
            language: tree_sitter_python::LANGUAGE.into(),
            body_query: PY_BODY_QUERY,
            comment_style: "hash",
        },
        LanguageEntry {
            extensions: &["go"],
            language_id: Some("go"),
            language: tree_sitter_go::LANGUAGE.into(),
            body_query: GO_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["rs"],
            language_id: Some("rust"),
            language: tree_sitter_rust::LANGUAGE.into(),
            body_query: RS_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["java"],
            language_id: Some("java"),
            language: tree_sitter_java::LANGUAGE.into(),
            body_query: JAVA_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["c", "h"],
            language_id: Some("c"),
            language: tree_sitter_c::LANGUAGE.into(),
            body_query: C_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["rb", "rake", "gemspec", "ru"],
            language_id: Some("ruby"),
            language: tree_sitter_ruby::LANGUAGE.into(),
            body_query: RUBY_BODY_QUERY,
            comment_style: "hash",
        },
        LanguageEntry {
            // PHP variables require `$` prefix — expando char must be `$` so
            // patterns like `foo($ARG)` parse as valid PHP. See structural/language.rs.
            extensions: &["php"],
            language_id: Some("php"),
            language: tree_sitter_php::LANGUAGE_PHP.into(),
            body_query: PHP_BODY_QUERY,
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["kt", "kts"],
            language_id: Some("kotlin"),
            language: tree_sitter_kotlin_ng::LANGUAGE.into(),
            body_query: KOTLIN_BODY_QUERY,
            comment_style: "c",
        },
        #[cfg(feature = "tree-sitter-extended")]
        LanguageEntry {
            extensions: &["sql"],
            language_id: Some("sql"),
            language: tree_sitter_sequel::LANGUAGE.into(),
            body_query: "", // data-query language — no function bodies
            comment_style: "c",
        },
        // ── Markup / style grammars: structural-search only ──────────────────
        // These grammars are already linked (the LSP layer uses them). They are
        // registered here so `structural::search` can resolve them, but they
        // carry an EMPTY `body_query` so the signature path returns no outline
        // (markup/styles have no fn body to strip).
        LanguageEntry {
            extensions: &["html", "htm"],
            language_id: Some("html"),
            language: tree_sitter_html::LANGUAGE.into(),
            body_query: "",
            comment_style: "html",
        },
        LanguageEntry {
            extensions: &["css"],
            language_id: Some("css"),
            language: tree_sitter_css::LANGUAGE.into(),
            body_query: "",
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["scss"],
            language_id: Some("scss"),
            language: tree_sitter_scss::language(),
            body_query: "",
            comment_style: "c",
        },
        // Scala: function bodies stripped; class/object/trait bodies kept so
        // member signatures remain visible (mirrors Java/TS behaviour).
        // No LSP server configured — `language_id: None` keeps it absent from
        // the LSP grammar map until a standard scala-ls binary path is established.
        #[cfg(feature = "tree-sitter-extended")]
        LanguageEntry {
            extensions: &["scala", "sc", "sbt"],
            language_id: None,
            language: tree_sitter_scala::LANGUAGE.into(),
            body_query: SCALA_BODY_QUERY,
            comment_style: "c",
        },
        // ── Config grammars: structural-search only ──────────────────────────
        // Linked for the LSP layer; registered here so structural search can run
        // shape queries over package manifests, CI workflows, k8s/compose YAML,
        // etc. Empty body_query + their presence in NO_SYMBOL_EXTS keeps the
        // signature path returning None (data files have no code signatures).
        LanguageEntry {
            extensions: &["json", "jsonc"],
            language_id: Some("json"),
            language: tree_sitter_json::LANGUAGE.into(),
            body_query: "",
            comment_style: "c",
        },
        LanguageEntry {
            extensions: &["yaml", "yml"],
            language_id: Some("yaml"),
            language: tree_sitter_yaml::LANGUAGE.into(),
            body_query: "",
            comment_style: "hash",
        },
    ];

    // Feature-gated grammars: conditional push after vec! creation is fine.
    #[cfg(feature = "tree-sitter-cpp")]
    entries.push(LanguageEntry {
        // Include the `.hh`/`.hxx` header variants the structural expando table
        // already anticipates.
        extensions: &["cpp", "hpp", "cc", "cxx", "hh", "hxx"],
        language_id: Some("cpp"),
        language: tree_sitter_cpp::LANGUAGE.into(),
        body_query: CPP_BODY_QUERY,
        comment_style: "c",
    });

    #[cfg(feature = "tree-sitter-c-sharp")]
    entries.push(LanguageEntry {
        extensions: &["cs"],
        language_id: Some("csharp"),
        language: tree_sitter_c_sharp::LANGUAGE.into(),
        body_query: CS_BODY_QUERY,
        comment_style: "c",
    });

    #[cfg(feature = "tree-sitter-swift")]
    entries.push(LanguageEntry {
        extensions: &["swift"],
        language_id: Some("swift"),
        language: tree_sitter_swift::LANGUAGE.into(),
        body_query: SWIFT_BODY_QUERY,
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

/// Extensions that produce a signature outline: tree-sitter grammars with a
/// non-empty `body_query`. Excludes structural-search-only grammars
/// (HTML/CSS/SCSS/JSON/YAML), which have no function bodies to
/// strip and therefore no outline.
pub fn signature_extensions() -> Vec<&'static str> {
    LANGUAGE_TABLE
        .iter()
        .filter(|e| !e.body_query.is_empty())
        .flat_map(|e| e.extensions.iter().copied())
        .collect()
}

#[cfg(test)]
#[path = "languages_tests.rs"]
mod tests;
