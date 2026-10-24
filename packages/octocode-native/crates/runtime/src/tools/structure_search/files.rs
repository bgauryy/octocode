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
    non_empty(&query.include)
}
fn entry_kind(query: &StructureSearchQueryFiles) -> Option<EntryType> {
    use octocode_contract_types::StructureSearchQueryFilesEntryType;
    query.entry_type.map(|kind| match kind {
        StructureSearchQueryFilesEntryType::File => EntryType::File,
        StructureSearchQueryFilesEntryType::Directory => EntryType::Directory,
    })
}
fn permission_bits(query: &StructureSearchQueryFiles) -> Option<String> {
    query.permissions.as_ref().map(ToString::to_string)
}

super::listing::listing_fields!(StructureSearchQueryFiles {
    /// Rows grouped by directory.
    const LAYOUT: &'static str = "workspaceRelativeDirGroups";
    fn sort(&self) -> StructureSearchQueryFilesSort {
        self.sort.clone()
    }
    /// Path order is the walk order (name-sorted, depth-first), so the first
    /// `limit` matches are final once found: the walk stops there. Other
    /// sorts rank the whole walk.
    fn early_exit(&self) -> bool {
        is_traversal(&self.sort)
    }
    fn filter_probe(&self, root: &std::path::Path) -> Result<FilterProbe, ToolError> {
        FilterProbe::new(
            root,
            &include_globs(self).unwrap_or_default(),
            &self.extensions.iter().map(ToString::to_string).collect::<Vec<_>>(),
            self.name_regex.as_deref().map(String::as_str),
        )
    }
    fn engine_options(&self, root: &std::path::Path) -> FileSystemQueryOptions {
        use octocode_contract_types::StructureSearchQueryFilesAccess;
        FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            include_root: Some(!root.is_dir()),
            recursive: Some(true),
            max_depth: depth(self.max_depth),
            min_depth: depth(self.min_depth),
            ignore: octocode_engine::search::walk::IgnorePolicy::new(
                self.include_ignored,
                self.include_hidden,
            ),
            names: include_globs(self),
            extensions: (!self.extensions.is_empty()).then(|| self.extensions.iter().map(ToString::to_string).collect()),
            regex: self.name_regex.as_ref().map(ToString::to_string),
            entry_type: entry_kind(self),
            empty: self.empty,
            modified_from: self.modified_at.as_ref().and_then(|range| range.from.as_ref()).and_then(|value| timestamp(value.as_str())),
            modified_to: self.modified_at.as_ref().and_then(|range| range.to.as_ref()).and_then(|value| timestamp(value.as_str())),
            accessed_from: self.accessed_at.as_ref().and_then(|range| range.from.as_ref()).and_then(|value| timestamp(value.as_str())),
            accessed_to: self.accessed_at.as_ref().and_then(|range| range.to.as_ref()).and_then(|value| timestamp(value.as_str())),
            size_min: self.size_bytes.as_ref().and_then(|range|range.min).and_then(|value| u64::try_from(value).ok()),
            size_max: self.size_bytes.as_ref().and_then(|range|range.max).and_then(|value| u64::try_from(value).ok()),
            permissions: permission_bits(self),
            executable: Some(self.access == Some(StructureSearchQueryFilesAccess::Executable)),
            readable: Some(self.access == Some(StructureSearchQueryFilesAccess::Readable)),
            writable: Some(self.access == Some(StructureSearchQueryFilesAccess::Writable)),
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

pub(super) fn is_traversal(sort: &StructureSearchQueryFilesSort) -> bool {
    matches!(sort, StructureSearchQueryFilesSort::Variant1 { .. })
}
pub(super) fn sort_key(
    sort: &StructureSearchQueryFilesSort,
) -> Option<octocode_contract_types::StructureSearchQueryFilesSortVariant0By> {
    match sort {
        StructureSearchQueryFilesSort::Variant0 { by, .. } => Some(*by),
        _ => None,
    }
}
fn timestamp(value: &str) -> Option<std::time::SystemTime> {
    let (seconds, nanos) = crate::civil_date::parse_rfc3339(value)?;
    let seconds = if seconds >= 0 {
        std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(seconds.unsigned_abs()))
    } else {
        std::time::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(seconds.unsigned_abs()))
    }?;
    seconds.checked_add(std::time::Duration::from_nanos(u64::from(nanos)))
}
