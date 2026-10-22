//! Row helpers the astSearch behavior tests share: run a JSON row, read
//! match and outline rows, follow page chains.
use super::{AstResult, AstSearchQuery, execute_ast};
use crate::execution::cancel::NeverCancel;
use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::tools::test_support::{checks, simple_policy};
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use serde_json::json;

/// Tests speak JSON rows, or a continuation's complete input (its one row);
/// the runtime owns the typed parse.
pub(super) fn execute_row(
    query: serde_json::Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
) -> AstResult {
    let row = query
        .get("queries")
        .map_or(query.clone(), |rows| rows[0].clone());
    let query: AstSearchQuery = serde_json::from_value(row).expect("typed astSearch row");
    execute_ast(&query, paths, security, cancellation, None)
}

/// A match row `{line, column, endLine?, value}` as (line, endLine, value).
pub(super) fn lean_row(row: &serde_json::Value) -> (u64, Option<u64>, &str) {
    (
        row["line"].as_u64().expect("match row line"),
        row["endLine"].as_u64(),
        row["value"].as_str().expect("match row value"),
    )
}

/// The declaration names of an outline row (a declaration object and its
/// nested members), in source order.
pub(super) fn outline_row_names(row: &serde_json::Value) -> Vec<&str> {
    let mut names = vec![row["symbolName"].as_str().expect("declaration row")];
    for member in row["members"].as_array().into_iter().flatten() {
        names.extend(outline_row_names(member));
    }
    names
}

/// Every expansion was followed: at most the read of the hits remains.
pub(super) fn only_read_left(out: &serde_json::Value) -> bool {
    out["next"]
        .as_object()
        .is_none_or(|next| next.keys().all(|name| name == "read"))
}

/// The first declaration name of an outline row.
pub(super) fn outline_name(row: &serde_json::Value) -> &str {
    outline_row_names(row)[0]
}

pub(super) fn outline_names(rows: &serde_json::Value) -> Vec<&str> {
    rows.as_array()
        .expect("outline rows")
        .iter()
        .flat_map(outline_row_names)
        .collect()
}

pub(super) fn run(
    root: &std::path::Path,
    query: serde_json::Value,
) -> Result<serde_json::Value, ToolError> {
    let (paths, security) = simple_policy(root);
    execute_row(query, &paths, &security, &NeverCancel)
}

/// A row run under `paths`, the result a successful run returns.
pub(super) fn run_row(
    query: &serde_json::Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
) -> serde_json::Value {
    execute_row(query.clone(), paths, security, &NeverCancel).expect("astSearch")
}

/// The localFetch read rows of the one `expandValues` lead, by path. The
/// lead has one name (never `expandValues2`…) and batches its rows within
/// localFetch's input limit.
pub(super) fn value_reads(out: &serde_json::Value) -> Vec<serde_json::Value> {
    let names = out["next"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name.starts_with("expandValues"))
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert!(
        names.len() <= 1 && names.iter().all(|name| *name == "expandValues"),
        "{out}"
    );
    let read = &out["next"]["expandValues"];
    if read.is_null() {
        return vec![];
    }
    assert_eq!(read["tool"], "localFetch", "{read}");
    let rows = read["query"]["queries"].as_array().expect("rows").clone();
    assert!(
        rows.len() <= crate::response::result::rows_per_call(crate::id::ToolId::LocalFetch),
        "{read}"
    );
    for row in &rows {
        crate::contracts::validate_query("localFetch", row.clone())
            .expect("a valid localFetch row");
    }
    let mut reads = rows;
    reads.sort_by_key(|row| row["path"].as_str().map(str::to_owned));
    reads
}

/// The snapshot of a 4-row symbols page of `source`.
pub(super) fn out_snapshot(root: &std::path::Path, source: &std::path::Path) -> serde_json::Value {
    let first = run(
        root,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":4}),
    )
    .expect("first page");
    first["snapshot"].clone()
}

/// Walks `query` and every page lead of each page (`checks::walk`: each
/// call once, each row once, every output and lead within the contract).
/// A page offers one continuation (the next page, else the scan resume), and
/// the last is not cut short. Returns every page, in walk order.
pub(super) fn ast_chain(
    root: &std::path::Path,
    query: serde_json::Value,
) -> Vec<serde_json::Value> {
    let mut pages = Vec::new();
    checks::walk("astSearch", json!({ "queries": [query] }), |call| {
        let row = &call["queries"][0];
        let out = run(root, row.clone()).expect("page");
        let page = out["next"].get("nextPage");
        let expand = out["next"].get("expandScan");
        assert!(
            page.is_none() || expand.is_none(),
            "one continuation per page: {out}"
        );
        if page.is_none() && expand.is_none() {
            assert!(out.get("terminalLimit").is_none(), "{out}");
        }
        let walked = checks::Page {
            output: crate::tools::test_support::published(ToolId::AstSearch, row, &out),
            rows: ast_rows(std::slice::from_ref(&out)),
        };
        pages.push(out);
        walked
    });
    pages
}

/// Every `(file, row)` a page lists: match rows under `files[].matches`,
/// symbol rows under `files[].symbols`.
pub(super) fn ast_rows(pages: &[serde_json::Value]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|out| out["files"].as_array().cloned().unwrap_or_default())
        .flat_map(|file| {
            let path = file["path"].as_str().expect("path").to_owned();
            let rows = file
                .get("matches")
                .or_else(|| file.get("symbols"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            rows.into_iter().map(move |row| format!("{path} {row}"))
        })
        .collect()
}
