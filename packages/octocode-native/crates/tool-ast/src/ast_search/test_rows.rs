//! Row helpers the astSearch behavior tests share: run a JSON row, read
//! match and outline rows, follow page chains.
use super::{AstSearchQuery, execute_ast};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::test_support::{checks, simple_policy};
use octocode_runtime_base::{
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
        octocode_runtime_base::test_support::opened_input(ToolId::AstSearch, &query)["queries"][0]
            .clone()
    } else {
        query
    };
    let row = octocode_runtime_base::test_support::opened(ToolId::AstSearch, &row);
    let query: AstSearchQuery = serde_json::from_value(row).expect("typed astSearch row");
    execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        paths,
        security,
        cancellation,
        None,
    )
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
        octocode_runtime_base::test_support::opened_input(ToolId::AstSearch, &row)["queries"][0]
            .clone()
    } else {
        row
    };
    let row = octocode_runtime_base::test_support::opened(ToolId::AstSearch, &row);
    let query = serde_json::from_value(row).expect("canonical AST row");
    execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &paths,
        &security,
        &NeverCancel,
        Some(window),
    )
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
        let input = octocode_runtime_base::test_support::opened_input(ToolId::AstSearch, call);
        let row = &input["queries"][0];
        let out = run(root, row.clone()).expect("page");
        assert_no_repeated_continuation(&out);
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
            output: octocode_runtime_base::test_support::published(ToolId::AstSearch, &out),
            rows: ast_rows(std::slice::from_ref(&out)),
        };
        pages.push(out);
        walked
    });
    pages
}

/// One required call is offered on one channel of a response: no continuation
/// call appears twice in it.
pub(super) fn assert_no_repeated_continuation(out: &Value) {
    let mut seen = std::collections::BTreeSet::new();
    for lead in octocode_runtime_base::test_support::checks::leads(out)
        .into_iter()
        .filter(|lead| lead.kind == "continue")
    {
        assert!(
            seen.insert(lead.query.to_string()),
            "{} {} repeats a call in one response: {out}",
            lead.channel,
            lead.name
        );
    }
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

/// Exact UTF-8 fixture correspondence, including the retained snapshot and descriptor closure.
pub(super) fn assert_copied_row(
    output: &Value,
    row: &Value,
    mut expected: Value,
    range: std::ops::Range<usize>,
    header: Option<std::ops::Range<usize>>,
) {
    let catalog = &output["sourceEvidence"];
    let unit_id = row["sourceOrigin"]["analysisUnitId"]
        .as_str()
        .expect("unit identity");
    let unit = catalog["units"]
        .as_array()
        .expect("units")
        .iter()
        .find(|unit| unit["id"] == unit_id)
        .expect("closed unit");
    let representation = catalog["representations"]
        .as_array()
        .expect("representations")
        .iter()
        .find(|representation| representation["id"] == unit["representationId"])
        .expect("closed representation");
    assert_eq!(
        representation["sourceIds"]
            .as_array()
            .expect("sources")
            .len(),
        1
    );
    let source_id = &representation["sourceIds"][0];
    let source = catalog["snapshots"]
        .as_array()
        .expect("snapshots")
        .iter()
        .find(|source| source["id"] == *source_id)
        .expect("closed snapshot");
    let raw = std::fs::read(source["path"].as_str().expect("source path")).expect("fixture bytes");
    let digest = octocode_engine::digest::sha256(&raw);
    assert_eq!(source["digest"], digest);
    assert_eq!(source["encoding"], "utf8");
    assert_eq!(representation["sourceDigest"], digest);
    let origin = |range: &std::ops::Range<usize>| {
        let span = json!({"startByte":range.start,"endByte":range.end});
        (
            span.clone(),
            json!({"analysisUnitId":unit_id,"segments":[{
                "sourceRange":span,"analysis":"included","branches":[{
                    "kind":"copied","origin":{"sourceId":source_id,"rawRange":span}
                }]
            }]}),
        )
    };
    let (span, projected) = origin(&range);
    expected["sourceRange"] = span;
    expected["sourceOrigin"] = projected;
    expected["sourceDigest"] = json!(digest);
    if let Some(header) = header {
        assert!(range.start <= header.start && header.end <= range.end);
        let (span, projected) = origin(&header);
        expected["headerSourceRange"] = span;
        expected["headerOrigin"] = projected;
    }
    assert_eq!(*row, expected);
}

/// A row's word on what its walk skipped: the one shared sentence
/// for exactly `causes`, which names the `skippedPaths` ledger and the
/// `related[]` request, beside the one lead that sets each cause's flag.
pub(super) fn assert_unsearched(
    out: &serde_json::Value,
    causes: &[octocode_tools_shared::empty_walk::Cause],
) {
    let said = octocode_tools_shared::leads::skipped::unsearched(causes)
        .expect("a walk that skipped paths")
        .text;
    for named in ["skippedPaths", "related[]"] {
        assert!(said.contains(named), "{named}: {said}");
    }
    // One channel by the row's status: an empty row's recovery hint, a
    // warning on a row with evidence.
    let (channel, other) = if out["status"] == "empty" {
        ("hints", "warnings")
    } else {
        ("warnings", "hints")
    };
    let stated = |channel: &str| {
        out[channel]
            .as_array()
            .into_iter()
            .flatten()
            .any(|text| text.as_str() == Some(said.as_str()))
    };
    assert!(stated(channel), "{channel} lacks {said}: {out}");
    assert!(!stated(other), "{other} repeats {said}: {out}");
    let call = skipped_call(out).unwrap_or_else(|| panic!("missing skipped-scope lead: {out}"));
    let lead = &call["input"]["queries"][0];
    for cause in causes {
        let (field, value) = cause.flag();
        assert_eq!(lead[field], value, "{field}: {out}");
    }
}

/// The fields that qualify an empty or partial match answer: the inferred
/// grammar and the counts of the grammars and formats the rule did not
/// search.
pub(super) const QUALIFIERS: [&str; 3] = ["inferredLanguage", "skippedGrammars", "skippedFormats"];

/// A complete answer with matches states none of [`QUALIFIERS`]: its
/// matches name their files, and what it did not search did not change it.
pub(super) fn assert_unqualified(out: &serde_json::Value) {
    assert!(out["matchCount"].as_u64() > Some(0), "{out}");
    assert!(out.get("isPartial").is_none(), "{out}");
    for field in QUALIFIERS {
        assert!(out.get(field).is_none(), "{field}: {out}");
    }
}
