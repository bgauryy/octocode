//! `tree`: a bounded directory outline, children name-sorted, depth-first,
//! rows grouped by directory ([`super::listing`] runs it).

use crate::num::{u32_of, u32_of_usize};
use crate::response::result::ToolError;
use octocode_contract_types::{
    StructureSearchQueryFilesSort, StructureSearchQueryTree, StructureSearchQueryTreeEntryType,
};
use octocode_engine::search::walk::IgnorePolicy;
use octocode_engine::types::{EntryType, FileSystemQueryOptions};

// Engine-unit views over the generated `tree` query.

/// Levels below `path` (1 = its children), as the engine counts them.
/// Omitted, a listing shows `path`'s children; a filter (`include`,
/// `extensions`, `entryType`) searches every level, like ghStructure.
fn walk_depth(query: &StructureSearchQueryTree) -> u32 {
    query.max_depth.map_or_else(
        || {
            if super::listing::Listing::filtered(query) {
                u32_of_usize(crate::contracts::query_limits::structure_search::MAX_DEPTH_MAXIMUM)
            } else {
                1
            }
        },
        |depth| u32_of(depth.get()),
    )
}

super::listing::listing_fields!(StructureSearchQueryTree {
    /// Rows grouped by directory.
    const LAYOUT: &'static str = "workspaceRelativeTreeGroups";
    /// An outline lists in walk order.
    fn sort(&self) -> StructureSearchQueryFilesSort {
        StructureSearchQueryFilesSort::Path
    }
    /// An outline walks to its ceiling, so it knows how many rows matched.
    fn early_exit(&self) -> bool {
        false
    }
    fn filter_probe(&self, root: &std::path::Path) -> Result<super::FilterProbe, ToolError> {
        super::FilterProbe::new(
            root,
            &crate::policy::include::include_globs(&self.include),
            &self.extensions,
            None,
        )
    }
    fn engine_options(&self, root: &std::path::Path) -> FileSystemQueryOptions {
        let include = crate::policy::include::include_globs(&self.include);
        FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            include_root: Some(false),
            recursive: Some(true),
            max_depth: Some(walk_depth(self)),
            ignore: IgnorePolicy::new(self.include_ignored, Some(self.include_hidden == Some(true))),
            names: (!include.is_empty()).then_some(include),
            extensions: (!self.extensions.is_empty()).then(|| self.extensions.clone()),
            entry_type: self.entry_type.map(|kind| match kind {
                StructureSearchQueryTreeEntryType::F => EntryType::File,
                StructureSearchQueryTreeEntryType::D => EntryType::Directory,
            }),
            exclude: (!self.exclude.is_empty()).then(|| self.exclude.clone()),
            ..Default::default()
        }
    }
    /// Without its name filters an unbounded tree would list one level:
    /// the broadened listing keeps the depth this one walked.
    fn broadening(&self) -> Self {
        let mut walked = self.clone();
        walked.max_depth = std::num::NonZeroU64::new(u64::from(walk_depth(self)));
        walked
    }
    /// A file names no outline: the call leads to its read instead.
    fn check_root(
        &self,
        root: &std::path::Path,
        paths: &crate::policy::path::PathPolicy,
    ) -> Result<(), ToolError> {
        if root.is_file() {
            return Err(super::file_root(root, paths));
        }
        Ok(())
    }
});
