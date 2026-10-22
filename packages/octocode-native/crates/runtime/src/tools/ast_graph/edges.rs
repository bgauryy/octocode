//! The kinds of file edge the graph builder emits, and what each means for
//! module loading. One table: the builder, the cycle engine and the
//! detectors read it, so they classify an edge the same way.

/// `mod child;`: module structure, not a dependency.
pub(super) const RUST_MODULE: &str = "rust-module";
/// A Python import inside a function body: it runs when the function is
/// called, not when the module loads.
pub(super) const PYTHON_LAZY_IMPORT: &str = "python-lazy-import";
pub(super) const PYTHON_IMPORT: &str = "python-import";
pub(super) const DYNAMIC_IMPORT: &str = "dynamic-import";
pub(super) const RUST_PATH: &str = "rust-path";
/// `include_str!`/`include_bytes!`: the target is embedded as data.
pub(super) const RUST_INCLUDE_DATA: &str = "rust-include-data";

/// When the import an edge records takes effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Load {
    /// While the importing module loads: a loop of these is a runtime
    /// (module-load) cycle.
    Eager,
    /// Later, on a call (a dynamic `import()`, a Python import inside a
    /// function).
    Deferred,
    /// Never: erased before run time (`import type`).
    TypeOnly,
    /// At compile or link time, with no load order (Rust `use` and paths,
    /// Go and Java imports, C includes).
    CompileTime,
    /// Module structure (a Rust `mod` declaration): not a dependency.
    ModuleTree,
    /// Never as code: the target is embedded as text or bytes. A build
    /// input (its dependents rebuild), but it adds no names and closes no
    /// dependency cycle.
    Data,
}

/// Every edge kind with its [`Load`].
const KINDS: &[(&str, Load)] = &[
    ("static-import", Load::Eager),
    ("named-reexport", Load::Eager),
    ("star-reexport", Load::Eager),
    ("commonjs-require", Load::Eager),
    ("create-require", Load::Eager),
    (PYTHON_IMPORT, Load::Eager),
    (DYNAMIC_IMPORT, Load::Deferred),
    (PYTHON_LAZY_IMPORT, Load::Deferred),
    ("type-import", Load::TypeOnly),
    ("type-named-reexport", Load::TypeOnly),
    ("type-star-reexport", Load::TypeOnly),
    ("rust-use", Load::CompileTime),
    (RUST_PATH, Load::CompileTime),
    ("go-import", Load::CompileTime),
    ("java-import", Load::CompileTime),
    ("java-same-package", Load::CompileTime),
    ("c-include", Load::CompileTime),
    (RUST_MODULE, Load::ModuleTree),
    (RUST_INCLUDE_DATA, Load::Data),
];

/// The [`Load`] of edge `kind`. A kind outside the table (a store edge
/// such as `unparsed-target`) has no load order: compile time.
pub(super) fn load(kind: &str) -> Load {
    KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or(Load::CompileTime, |(_, load)| *load)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind the builder emits is classified once: the table names no
    /// kind twice, and the builder's own kind literals are in it.
    #[test]
    fn every_emitted_kind_is_classified_once() {
        let mut names = KINDS.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a kind is listed twice");
        let builder = include_str!("graph.rs");
        for literal in builder
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|text| text.ends_with("-import") || text.ends_with("-reexport"))
        {
            assert!(names.contains(&literal), "{literal} is not classified");
        }
        assert_eq!(load("static-import"), Load::Eager);
        assert_eq!(load(PYTHON_LAZY_IMPORT), Load::Deferred);
        assert_eq!(load(RUST_INCLUDE_DATA), Load::Data);
        assert_eq!(load("unparsed-target"), Load::CompileTime);
    }
}
