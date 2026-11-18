//! `testCode` on `references` and `callers`: which listed sites stay, by the
//! one test-code rule every local tool uses ([`octocode_tools_shared::test_code`]).
//! A site is test code by its file (path, test-only mount) or its line (a
//! Rust `#[cfg(test)]` item); what the scope leaves out is counted as
//! `testCodeExcluded`, files left out whole and lines otherwise. A verdict that
//! needed a file it could not read leaves the row partial.

use super::lsp_search_query;
use super::render::uri_to_path;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::types::TestCodeExcluded;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_tools_shared::test_code::Scoped;
use serde_json::Value;
use std::path::Path;

/// What the scope left out of a row, and the files its verdicts could not read.
#[derive(Default)]
pub(super) struct ScopeLeft {
    pub(super) excluded: Option<TestCodeExcluded>,
    pub(super) unread: Vec<std::path::PathBuf>,
}

/// Keep the server locations (`uri`, zero-based `range`) on the side the
/// query's `testCode` keeps.
pub(super) fn retain_locations(
    query: &LspSearchQuery,
    paths: &PathPolicy,
    locations: &mut Vec<Value>,
) -> ScopeLeft {
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
) -> ScopeLeft {
    retain(query, paths, items, |item| {
        let caller = item.get("from")?;
        let line = caller
            .get("line")
            .or_else(|| item.pointer("/fromRanges/0/line"))?
            .as_u64()?;
        Some((caller.get("path")?.as_str()?.to_owned(), line))
    })
}

/// Keep the answer gaps (sites the server left out or did not place) whose
/// file and line lie on the side the query's `testCode` keeps; a gap with no
/// line left is dropped.
pub(super) fn retain_gaps(
    query: &LspSearchQuery,
    paths: &PathPolicy,
    gaps: &[super::scope::Gap],
) -> (Vec<super::scope::Gap>, ScopeLeft) {
    let Some(scope) = Scoped::of(lsp_search_query::test_code(query), paths) else {
        return (gaps.to_vec(), ScopeLeft::default());
    };
    let kept = gaps
        .iter()
        .filter_map(|gap| {
            let keep = |lines: &[u32]| {
                lines
                    .iter()
                    .copied()
                    .filter(|line| scope.keeps(Path::new(&gap.file), line.saturating_add(1)))
                    .collect::<Vec<_>>()
            };
            let (left_out, unverified) = (keep(&gap.left_out), keep(&gap.unverified));
            (!left_out.is_empty() || !unverified.is_empty()).then(|| super::scope::Gap {
                file: gap.file.clone(),
                left_out,
                unverified,
            })
        })
        .collect();
    (
        kept,
        ScopeLeft {
            excluded: scope.excluded().stated(),
            unread: scope.unread(),
        },
    )
}

/// State what the scope left out on `row` (`testCodeExcluded`), and mark it
/// partial when a verdict is unproven.
/// A row that already states a count (the sites it lists) adds this one's.
pub(super) fn disclose(row: &mut Value, left: ScopeLeft) {
    if let (Some(excluded), Some(fields)) = (left.excluded, row.as_object_mut()) {
        let stated = fields
            .get("testCodeExcluded")
            .and_then(|stated| serde_json::from_value::<TestCodeExcluded>(stated.clone()).ok())
            .unwrap_or_default();
        let total = TestCodeExcluded {
            files: stated.files.saturating_add(excluded.files),
            lines: stated.lines.saturating_add(excluded.lines),
        };
        fields.insert(
            "testCodeExcluded".into(),
            serde_json::to_value(total).unwrap_or(Value::Null),
        );
    }
    if let Some(note) = octocode_tools_shared::test_code::unread_note(&left.unread) {
        octocode_runtime_base::response::paging::mark_partial(
            row,
            octocode_runtime_base::response::paging::PartialReason::FilesUnevaluated,
            &[note],
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
) -> ScopeLeft {
    let Some(scope) = Scoped::of(lsp_search_query::test_code(query), paths) else {
        return ScopeLeft::default();
    };
    rows.retain(|row| {
        site(row).is_none_or(|(path, line)| {
            scope.keeps(Path::new(&path), octocode_engine::num::saturating_u32(line))
        })
    });
    ScopeLeft {
        excluded: scope.excluded().stated(),
        unread: scope.unread(),
    }
}
