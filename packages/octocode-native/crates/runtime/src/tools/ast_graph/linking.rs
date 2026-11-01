//! How the files of each grammar link their imports: one record per grammar
//! of the engine registry, so linking branches on a grammar, never on the
//! spelling of an extension.

use super::edges;

/// The import linking of one grammar family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Linker {
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
    pub(super) fn of(file: &str) -> Self {
        let extension = octocode_engine::text::extension_of(file, true, "");
        octocode_engine::signatures::languages::find_entry(&extension)
            .and_then(|entry| BY_GRAMMAR.iter().find(|(name, _)| *name == entry.name))
            .map_or(Self::Unsupported, |(_, linker)| *linker)
    }

    /// The `coverage.languages[].linking` label.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::JsTs => "javascript-relative",
            Self::Python => "python-modules",
            Self::Go => "go-packages",
            Self::Java => "java-imports",
            Self::Rust => "rust-modules",
            Self::CFamily => "c-relative-includes",
            Self::Unsupported => "unsupported",
        }
    }

    /// The edge kind of an import fact of `kind` (`value`, `type`, `module`).
    pub(super) fn import_edge(self, kind: &str) -> &'static str {
        match (self, kind) {
            (Self::Rust, "module") => edges::RUST_MODULE,
            (Self::Rust, "include") => edges::RUST_INCLUDE,
            (Self::Rust, "include-data") => edges::RUST_INCLUDE_DATA,
            (Self::Rust, _) => "rust-use",
            (_, "type") => "type-import",
            (Self::Go, _) => "go-import",
            (Self::Java, _) => "java-import",
            (Self::Python, _) => edges::PYTHON_IMPORT,
            (Self::CFamily, _) => "c-include",
            (Self::JsTs | Self::Unsupported, _) => "static-import",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Linker;

    /// Every spelling of a grammar's extensions links alike, from the
    /// registry: a `.hxx` header like a `.c` file, `.pyi` like `.py`; an
    /// extension no compiled grammar parses links nothing.
    #[test]
    fn linkers_follow_the_registry_grammar() {
        for (file, linker) in [
            ("a.ts", Linker::JsTs),
            ("a.d.ts", Linker::JsTs),
            ("a.mjs", Linker::JsTs),
            ("a.tsx", Linker::JsTs),
            ("a.pyi", Linker::Python),
            ("a.rs", Linker::Rust),
            ("a.go", Linker::Go),
            ("A.java", Linker::Java),
            ("a.h", Linker::CFamily),
            ("a.hxx", Linker::CFamily),
            ("a.cs", Linker::Unsupported),
            ("README", Linker::Unsupported),
        ] {
            assert_eq!(Linker::of(file), linker, "{file}");
        }
        let cuda = octocode_engine::signatures::languages::find_entry("cuh").is_some();
        assert_eq!(
            Linker::of("a.cuh"),
            if cuda {
                Linker::CFamily
            } else {
                Linker::Unsupported
            }
        );
        assert_eq!(Linker::Rust.import_edge("module"), "rust-module");
        assert_eq!(Linker::JsTs.import_edge("type"), "type-import");
    }
}
