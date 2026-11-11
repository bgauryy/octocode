//! How the code graph links by a file's grammar
//! ([`crate::tools::import_resolution::Linker`]): the coverage label and the
//! edge kind of each import fact.

use super::edges;
pub(super) use crate::tools::import_resolution::Linker;

impl Linker {
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

    /// The edge kind of an import fact of `kind` (`value`, `type`, `module`,
    /// or [`octocode_engine::graph::UNKNOWN_LOAD`]).
    pub(super) fn import_edge(self, kind: &str) -> &'static str {
        match (self, kind) {
            (Self::Rust, "module") => edges::RUST_MODULE,
            (Self::Rust, "include") => edges::RUST_INCLUDE,
            (Self::Rust, "include-data") => edges::RUST_INCLUDE_DATA,
            (Self::Rust, _) => "rust-use",
            (_, "type") => "type-import",
            (Self::JsTs | Self::Unsupported, octocode_engine::graph::UNKNOWN_LOAD) => {
                edges::RECOVERED_IMPORT
            }
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
