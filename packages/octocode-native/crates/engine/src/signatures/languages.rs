/// The grammar inventory, built once.
#[must_use]
pub fn grammar_capabilities() -> &'static [crate::types::GrammarCapability] {
    static CAPABILITIES: std::sync::LazyLock<Vec<crate::types::GrammarCapability>> =
        std::sync::LazyLock::new(build_grammar_capabilities);
    &CAPABILITIES
}

fn build_grammar_capabilities() -> Vec<crate::types::GrammarCapability> {
    all_entries()
        .iter()
        .map(|entry| crate::types::GrammarCapability {
            language: entry.name.to_owned(),
            language_id: entry.language_id.map(str::to_owned),
            selector_aliases: entry
                .selector_aliases
                .iter()
                .map(|alias| (*alias).to_owned())
                .collect(),
            extensions: entry
                .extensions
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect(),
            structural_search: true,
            signature_outline: !entry.body_query.is_empty(),
            graph_facts: !entry.body_query.is_empty(),
        })
        .collect()
}

use std::sync::LazyLock;
use tree_sitter::Language;

use crate::text::JS_EXTENSIONS;

#[derive(Clone, Copy)]
pub(crate) struct PatternSyntax {
    pub expando: char,
    pub member_context: bool,
    pub function_context: bool,
    pub terminated_fragment_kind: Option<&'static str>,
}

impl PatternSyntax {
    const DEFAULT: Self = Self {
        expando: 'µ',
        member_context: false,
        function_context: false,
        terminated_fragment_kind: None,
    };
    const DOLLAR: Self = Self {
        expando: '$',
        ..Self::DEFAULT
    };
    const C_FAMILY: Self = Self {
        expando: '\u{10000}',
        function_context: true,
        terminated_fragment_kind: Some("call_expression"),
        ..Self::DEFAULT
    };
}

pub struct LanguageEntry {
    /// Human-readable grammar family name exposed to tools and agent context.
    pub name: &'static str,
    /// Additional family selectors beyond `name` and `language_id`. This is
    /// used only when one user-facing family spans multiple parser entries
    /// (for example TypeScript also selecting the TSX grammar).
    pub selector_aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    /// Extensions another grammar owns that this one also parses when it is
    /// selected (C++ reads C's `.h` headers). A file with such an extension
    /// is parsed by its owner unless this grammar is asked for.
    pub also_parses: &'static [&'static str],
    /// Protocol language id used for LSP document setup and grammar-based syntax
    /// anchoring. This does not imply a built-in semantic server route: Assembly
    /// has an id for trusted custom servers but intentionally no default command.
    /// `lsp::grammar` reads LSP grammars from this field (no second table).
    pub language_id: Option<&'static str>,
    /// Pre-built `Language` handle. `Language` is `Clone + Send + Sync` but
    /// NOT `Copy` in tree-sitter 0.27 — always use `.clone()` at call sites.
    pub language: Language,
    /// Tree-sitter S-expression query whose `@body` captures are the nodes the
    /// signature extractor drops. Every first-class grammar has a body query;
    /// structural, signature, and graph capability inventories therefore agree.
    pub body_query: &'static str,
    pub(crate) pattern_syntax: PatternSyntax,
    pub comment_style: CommentStyle,
}

/// The comment syntax an outline drops pure-comment lines by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentStyle {
    /// `//`, `/* … */` and ` * ` continuation lines.
    C,
    /// `#` lines (a `#!` shebang is kept).
    Hash,
    /// Assembly: `#`, `;`, `//` and `/*` comments.
    Asm,
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
static LANGUAGE_TABLE: LazyLock<Vec<LanguageEntry>> = LazyLock::new(init_language_table);

fn init_language_table() -> Vec<LanguageEntry> {
    // Feature-gated grammars sit inside the literal behind `#[cfg]`, so the
    // table is built in one expression with no mutable binding.
    vec![
        LanguageEntry {
            name: "TypeScript",
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
            name: "TSX",
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
            name: "JavaScript",
            pattern_syntax: PatternSyntax::DOLLAR,
            selector_aliases: &[],
            extensions: JS_EXTENSIONS,
            also_parses: &[],
            language_id: Some("javascript"),
            language: tree_sitter_javascript::LANGUAGE.into(),
            body_query: JS_TS_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        LanguageEntry {
            name: "Python",
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
            name: "Go",
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
            name: "Rust",
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
            name: "Java",
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
            name: "C",
            pattern_syntax: PatternSyntax::C_FAMILY,
            selector_aliases: &[],
            extensions: &["c", "h"],
            also_parses: &[],
            language_id: Some("c"),
            language: tree_sitter_c::LANGUAGE.into(),
            body_query: C_BODY_QUERY,
            comment_style: CommentStyle::C,
        },
        #[cfg(feature = "tree-sitter-asm")]
        LanguageEntry {
            name: "Assembly",
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
            name: "Scala",
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
            name: "C++",
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
            name: "CUDA",
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
            name: "C#",
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

pub fn find_entry(ext: &str) -> Option<&'static LanguageEntry> {
    LANGUAGE_TABLE.iter().find(|e| e.extensions.contains(&ext))
}

/// Whether `ext` is a C-family source (C, C++, CUDA): its grammar's language
/// id is one the C-family server answers.
pub(crate) fn is_c_family_extension(ext: &str) -> bool {
    find_entry(ext).is_some_and(|entry| crate::lsp::family::is_c_family(entry.language_id))
}

/// Whether `entry` is the grammar `selector` names: its name, language id,
/// or one of its extensions (ASCII case-insensitive).
fn names(entry: &LanguageEntry, selector: &str) -> bool {
    entry.name.eq_ignore_ascii_case(selector)
        || entry
            .language_id
            .is_some_and(|id| id.eq_ignore_ascii_case(selector))
        || entry
            .extensions
            .iter()
            .any(|ext| ext.eq_ignore_ascii_case(selector))
}

/// Whether `selector` selects `entry`: [`names`] it, or names its family
/// through a selector alias (`typescript` also selects TSX).
pub fn selects(entry: &LanguageEntry, selector: &str) -> bool {
    names(entry, selector)
        || entry
            .selector_aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(selector))
}

/// Every grammar `selector` selects, in registry order.
pub fn select(selector: &str) -> impl Iterator<Item = &'static LanguageEntry> {
    let selector = selector.trim().to_owned();
    LANGUAGE_TABLE
        .iter()
        .filter(move |entry| selects(entry, &selector))
}

/// The one grammar `selector` names by name, language id or extension;
/// `None` when it names none, or several.
pub(crate) fn select_one(selector: &str) -> Option<&'static LanguageEntry> {
    let selector = selector.trim();
    let mut named = LANGUAGE_TABLE.iter().filter(|entry| names(entry, selector));
    let entry = named.next()?;
    named.next().is_none().then_some(entry)
}

/// The grammars a selector names: a grammar name, language id or
/// extension, with or without its leading dot (`.cc` and `cpp` both name
/// C++), plus the family its selector aliases span.
fn selected(selector: &str) -> impl Iterator<Item = &'static LanguageEntry> {
    let selector = selector.trim();
    select(selector.strip_prefix('.').unwrap_or(selector))
}

/// The extensions (lowercase, no dot) a selector picks in a directory:
/// every selected grammar's own plus the ones it also parses (`h` under
/// C++); a `.ext` selector is that one registered extension. `None` when it
/// selects nothing.
pub fn family_extensions(selector: &str) -> Option<std::collections::BTreeSet<String>> {
    let selector = selector.trim().to_ascii_lowercase();
    if let Some(extension) = selector.strip_prefix('.') {
        return find_entry(extension)
            .map(|_| std::collections::BTreeSet::from([extension.to_owned()]));
    }
    let extensions: std::collections::BTreeSet<String> = selected(&selector)
        .flat_map(|entry| entry.extensions.iter().chain(entry.also_parses))
        .map(|ext| ext.to_ascii_lowercase())
        .collect();
    (!extensions.is_empty()).then_some(extensions)
}

/// Whether a grammar `selector` names, in any spelling (`cpp`, `c++`,
/// `hpp`, `.cc`), owns or also parses `path`'s extension: the file can be
/// read as that grammar.
pub fn parses_path(selector: &str, path: &str) -> bool {
    let extension = crate::text::file_extension::extension_of(path, true, "");
    selected(selector).any(|entry| {
        entry
            .extensions
            .iter()
            .chain(entry.also_parses)
            .any(|ext| ext.eq_ignore_ascii_case(&extension))
    })
}

/// The parser extension for `path` under `selector`: the file's own
/// extension when a selected grammar owns it (`.tsx` under `typescript`
/// needs the TSX grammar), the grammar's own first extension when it only
/// also parses it (`.h` under any C++ selector parses as `cpp`), else the
/// selector itself.
pub fn parser_for_path(selector: &str, path: &str) -> String {
    let extension = crate::text::file_extension::extension_of(path, true, "");
    if extension.is_empty() {
        return selector.to_owned();
    }
    let owns = |exts: &[&str]| exts.iter().any(|ext| ext.eq_ignore_ascii_case(&extension));
    if selected(selector).any(|entry| owns(entry.extensions)) {
        return extension;
    }
    selected(selector)
        .find(|entry| owns(entry.also_parses))
        .and_then(|entry| entry.extensions.first())
        .map_or_else(|| selector.to_owned(), |ext| (*ext).to_owned())
}

/// The selector a caller writes for a grammar: its language id (`rust`,
/// `typescript`), else its lowercased name.
pub fn label(entry: &LanguageEntry) -> String {
    entry
        .language_id
        .map_or_else(|| entry.name.to_ascii_lowercase(), str::to_owned)
}

/// Built JS extension (no dot) → the extensions TypeScript tries for the
/// module it names, in order: the source of the same module, then its
/// declaration file.
pub const JS_SOURCE_SWAPS: &[(&str, &[&str])] = &[
    ("js", &["ts", "tsx", "d.ts"]),
    ("jsx", &["tsx", "d.ts"]),
    ("mjs", &["mts", "d.mts"]),
    ("cjs", &["cts", "d.cts"]),
];

/// [`JS_SOURCE_SWAPS`] of one built extension (empty for any other).
pub fn js_source_swaps(extension: &str) -> &'static [&'static str] {
    JS_SOURCE_SWAPS
        .iter()
        .find(|(built, _)| *built == extension)
        .map_or(&[], |(_, sources)| sources)
}

/// The full registry — the single source of truth for grammar capabilities.
pub fn all_entries() -> &'static [LanguageEntry] {
    &LANGUAGE_TABLE
}

pub(crate) fn supported_extensions() -> Vec<&'static str> {
    LANGUAGE_TABLE
        .iter()
        .flat_map(|e| e.extensions.iter().copied())
        .collect()
}

#[cfg(test)]
#[path = "languages_tests.rs"]
mod tests;
