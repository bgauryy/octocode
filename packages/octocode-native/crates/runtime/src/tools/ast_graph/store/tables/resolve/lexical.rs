//! Where a bare name may bind once the caller's own file and imports did
//! not bind it, by the language's name-resolution rules.
//!
//! Python (scip-python/pyright) and Rust (rust-analyzer) resolve a bare name
//! in the module that spells it: its own items, the names its imports bind
//! and builtins or the prelude. A same-named item in another module is a
//! different binding, so the graph-wide fallback never applies. Go (gopls)
//! adds the files of the caller's package: its directory, without the
//! `_test.go` files a non-test build never compiles. Dot imports bind
//! through the import itself.
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

    /// A bare Go name declared once in the caller's package. A `_test.go`
    /// caller may belong to the external `_test` package, which sees the
    /// package's names only through an import: that binding stays medium.
    pub(super) fn same_go_package(&self, site: &Site, candidates: &[u32]) -> Option<Linked> {
        let caller_path = self.scope.path(self.file_of(site.caller))?;
        let caller_dir = crate::tools::ast_graph::graph::parent(caller_path);
        let caller_test = caller_path.ends_with("_test.go");
        let in_package = candidates
            .iter()
            .copied()
            .filter(|id| {
                self.scope.path(self.file_of(*id)).is_some_and(|path| {
                    crate::tools::ast_graph::graph::parent(path) == caller_dir
                        && (caller_test || !path.ends_with("_test.go"))
                })
            })
            .collect::<Vec<_>>();
        let [only] = in_package.as_slice() else {
            return None;
        };
        let confidence = if caller_test {
            Confidence::Medium
        } else {
            Confidence::High
        };
        (site.accept)(*only).then(|| (vec![*only], "same-package", confidence))
    }
}
