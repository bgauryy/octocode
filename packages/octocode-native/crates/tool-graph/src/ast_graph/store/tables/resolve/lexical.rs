//! Where a bare name may bind once the caller's own file and imports did
//! not bind it, by the language's name-resolution rules.
//!
//! Python (scip-python/pyright) and Rust (rust-analyzer) resolve a bare name
//! in the module that spells it: its own items, the names its imports bind
//! and builtins or the prelude. A same-named item in another module is a
//! different binding, so the graph-wide fallback never applies. Go (gopls)
//! adds the files of the caller's package: its directory's files under the
//! caller's package clause, without the `_test.go` files a non-test build
//! never compiles. Dot imports bind through the import itself.
use super::linker::{BareScope, Linked, Linker, Site};
use super::*;

impl Linker<'_> {
    pub(super) fn bare_scope(&self, caller: u32) -> BareScope {
        let file = self.file_of(caller);
        match self.scope.families.get(file as usize).copied() {
            Some("pypi" | "cargo") => BareScope::Module,
            Some("go") => BareScope::Package,
            _ => BareScope::Graph,
        }
    }

    /// A bare Go name declared once in the caller's package: the files of
    /// its directory whose package clause is the caller's. A `_test.go` file
    /// in the directory's package compiles into it, so it sees the package's
    /// names as surely as a non-test file; an external `<pkg>_test` file is
    /// a separate package that reaches them only through its import. A
    /// non-test caller never sees `_test.go` files. Only when a clause is
    /// unknown can a test caller not tell which package it is in: medium.
    pub(super) fn same_go_package(&self, site: &Site, candidates: &[u32]) -> Option<Linked> {
        let caller_file = self.file_of(site.caller);
        let caller_path = self.scope.path(caller_file)?;
        let caller_dir = crate::ast_graph::paths::parent(caller_path);
        let caller_test = caller_path.ends_with("_test.go");
        let caller_package = self.scope.go_package(caller_file);
        let mut clauses_known = caller_package.is_some();
        let in_package = candidates
            .iter()
            .copied()
            .filter(|id| {
                let file = self.file_of(*id);
                self.scope.path(file).is_some_and(|path| {
                    crate::ast_graph::paths::parent(path) == caller_dir
                        && (caller_test || !path.ends_with("_test.go"))
                        && match (caller_package, self.scope.go_package(file)) {
                            (Some(caller), Some(target)) => caller == target,
                            _ => true,
                        }
                })
            })
            .collect::<Vec<_>>();
        let [only] = in_package.as_slice() else {
            return None;
        };
        clauses_known &= self.scope.go_package(self.file_of(*only)).is_some();
        let confidence = if caller_test && !clauses_known {
            Confidence::Medium
        } else {
            Confidence::High
        };
        (site.accept)(*only).then(|| (vec![*only], "same-package", confidence))
    }
}
