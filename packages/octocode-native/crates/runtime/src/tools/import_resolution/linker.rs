//! Which import linking a file's grammar takes: one record per grammar of
//! the engine registry, so linking branches on a grammar, never on the
//! spelling of an extension.

/// The import linking of one grammar family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Linker {
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

impl Linker {
    /// The linker of `file`, by the grammar its extension selects.
    pub(crate) fn of(file: &str) -> Self {
        let extension = octocode_engine::text::extension_of(file, true, "");
        octocode_engine::signatures::languages::find_entry(&extension)
            .and_then(|entry| BY_GRAMMAR.iter().find(|(name, _)| *name == entry.name))
            .map_or(Self::Unsupported, |(_, linker)| *linker)
    }
}
