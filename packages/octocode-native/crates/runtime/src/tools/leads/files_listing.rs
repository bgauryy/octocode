//! The base of the structureSearch `files` listings a lead offers.
use octocode_contract_types::{
    SortOrder, StructureSearchQueryFiles, StructureSearchQueryFilesOperation, StructureSearchSort,
    StructureSearchSortAnyOrder, StructureSearchSortAnyOrderBy, StructureSearchView,
};
use std::num::NonZeroU64;

/// A structureSearch `files` listing of `path` in a search's walk scope: its
/// globs (`include`, `exclude`), `includeIgnored` and `defaultExcludes` (a
/// `files` listing always shows dot entries). A lead offers it related, so
/// the names a search's globs missed are one call away.
pub(crate) fn scope_listing(
    path: &str,
    (include, exclude): (Vec<String>, Vec<String>),
    include_ignored: bool,
    default_excludes_off: bool,
) -> Option<serde_json::Value> {
    use octocode_contract_types::{DefaultExcludes, StructureSearchQuery};
    let listing = StructureSearchQueryFiles {
        include,
        exclude,
        include_ignored: include_ignored.then_some(true),
        default_excludes: default_excludes_off.then_some(DefaultExcludes(false)),
        ..files_listing(path)?
    };
    Some(
        crate::response::result::Continuation::related_query(StructureSearchQuery::from(listing))
            .build(),
    )
}

/// A structureSearch `files` query of `path` with every optional field
/// unset and the contract defaults: the base of the listings a search leads to.
pub(crate) fn files_listing(path: &str) -> Option<StructureSearchQueryFiles> {
    Some(StructureSearchQueryFiles {
        access: None,
        accessed_at: None,
        classification: None,
        default_excludes: None,
        selection: None,
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
