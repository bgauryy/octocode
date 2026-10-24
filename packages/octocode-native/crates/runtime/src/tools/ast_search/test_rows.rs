//! Row helpers the astSearch behavior tests share: run a JSON row, read
//! match and outline rows, follow page chains.
use super::{AstSearchQuery, execute_ast};
use crate::execution::cancel::NeverCancel;
use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::tools::test_support::{checks, simple_policy};
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use serde_json::{Value, json};

/// Tests speak JSON rows, or a continuation's complete input (its one row);
/// the runtime owns the typed parse.
pub(super) fn execute_row(
    query: serde_json::Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    let row = if query.get("queries").is_some() || query.get("cursor").is_some() {
        crate::tools::test_support::opened_input(ToolId::AstSearch, &query)["queries"][0].clone()
    } else {
        query
    };
    let row = crate::tools::test_support::opened(ToolId::AstSearch, &row);
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

/// Run a canonical row with a controlled runtime response budget.
pub(super) fn run_window(
    root: &std::path::Path,
    row: Value,
    window: usize,
) -> Result<Value, ToolError> {
    let (paths, security) = simple_policy(root);
    let row = if row.get("queries").is_some() || row.get("cursor").is_some() {
        crate::tools::test_support::opened_input(ToolId::AstSearch, &row)["queries"][0].clone()
    } else {
        row
    };
    let row = crate::tools::test_support::opened(ToolId::AstSearch, &row);
    let query = serde_json::from_value(row).expect("canonical AST row");
    execute_ast(&query, &paths, &security, &NeverCancel, Some(window))
}

/// The localFetch read rows of the one `expandValues` lead, by path. The
/// lead has one name (never `expandValues2`…) and batches its rows within
/// localFetch's input limit.
pub(super) fn value_reads(out: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    for (name, read) in out["next"].as_object().into_iter().flatten() {
        if !name.starts_with("expandValues") {
            continue;
        }
        assert_eq!(read["tool"], "localFetch", "{read}");
        assert_eq!(read["kind"], "continue", "{read}");
        let batch = read["input"]["queries"].as_array().expect("rows");
        assert!(batch.len() <= crate::response::result::rows_per_call(ToolId::LocalFetch));
        for row in batch {
            crate::contracts::validate_built_query("localFetch", row.clone())
                .expect("valid reader row");
            rows.push(row.clone());
        }
    }
    let mut reads = rows;
    reads.sort_by_key(|row| row["path"].as_str().map(str::to_owned));
    reads
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
    checks::walker::walk("astSearch", json!({"queries":[query]}), |call| {
        let input = crate::tools::test_support::opened_input(ToolId::AstSearch, call);
        let row = &input["queries"][0];
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

pub(super) fn skipped_call(out: &serde_json::Value) -> Option<&serde_json::Value> {
    out["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| {
            let row = &call["input"]["queries"][0];
            call["kind"] == "related"
                && call["tool"] == "astSearch"
                && (row["includeHidden"] == true
                    || row["includeIgnored"] == true
                    || row["defaultExcludes"] == false)
        })
}
