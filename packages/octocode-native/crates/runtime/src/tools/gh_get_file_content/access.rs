use crate::tools::variant_field::variant_field;
use octocode_contract_types::GhGetFileContentQuery;

/// Binds `$q` to the query struct of every view.
macro_rules! each {
    ($query:expr, $q:ident, $body:expr) => {
        variant_field!($query, each [
            GhGetFileContentQuery::Content0, GhGetFileContentQuery::ContentRanges,
            GhGetFileContentQuery::ContentMatchString, GhGetFileContentQuery::Compact3,
            GhGetFileContentQuery::CompactRanges, GhGetFileContentQuery::Symbols,
        ] $q => $body)
    };
}
pub(super) fn owner(q: &GhGetFileContentQuery) -> &str {
    each!(q, v, v.owner.as_str())
}
pub(super) fn repo(q: &GhGetFileContentQuery) -> &str {
    each!(q, v, v.repo.as_str())
}
pub(super) fn path(q: &GhGetFileContentQuery) -> &str {
    each!(q, v, v.path.as_str())
}
pub(super) fn reference(q: &GhGetFileContentQuery) -> Option<&str> {
    each!(q, v, v.ref_.as_ref().map(|r| r.as_str()))
}
pub(super) fn progress(
    q: &GhGetFileContentQuery,
) -> Option<&octocode_contract_types::ReadProgress> {
    each!(q, v, v.read_progress.as_ref())
}
pub(super) fn classified(q: &GhGetFileContentQuery) -> bool {
    variant_field!(q, each [
        GhGetFileContentQuery::Content0, GhGetFileContentQuery::ContentRanges,
        GhGetFileContentQuery::ContentMatchString,
    ] v => v.classification.is_some() || v.selection.is_some(), else => false)
}
pub(super) fn set_ref(
    q: &mut GhGetFileContentQuery,
    reference: &str,
) -> Result<(), octocode_github::ProviderError> {
    each!(q, v, {
        v.ref_ = Some(
            reference
                .parse()
                .map_err(super::local_query::decode_display)?,
        );
        Ok(())
    })
}
pub(super) fn set_path(
    q: &mut GhGetFileContentQuery,
    path: &str,
) -> Result<(), octocode_github::ProviderError> {
    each!(q, v, {
        v.path = path.parse().map_err(super::local_query::decode_display)?;
        Ok(())
    })
}
