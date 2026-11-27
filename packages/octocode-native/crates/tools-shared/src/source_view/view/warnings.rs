use super::super::types::*;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};

pub(super) fn read_scope_warnings(
    q: &LocalFetchQuery,
    pg: &super::super::pagination::Page,
    warnings: &mut Vec<String>,
) {
    let page = &pg.pagination;
    if let Some(requested) = local_fetch_query::offset(q)
        && local_fetch_query::unit(q) == ReadProgressUnit::Bytes
        && page.unit == ReadProgressUnit::Bytes
        && !pg.out_of_range
        && page.offset < requested
    {
        warnings.push(format!("Byte offset {requested} falls inside a UTF-8 character; the page starts at its first byte, {}.", page.offset));
    }
}
