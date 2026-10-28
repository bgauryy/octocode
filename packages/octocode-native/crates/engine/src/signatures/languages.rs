pub fn analysis_capabilities() -> &'static crate::types::AnalysisCapabilities {
    static CAPABILITIES: std::sync::LazyLock<crate::types::AnalysisCapabilities> =
        std::sync::LazyLock::new(|| {
            use crate::text::analysis::Encoding;
            use crate::types::{
                AnalysisCapabilities, SourceDecodingCapability, SourceEncodingCapability,
            };
            AnalysisCapabilities {
                grammar_capabilities: build_grammar_capabilities(),
                source_decoding: SourceDecodingCapability {
                    encodings: vec![
                        SourceEncodingCapability {
                            encoding: Encoding::Utf8,
                            bom: "optional",
                        },
                        SourceEncodingCapability {
                            encoding: Encoding::Utf16Le,
                            bom: "required",
                        },
                        SourceEncodingCapability {
                            encoding: Encoding::Utf16Be,
                            bom: "required",
                        },
                    ],
                    malformed: "rejected",
                    correspondence: "scalarBoundaries",
                },
            }
        });
    &CAPABILITIES
}

/// The grammar inventory, built once.
#[must_use]
pub fn grammar_capabilities() -> &'static [crate::types::GrammarCapability] {
    &analysis_capabilities().grammar_capabilities
}

fn build_grammar_capabilities() -> Vec<crate::types::GrammarCapability> {
    RegistryView::installed().capabilities()
}

fn grammar_capability(entry: &LanguageEntry) -> crate::types::GrammarCapability {
    crate::types::GrammarCapability {
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
        structural_search: entry.admits(crate::text::analysis::AnalysisOperation::StructuralSearch),
        signature_outline: entry.admits(crate::text::analysis::AnalysisOperation::SignatureOutline),
        graph_facts: entry.fact_capabilities.any(),
        facts: entry.fact_capabilities,
        semantic_binding: false,
        syntax_tree: entry.admits(crate::text::analysis::AnalysisOperation::SyntaxTree),
        region_adapters: entry.region_adapters(),
        test_frameworks: entry
            .test_frameworks
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        argument_references: entry.argument_capabilities(),
        value_flow: false,
    }
}

use tree_sitter::Language;

mod admission;
mod registry;
pub use admission::{FactExtractor, FactProvider, RegistryEntry, RegistryView};

use registry::LANGUAGE_TABLE;

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
    pub structural_operation: bool,
    pub syntax_tree_operation: bool,
    pub outline_operation: bool,
    /// Tree-sitter S-expression query whose `@body` captures are the nodes the
    /// signature extractor drops. Every first-class grammar has a body query;
    /// structural, signature, and graph capability inventories therefore agree.
    pub body_query: &'static str,
    pub fact_capabilities: crate::types::GrammarFactCapabilities,
    pub test_frameworks: &'static [&'static str],
    pub(crate) call_adapter: Option<super::calls::CallAdapter>,
    pub(crate) visibility_adapter: super::visibility::VisibilityAdapter,
    pub(crate) export_forwarding_node: Option<&'static str>,
    pub(crate) argument_adapter: Option<super::arguments::ArgumentAdapter>,
    pub(crate) pattern_syntax: PatternSyntax,
    pub comment_style: CommentStyle,
}

impl LanguageEntry {
    pub fn admitted(&self, operation: crate::text::analysis::AnalysisOperation) -> Option<&Self> {
        self.admits(operation).then_some(self)
    }
    pub fn admits(&self, operation: crate::text::analysis::AnalysisOperation) -> bool {
        use crate::text::analysis::AnalysisOperation as Op;
        match operation {
            Op::StructuralSearch => self.structural_operation,
            Op::SyntaxTree => self.syntax_tree_operation,
            Op::SignatureOutline => self.outline_operation && !self.body_query.is_empty(),
            Op::Declarations => self.fact_capabilities.declarations,
            Op::GraphFacts => self.fact_capabilities.any(),
        }
    }

    pub fn region_adapters(&self) -> Vec<crate::types::RegionAdapterCapability> {
        use crate::text::analysis::AnalysisOperation as Op;
        let mut adapters = Vec::new();
        if self.pattern_syntax.function_context {
            adapters.push(crate::types::RegionAdapterCapability {
                id: "c-macro-body".into(),
                version: "1".into(),
                operations: vec![Op::StructuralSearch],
            });
        } else if self.language.id_for_node_kind("macro_invocation", true) != 0
            && self.language.id_for_node_kind("token_tree", true) != 0
        {
            adapters.push(crate::types::RegionAdapterCapability {
                id: "rust-macro-body".into(),
                version: "1".into(),
                operations: vec![Op::StructuralSearch],
            });
            adapters.push(crate::types::RegionAdapterCapability {
                id: "rust-item-macro-body".into(),
                version: "1".into(),
                operations: vec![Op::Declarations, Op::GraphFacts, Op::SignatureOutline],
            });
        }
        adapters
    }

    pub fn argument_capabilities(&self) -> crate::types::GrammarArgumentCapabilities {
        self.argument_adapter.map_or_else(
            || crate::types::GrammarArgumentCapabilities {
                supported: false,
                forms: Vec::new(),
                unsupported_constructs: vec!["argument extraction unavailable".to_owned()],
            },
            super::arguments::ArgumentAdapter::capabilities,
        )
    }
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

pub fn find_entry(ext: &str) -> Option<&'static LanguageEntry> {
    LANGUAGE_TABLE.iter().find(|e| e.extensions.contains(&ext))
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
#[cfg(test)]
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
    let extension = crate::text::extension_of(path, true, "");
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
    let extension = crate::text::extension_of(path, true, "");
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
mod tests;

#[cfg(any(test, feature = "test-hooks"))]
pub mod conformance;
