//! The code graph: [`graph`] links one scan's syntax facts into a
//! [`types::BuiltGraph`]; [`store`] projects it into a persisted snapshot
//! (`graphBuild`) and answers questions from it (`graphQuery`).
mod algorithms;
mod build_scope;
mod cargo;
mod classify;
mod edges;
mod freshness;
mod graph;
mod hashing;
mod languages;
mod limits;
pub mod linking;
mod packages;
mod progress;
pub mod store;
mod types;
pub mod walk_scope;

// The import resolution the graph links by: its modules are reached here by
// the names the graph's own modules use.
use octocode_tools_shared::import_resolution::{aliases, js_paths, paths};

/// Bump when persisted linking semantics change without a binary format
/// change: snapshots and extraction checkpoints of another version rebuild.
///
/// 26: a data member is shadowed only by a callable of its own type; a
/// function-typed member is called by its bare name inside its type (Scala,
/// C#, C++); declared-type chains guard cycles per path; a workspace
/// member reaches itself through its `node_modules` link.
/// 27: declared types follow a global `Window` interface merged across files,
/// and the parameter a call passes a function to.
/// 28: a declared type's bare name reaches a merged global only when no scope
/// binds it (not an import, not a name match in another module), and `window`
/// is `globalThis.Window` whatever the file declares or imports.
/// 29: a Rust call whose whole qualifier is `self`, `super` or `crate` binds
/// in the module the keyword names (its declarations and `use` bindings), and
/// a `super`/`self` path resolves from its own module file without a scanned
/// crate root.
/// 30: a file's facts are stored as its source judged whole shows them: every
/// value the whole-source secret pass replaces is replaced in the names and
/// texts extracted from it.
/// 31: a named re-export links its file to each declaration it re-exports
/// (`uses`, detail `reexported-name`), so the public API is known per name.
/// 32: a snapshot's `linkage` states its `sites` total (eligible + excluded),
/// which reused snapshots answer from their manifest.
/// 33: a TypeScript value import its file uses only as a type is a
/// `type-use-import` edge of unknown load, so a cycle only such edges close
/// is `runtime: "unknown"`.
/// 34: a bare call of a term bound in scope (`term_binding`: a parameter, a
/// local variable, a closure) links nothing and stays unresolved as
/// `local-binding`, in every language that records the fact; a JS/TS call
/// bound to a declaration (`bound_declaration`) links that one, high.
/// 35: an unlinked bare JS/TS specifier whose code the configuration places
/// beyond the snapshot root (a workspace sibling or path dependency outside
/// it, an alias or `baseUrl` module above it) is diagnosed as outside the
/// root, like a relative import climbing out of it.
/// 36: so is every language's: a Go module of the enclosing module, a local
/// `replace` or `go.work` member outside the root; a Java class above it; a
/// Rust path dependency outside it; a Python module above it, of the enclosing
/// package, or of a local path dependency a `pyproject.toml` names (until now
/// external). One recorder diagnoses all of them.
/// 37: a repeated name's key suffix is its place among the same-named
/// declarations (`name@1`), not its line, so a moved line moves no key; a
/// Rust path rooted at `::` names an extern crate, never an unresolved
/// internal one.
/// 38: the manifest states, per declared name, the call sites that name it on
/// a receiver no edge resolves (`unresolvedCallNames`), so `callers` can say
/// it is partial without reading evidence text.
/// 39: a source that is not UTF-8 (8-bit text, Latin-1 or with stray bytes)
/// is indexed as its decoded text instead of skipped as unreadable.
/// 40: a Rust `use` path starts at a name another `use` binds (`use
/// streamed::f` after `use scanned::streamed`), and `use super::*` binds the
/// parent's `use`s for the paths and calls that name them; lockfiles and other
/// files no linking step reads are no snapshot inputs.
/// 41: a Rust module file another crate also mounts by `#[path]` resolves
/// `crate::` and `super::` through its home crate's plain `mod`; a method
/// call on a receiver of a type the repository does not declare is
/// `not-declared-in-repo`, outside the linkage ratio.
/// 42: a resolved Rust path outside call and `use` position (a value path,
/// a path in a `macro_rules!` body) is a `uses` edge with via `path` to the
/// item it names, so a file naming only a sibling item binds its module.
/// 43: the manifest lists the files with unextracted argument groups, so
/// symbol impact keeps such an importer as a candidate.
/// 44: each such file lists the declared names its unextracted groups write
/// (`null`: any name), so symbol impact keeps the importer only when one
/// names an affected symbol.
/// 45: a Rust path whose first segment a `use super::*` binds through a
/// parent that itself only globs its own parent binds through that
/// ancestor's `use`, so the file links to what the path names; a bare Rust
/// type name in type or struct-literal position binds through such globs too.
/// A Rust path that reaches its item through a re-export is a `uses` edge
/// with detail `reexport`, so its file is a dependent of the item's file.
/// 46: the glob walk also follows a parent's `use child::*` to the child's
/// declarations and bound names. A Rust call or path drops its turbofish
/// (`m::f::<T>()` is `m::f`) and, headed by a module, uses the item it names
/// (`format::Reader::open()` uses `Reader`), so a call through a re-export
/// makes its file a dependent of the declaring file. The same holds for a
/// path whose module a `use super::*` chain binds (`format::Reader::open()`
/// two globs below the `mod format;`), and for a bare call a glob import binds
/// to a declaration its module re-exports: the file uses the re-export. A
/// child's `pub(super)` or `pub(crate)` item passes through the glob as a
/// `pub` one does, and a file's own `use child::*` binds the names the
/// child passes on as a `use super::*` binds its parent's.
const ANALYSIS_VERSION: u32 = 46;

/// graphBuild's and graphQuery's recovery for the shared response stages.
pub struct Output;
impl octocode_runtime_base::response::output::ToolOutput for Output {}

#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod test_support;
