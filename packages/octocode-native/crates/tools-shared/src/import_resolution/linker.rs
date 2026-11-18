//! Which import linking a file's grammar takes: one record per grammar of
//! the engine registry, so linking branches on a grammar, never on the
//! spelling of an extension.

/// The import linking of one grammar family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linker {
    /// Relative specifiers, `tsconfig` paths and workspace packages.
    JsTs,
    /// Python module paths under the package roots.
    Python,
    /// Go import paths under `go.mod`.
    Go,
    /// Java packages and same-package classes.
    Java,
    /// Rust module paths (`crate::`, `self::`, `super::`, Cargo crates).
    Rust,
    /// C, C++ and CUDA `#include`s.
    CFamily,
    /// No import linking: declarations only.
    Unsupported,
}

/// Each registry grammar (by its name) with its linker; a grammar not
/// listed links nothing.
const BY_GRAMMAR: &[(&str, Linker)] = &[
    ("TypeScript", Linker::JsTs),
    ("TSX", Linker::JsTs),
    ("JavaScript", Linker::JsTs),
    ("Python", Linker::Python),
    ("Go", Linker::Go),
    ("Java", Linker::Java),
    ("Rust", Linker::Rust),
    ("C", Linker::CFamily),
    ("C++", Linker::CFamily),
    ("CUDA", Linker::CFamily),
];

/// The grammar `file` is read with: the one its extension selects, else the
/// one `parser` names (an extension or a language id, as the facts of its
/// parse state it): a `parserGlobs` rule maps an extension no grammar claims
/// to a grammar. `None` when neither names one.
pub fn read_grammar(
    file: &str,
    parser: &str,
) -> Option<&'static octocode_engine::signatures::languages::LanguageEntry> {
    use octocode_engine::signatures::languages;
    let extension = octocode_engine::text::extension_of(file, true, "");
    languages::find_entry(&extension).or_else(|| {
        let parser = parser.trim();
        if parser.is_empty() {
            return None;
        }
        languages::find_entry(parser).or_else(|| languages::select(parser).next())
    })
}

impl Linker {
    /// The linker of `file`, by the grammar its extension selects.
    pub fn of(file: &str) -> Self {
        Self::of_read(file, "")
    }

    /// The linker of `file` read with `parser` ([`read_grammar`]).
    pub fn of_read(file: &str, parser: &str) -> Self {
        read_grammar(file, parser)
            .and_then(|entry| BY_GRAMMAR.iter().find(|(name, _)| *name == entry.name))
            .map_or(Self::Unsupported, |(_, linker)| *linker)
    }
}
