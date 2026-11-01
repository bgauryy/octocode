//! `testCode` on `references` and `callers`: which listed sites stay, by the
//! one test-code rule every local tool uses ([`crate::tools::test_code`]).
//! A site is test code by its file (path, test-only mount) or its line (a
//! Rust `#[cfg(test)]` item); what the scope leaves out is counted as
//! `testCodeExcluded`, files left out whole and lines otherwise.

use super::lsp_search_query;
use super::render::uri_to_path;
use crate::policy::path::PathPolicy;
use crate::tools::test_code::{Excluded, Scoped};
use octocode_contract_types::LspSearchQuery;
use serde_json::Value;
use std::path::Path;

/// Keep the server locations (`uri`, zero-based `range`) on the side the
/// query's `testCode` keeps. `None` when nothing was left out.
pub(super) fn retain_locations(
    query: &LspSearchQuery,
    paths: &PathPolicy,
    locations: &mut Vec<Value>,
) -> Option<Excluded> {
    retain(query, paths, locations, |location| {
        let path = uri_to_path(location.get("uri")?.as_str()?);
        let line = location.pointer("/range/start/line")?.as_u64()?;
        Some((path, line + 1))
    })
}

/// Keep the incoming-call rows whose caller (`from`, at its one-based
/// declaration line) lies on the side the query's `testCode` keeps.
pub(super) fn retain_callers(
    query: &LspSearchQuery,
    paths: &PathPolicy,
    items: &mut Vec<Value>,
) -> Option<Excluded> {
    retain(query, paths, items, |item| {
        let caller = item.get("from")?;
        let line = caller
            .pointer("/displayRange/startLine")
            .or_else(|| item.pointer("/fromRanges/0/startLine"))?
            .as_u64()?;
        Some((caller.get("path")?.as_str()?.to_owned(), line))
    })
}

/// State what the scope left out on `row` (`testCodeExcluded`).
pub(super) fn disclose(row: &mut Value, excluded: Option<Excluded>) {
    if let (Some(excluded), Some(fields)) = (excluded, row.as_object_mut()) {
        fields.insert(
            "testCodeExcluded".into(),
            serde_json::to_value(excluded).unwrap_or(Value::Null),
        );
    }
}

/// Keep each row whose `site` (file, one-based line) the scope keeps; a
/// row without a site stays.
fn retain(
    query: &LspSearchQuery,
    paths: &PathPolicy,
    rows: &mut Vec<Value>,
    site: impl Fn(&Value) -> Option<(String, u64)>,
) -> Option<Excluded> {
    let scope = Scoped::of(lsp_search_query::test_code(query), paths)?;
    rows.retain(|row| {
        site(row).is_none_or(|(path, line)| scope.keeps(Path::new(&path), crate::num::u32_of(line)))
    });
    scope.excluded().stated()
}
