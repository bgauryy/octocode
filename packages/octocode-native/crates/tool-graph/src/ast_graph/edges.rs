//! The kinds of file edge the graph builder emits, and what each means for
//! module loading. One table: the builder, the cycle engine and the
//! detectors read it, so they classify an edge the same way.

/// `mod child;`: module structure, not a dependency.
pub(super) const RUST_MODULE: &str = "rust-module";
/// `include!("file.rs")`: the file's items are spliced into the including
/// module. A compile-time dependency, and part of that module.
pub(super) const RUST_INCLUDE: &str = "rust-include";
/// A Python import inside a function body: it runs when the function is
/// called, not when the module loads.
pub(super) const PYTHON_LAZY_IMPORT: &str = "python-lazy-import";
pub(super) const PYTHON_IMPORT: &str = "python-import";
pub(super) const DYNAMIC_IMPORT: &str = "dynamic-import";
pub(super) const RUST_PATH: &str = "rust-path";
/// `include_str!`/`include_bytes!`: the target is embedded as data.
pub(super) const RUST_INCLUDE_DATA: &str = "rust-include-data";
/// An import or re-export of a JS/TS file only the tree-sitter fallback lane
/// read: the edge is real, its load is not established.
pub(super) const RECOVERED_IMPORT: &str = "recovered-import";
pub(super) const RECOVERED_NAMED_REEXPORT: &str = "recovered-named-reexport";
pub(super) const RECOVERED_STAR_REEXPORT: &str = "recovered-star-reexport";
/// A TypeScript value import whose binding the file uses only as a type
/// (`typeUseOnly`): TypeScript's emit elides it unless the project keeps
/// value imports (`verbatimModuleSyntax`, decorator metadata), which the
/// graph does not read, so its load is not established.
pub(super) const TYPE_USE_IMPORT: &str = "type-use-import";

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
    /// Not established: a parser fallback read the edge, or a TypeScript
    /// import is used only as a type and the project's emit settings decide
    /// it, so it may load eagerly or be erased.
    Unknown,
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
    (RECOVERED_IMPORT, Load::Unknown),
    (RECOVERED_NAMED_REEXPORT, Load::Unknown),
    (RECOVERED_STAR_REEXPORT, Load::Unknown),
    (TYPE_USE_IMPORT, Load::Unknown),
    ("rust-use", Load::CompileTime),
    (RUST_PATH, Load::CompileTime),
    ("go-import", Load::CompileTime),
    ("java-import", Load::CompileTime),
    ("java-same-package", Load::CompileTime),
    ("c-include", Load::CompileTime),
    (RUST_MODULE, Load::ModuleTree),
    (RUST_INCLUDE, Load::CompileTime),
    (RUST_INCLUDE_DATA, Load::Data),
];

/// Edge kinds whose target becomes part of the source's module (a declared
/// `mod`, a spliced `include!`): the target shares the module's test role.
const MOUNTS: &[&str] = &[RUST_MODULE, RUST_INCLUDE];

pub(super) fn mounts(kind: &str) -> bool {
    MOUNTS.contains(&kind)
}

/// The [`Load`] of edge `kind`. A kind outside the table (a store edge
/// such as `unparsed-target`) has no load order: compile time.
pub(super) fn load(kind: &str) -> Load {
    KINDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or(Load::CompileTime, |(_, load)| *load)
}

#[cfg(test)]
mod tests;
