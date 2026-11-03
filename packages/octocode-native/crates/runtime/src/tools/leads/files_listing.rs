//! The base of the structureSearch `files` listings a lead offers.
use octocode_contract_types::{
    SortOrder, StructureSearchQueryFiles, StructureSearchQueryFilesOperation, StructureSearchSort,
    StructureSearchSortAnyOrder, StructureSearchSortAnyOrderBy, StructureSearchView,
};
use std::num::NonZeroU64;

/// A structureSearch `files` query of `path` with every optional field
/// unset and the contract defaults: the base of the listings a search leads to.
pub(crate) fn files_listing(path: &str) -> Option<StructureSearchQueryFiles> {
    Some(StructureSearchQueryFiles {
        access: None,
        accessed_at: None,
        default_excludes: None,
        empty: None,
        entry_type: None,
        exclude: Vec::new(),
        extensions: Vec::new(),
        include: Vec::new(),
        include_hidden: None,
        include_ignored: None,
        limit: None,
        main_goal: None,
        max_depth: None,
        min_depth: None,
        modified_at: None,
        name_regex: None,
        operation: StructureSearchQueryFilesOperation::Files,
        page: NonZeroU64::MIN,
        path: path.try_into().ok()?,
        permissions: None,
        reasoning: None,
        scan_offset: None,
        size_bytes: None,
        snapshot: None,
        sort: StructureSearchSort::AnyOrder(StructureSearchSortAnyOrder {
            by: StructureSearchSortAnyOrderBy::Path,
            order: SortOrder::Asc,
        }),
        test_code: None,
        view: StructureSearchView::Details,
        walk_offset: None,
    })
}
