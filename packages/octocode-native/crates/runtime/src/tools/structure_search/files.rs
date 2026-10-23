//! `files`: paths found by name or metadata, sorted, path-ordered rows
//! grouped like `tree` ([`super::listing`] runs it).
use super::probe::FilterProbe;

use crate::num::u32_of;
use crate::response::result::ToolError;
use octocode_contract_types::{StructureSearchQueryFiles, StructureSearchQueryFilesSort};
use octocode_engine::types::{EntryType, FileSystemQueryOptions};

// Engine-unit views over the generated `files` query.

/// Levels below `path` (1 = its children), as `tree` and the engine
/// count them.
fn depth(value: Option<std::num::NonZeroU64>) -> Option<u32> {
    value.map(|depth| u32_of(depth.get()))
}
fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}
fn include_globs(query: &StructureSearchQueryFiles) -> Option<Vec<String>> {
    non_empty(&query.include).map(|globs| crate::policy::include::include_globs(&globs))
}
fn entry_kind(query: &StructureSearchQueryFiles) -> Option<EntryType> {
    use octocode_contract_types::StructureSearchQueryFilesEntryType as Listed;
    query.entry_type.map(|kind| match kind {
        Listed::F => EntryType::File,
        Listed::D => EntryType::Directory,
    })
}
fn permission_bits(query: &StructureSearchQueryFiles) -> Option<String> {
    query.permissions.as_ref().map(ToString::to_string)
}

super::listing::listing_fields!(StructureSearchQueryFiles {
    /// Rows grouped by directory.
    const LAYOUT: &'static str = "workspaceRelativeDirGroups";
    fn sort(&self) -> StructureSearchQueryFilesSort {
        self.sort
    }
    /// Path order is the walk order (name-sorted, depth-first), so the first
    /// `limit` matches are final once found: the walk stops there. Other
    /// sorts rank the whole walk.
    fn early_exit(&self) -> bool {
        self.sort == StructureSearchQueryFilesSort::Path
    }
    fn filter_probe(&self, root: &std::path::Path) -> Result<FilterProbe, ToolError> {
        FilterProbe::new(
            root,
            &include_globs(self).unwrap_or_default(),
            &self.extensions,
            self.name_regex.as_deref(),
        )
    }
    fn engine_options(&self, root: &std::path::Path) -> FileSystemQueryOptions {
        use octocode_contract_types::StructureSearchQueryFilesAccess as Access;
        let time = &self.time;
        FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            include_root: Some(true),
            recursive: Some(true),
            max_depth: depth(self.max_depth),
            min_depth: depth(self.min_depth),
            ignore: octocode_engine::search::walk::IgnorePolicy::new(
                self.include_ignored,
                self.include_hidden,
            ),
            names: include_globs(self),
            extensions: non_empty(&self.extensions),
            regex: self.name_regex.clone(),
            entry_type: entry_kind(self),
            empty: self.empty,
            modified_within: time
                .as_ref()
                .and_then(|t| t.modified_within.as_deref())
                .cloned(),
            modified_before: time
                .as_ref()
                .and_then(|t| t.modified_before.as_deref())
                .cloned(),
            accessed_within: time
                .as_ref()
                .and_then(|t| t.accessed_within.as_deref())
                .cloned(),
            size_greater: self
                .size
                .as_ref()
                .and_then(|s| s.greater.as_ref())
                .map(ToString::to_string),
            size_less: self
                .size
                .as_ref()
                .and_then(|s| s.less.as_ref())
                .map(ToString::to_string),
            permissions: permission_bits(self),
            executable: Some(self.access == Some(Access::Executable)),
            readable: Some(self.access == Some(Access::Readable)),
            writable: Some(self.access == Some(Access::Writable)),
            exclude: non_empty(&self.exclude),
            ..Default::default()
        }
    }
    fn broadening(&self) -> Self {
        self.clone()
    }
    /// A file root lists itself.
    fn check_root(
        &self,
        _root: &std::path::Path,
        _paths: &crate::policy::path::PathPolicy,
    ) -> Result<(), ToolError> {
        Ok(())
    }
});
