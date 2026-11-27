//! How the code graph links by a file's grammar
//! ([`octocode_tools_shared::import_resolution::Linker`]): the coverage label and the
//! edge kind of each import fact.

use super::edges;
pub use octocode_tools_shared::import_resolution::Linker;

/// The graph's reading of a [`Linker`]: its coverage label and the edge kind of
/// each import fact.
pub(crate) trait LinkerGraph {
    /// The `coverage.languages[].linking` label.
    fn label(self) -> &'static str;
    /// The edge kind of an import fact of `kind`.
    fn import_edge(self, kind: &str) -> &'static str;
    /// Whether the grammar invokes a macro with syntax of its own (Rust
    /// `name!(..)`), so a macro and a function are separate namespaces. A
    /// C-family function-like macro is invoked with call syntax
    /// (`listLength(l)`): a call reaches it as it reaches a function.
    fn invokes_macros(self) -> bool;
}

impl LinkerGraph for Linker {
    /// The `coverage.languages[].linking` label.
    fn label(self) -> &'static str {
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
    fn import_edge(self, kind: &str) -> &'static str {
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

    fn invokes_macros(self) -> bool {
        matches!(self, Self::Rust)
    }
}

#[cfg(test)]
mod tests;
