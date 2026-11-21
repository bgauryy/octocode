//! `tree`: a bounded directory outline, children name-sorted, depth-first,
//! rows grouped by directory ([`super::listing`] runs it).
use super::probe::FilterProbe;

use octocode_contract_types::{StructureSearchQueryTree, StructureSearchSort};
use octocode_engine::num::saturating_u32;
use octocode_engine::types::FileSystemQueryOptions;
use octocode_engine::types::IgnorePolicy;
use octocode_runtime_base::response::result::ToolError;

// Engine-unit views over the generated `tree` query.

/// Levels below `path` (1 = its children), as the engine counts them.
/// Omitted, a listing shows `path`'s children; a filter (`include`,
/// `extensions`, `entryType`) searches every level, like ghStructure.
fn walk_depth(query: &StructureSearchQueryTree) -> u32 {
    saturating_u32(query.max_depth.get())
}

super::listing::listing_fields!(StructureSearchQueryTree {
    /// Rows grouped by directory.
    const LAYOUT: &'static str = "workspaceRelativeTreeGroups";
    /// An outline lists in walk order.
    fn sort(&self) -> StructureSearchSort {
        StructureSearchSort::AscOnly(octocode_contract_types::SearchSortAscOnly {
            by: octocode_contract_types::TraversalBy::Traversal,
            order: octocode_contract_types::SearchSortAscOnlyOrder::Asc,
        })
    }
    /// An outline walks its whole scope, so it knows how many rows matched.
    fn early_exit(&self) -> bool {
        false
    }
    fn filter_probe(&self, root: &std::path::Path) -> Result<FilterProbe, ToolError> {
        FilterProbe::new(
            root,
            &self.include,
            &self.extensions.iter().map(ToString::to_string).collect::<Vec<_>>(),
            None,
        )
    }
    fn engine_options(&self, root: &std::path::Path) -> FileSystemQueryOptions {
        let include = self.include.clone();
        FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            include_root: Some(false),
            recursive: Some(true),
            max_depth: Some(walk_depth(self)),
            ignore: IgnorePolicy::new(self.include_ignored, Some(self.include_hidden == Some(true))),
            names: (!include.is_empty()).then_some(include),
            extensions: (!self.extensions.is_empty()).then(|| self.extensions.iter().map(ToString::to_string).collect()),
            entry_type: self.entry_type.map(super::files::entry_kind),
            exclude: octocode_tools_shared::variant_field::non_empty(&self.exclude),
            ..Default::default()
        }
    }
    /// Without its name filters an unbounded tree would list one level:
    /// the broadened listing keeps the depth this one walked.
    fn broadening(&self) -> Self {
        self.clone()
    }
    /// A file names no outline: the call leads to its read instead.
    fn check_root(
        &self,
        root: &std::path::Path,
        paths: &octocode_runtime_base::policy::path::PathPolicy,
    ) -> Result<(), ToolError> {
        if root.is_file() {
            return Err(super::file_root(root, paths));
        }
        Ok(())
    }
});
