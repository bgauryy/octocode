use tree_sitter::Language;

// ── TypeScript / TSX ─────────────────────────────────────────────────────────
fn ts_language() -> Language { tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into() }
fn tsx_language() -> Language { tree_sitter_typescript::LANGUAGE_TSX.into() }

const TS_BODY_QUERY: &str = r#"[
  (function_declaration        body: (statement_block) @body)
  (function_expression         body: (statement_block) @body)
  (arrow_function              body: (statement_block) @body)
  (generator_function_declaration body: (statement_block) @body)
  (generator_function          body: (statement_block) @body)
  (method_definition           body: (statement_block) @body)
]"#;

// ── JavaScript / JSX ─────────────────────────────────────────────────────────
fn js_language() -> Language { tree_sitter_javascript::LANGUAGE.into() }

// Identical query — JS grammar shares the same node types as TS for bodies.
const JS_BODY_QUERY: &str = TS_BODY_QUERY;

// ── Python ────────────────────────────────────────────────────────────────────
fn py_language() -> Language { tree_sitter_python::LANGUAGE.into() }

const PY_BODY_QUERY: &str = r#"[
  (function_definition body: (block) @body)
]"#;

// ── Go ────────────────────────────────────────────────────────────────────────
fn go_language() -> Language { tree_sitter_go::LANGUAGE.into() }

const GO_BODY_QUERY: &str = r#"[
  (function_declaration body: (block) @body)
  (method_declaration   body: (block) @body)
  (func_literal         body: (block) @body)
]"#;

// ── Rust ──────────────────────────────────────────────────────────────────────
fn rs_language() -> Language { tree_sitter_rust::LANGUAGE.into() }

const RS_BODY_QUERY: &str = r#"[
  (function_item    body: (block) @body)
  (closure_expression body: (block) @body)
]"#;

// ── Java ──────────────────────────────────────────────────────────────────────
fn java_language() -> Language { tree_sitter_java::LANGUAGE.into() }

const JAVA_BODY_QUERY: &str = r#"[
  (method_declaration      body: (block) @body)
  (constructor_declaration body: (block) @body)
  (lambda_expression       body: (block) @body)
]"#;

// ── C ─────────────────────────────────────────────────────────────────────────
fn c_language() -> Language { tree_sitter_c::LANGUAGE.into() }

const C_BODY_QUERY: &str = r#"
  (function_definition body: (compound_statement) @body)
"#;

// ── C++ ───────────────────────────────────────────────────────────────────────
fn cpp_language() -> Language { tree_sitter_cpp::LANGUAGE.into() }

const CPP_BODY_QUERY: &str = r#"[
  (function_definition  body: (compound_statement) @body)
  (lambda_expression    body: (compound_statement) @body)
]"#;

// ── C# ────────────────────────────────────────────────────────────────────────
fn cs_language() -> Language { tree_sitter_c_sharp::LANGUAGE.into() }

const CS_BODY_QUERY: &str = r#"[
  (method_declaration        body: (block) @body)
  (constructor_declaration   body: (block) @body)
  (accessor_declaration      body: (block) @body)
  (local_function_statement  body: (block) @body)
  (lambda_expression         body: (block) @body)
]"#;

// ── Bash / Shell ──────────────────────────────────────────────────────────────
fn bash_language() -> Language { tree_sitter_bash::LANGUAGE.into() }

const BASH_BODY_QUERY: &str = r#"
  (function_definition body: (compound_statement) @body)
"#;

// ── Config table ──────────────────────────────────────────────────────────────

pub struct LanguageEntry {
    pub extensions:    &'static [&'static str],
    pub language_fn:   fn() -> Language,
    pub body_query:    &'static str,
    pub comment_style: &'static str,
}

pub static LANGUAGE_TABLE: &[LanguageEntry] = &[
    LanguageEntry {
        extensions:    &["ts"],
        language_fn:   ts_language,
        body_query:    TS_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["tsx"],
        language_fn:   tsx_language,
        body_query:    TS_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["js", "jsx", "mjs", "cjs"],
        language_fn:   js_language,
        body_query:    JS_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["py"],
        language_fn:   py_language,
        body_query:    PY_BODY_QUERY,
        comment_style: "hash",
    },
    LanguageEntry {
        extensions:    &["go"],
        language_fn:   go_language,
        body_query:    GO_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["rs"],
        language_fn:   rs_language,
        body_query:    RS_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["java"],
        language_fn:   java_language,
        body_query:    JAVA_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["c", "h"],
        language_fn:   c_language,
        body_query:    C_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["cpp", "hpp", "cc", "cxx"],
        language_fn:   cpp_language,
        body_query:    CPP_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["cs"],
        language_fn:   cs_language,
        body_query:    CS_BODY_QUERY,
        comment_style: "c",
    },
    LanguageEntry {
        extensions:    &["sh", "bash", "zsh"],
        language_fn:   bash_language,
        body_query:    BASH_BODY_QUERY,
        comment_style: "hash",
    },
];

pub fn find_entry(ext: &str) -> Option<&'static LanguageEntry> {
    LANGUAGE_TABLE.iter().find(|e| e.extensions.contains(&ext))
}

pub fn supported_extensions() -> Vec<&'static str> {
    LANGUAGE_TABLE.iter().flat_map(|e| e.extensions.iter().copied()).collect()
}
