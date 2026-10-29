use super::{StructureSearchQuery, execute_structure};
use crate::execution::cancel::NeverCancel;
use crate::response::result::ToolError;
use crate::tools::test_support::{
    AfterRoot, Fixture, sensitive_fixture, simple_policy, skipped_count, skipped_names,
    skipped_reasons,
};
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

mod budget;
mod files;
mod ignore;
mod inputs;
mod lines;
mod listing;
mod pages;
mod probes;
mod read_leads;
mod roles;
mod scans;
mod skips;
mod summaries;
mod tree;
mod views;
mod walk_ceiling;

/// Tests speak JSON rows, or a continuation's complete input (its one row);
/// the runtime owns the typed parse.
fn execute_row(
    query: Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
) -> Result<Value, ToolError> {
    let row = query
        .get("queries")
        .map_or(query.clone(), |rows| rows[0].clone());
    let row = crate::tools::test_support::opened(crate::id::ToolId::StructureSearch, &row);
    let query: StructureSearchQuery =
        serde_json::from_value(row).expect("typed structureSearch row");
    execute_structure(&query, paths, security, cancellation, None)
}

fn run_budget(root: &std::path::Path, query: Value, budget: usize) -> Result<Value, ToolError> {
    let (paths, security) = simple_policy(root);
    let row = crate::tools::test_support::opened(crate::id::ToolId::StructureSearch, &query);
    let query: StructureSearchQuery = serde_json::from_value(row).expect("typed row");
    execute_structure(&query, &paths, &security, &NeverCancel, Some(budget))
}

fn related<'a>(out: &'a Value, tool: &str) -> &'a Value {
    out["related"]
        .as_array()
        .and_then(|calls| calls.iter().find(|call| call["tool"] == tool))
        .unwrap_or(&Value::Null)
}

fn paths(out: &Value) -> Vec<String> {
    listed(out)
        .iter()
        .map(|entry| entry["path"].as_str().expect("path").to_owned())
        .collect()
}

fn run(root: &std::path::Path, query: Value) -> Result<Value, ToolError> {
    let (paths, security) = simple_policy(root);
    execute_row(query, &paths, &security, &NeverCancel)
}

/// Runs `start` and every page lead its pages offer (`checks::walk`: each
/// call once, each listed entry once, every page and lead within the
/// contract). Returns the raw pages in walk order.
fn walk_pages(root: &std::path::Path, start: Value) -> Vec<Value> {
    walk_pages_budget(root, start, None)
}

fn walk_pages_budget(root: &std::path::Path, start: Value, budget: Option<usize>) -> Vec<Value> {
    let mut pages = Vec::new();
    crate::tools::test_support::checks::walker::walk(
        "structureSearch",
        json!({"queries": [start]}),
        |call| {
            let input =
                crate::tools::test_support::opened_input(crate::id::ToolId::StructureSearch, call);
            let row = &input["queries"][0];
            let out = budget
                .map_or_else(
                    || run(root, row.clone()),
                    |budget| run_budget(root, row.clone(), budget),
                )
                .unwrap_or_else(|error| panic!("{row}: {error:?}"));
            let page = crate::tools::test_support::checks::Page {
                output: crate::tools::test_support::published(
                    crate::id::ToolId::StructureSearch,
                    &out,
                ),
                rows: listed(&out)
                    .iter()
                    .map(|entry| entry["path"].as_str().expect("path").to_owned())
                    .collect(),
            };
            pages.push(out);
            page
        },
    );
    pages
}

/// `files` rows expanded, one object per entry: `path` (`data.path` +
/// group `dir` + name, `.` naming the directory itself; bare entries are
/// `path`'s own), `type` for directories and symlinks, and the entry's
/// `size`, `totalLines` and `modified` (as `modifiedMs`) fields.
fn listed(out: &Value) -> Vec<Value> {
    let root = out["path"].as_str().expect("path").to_owned();
    // Group dirs are workspace-relative; fixtures list the workspace
    // root itself (`.`) or a direct child of it (named like `root`).
    let join = |dir: &str| {
        if dir.is_empty() || dir == "." {
            root.clone()
        } else if dir == root || dir.starts_with(&format!("{root}/")) {
            dir.to_owned()
        } else {
            format!("{root}/{dir}")
        }
    };
    out["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files groups: {out}"))
        .iter()
        .flat_map(|group| match group.as_str() {
            Some(entry) => vec![listed_entry(&root, entry)],
            None => {
                let dir = join(group["dir"].as_str().expect("dir"));
                group["files"]
                    .as_array()
                    .expect("group entries")
                    .iter()
                    .map(|entry| listed_entry(&dir, entry.as_str().expect("entry text")))
                    .collect()
            }
        })
        .collect()
}

fn listed_entry(dir: &str, entry: &str) -> Value {
    let (name, fields) = match entry.strip_suffix(')').and_then(|e| e.rsplit_once(" (")) {
        Some((name, fields)) => (name, fields.split(", ").collect::<Vec<_>>()),
        None => (entry, Vec::new()),
    };
    let mut row = json!({});
    let name = match name.strip_suffix('/') {
        Some(name) => {
            row["type"] = json!("directory");
            name
        }
        None => name,
    };
    row["path"] = json!(match (dir, name) {
        (dir, ".") => dir.to_owned(),
        ("", name) => name.to_owned(),
        (dir, name) => format!("{dir}/{name}"),
    });
    for field in fields {
        if field == "symlink" {
            row["type"] = json!("symlink");
        } else if matches!(field, "binary" | "unreadable") {
            row["totalLines"] = json!(field);
        } else if let Some(lines) = field.strip_prefix("totalLines=") {
            row["totalLines"] = json!(lines.parse::<u64>().expect("totalLines"));
        } else if let Some(modified) = field.strip_prefix("modified=") {
            row["modifiedMs"] =
                json!(crate::civil_date::rfc3339_millis(modified).expect("modified"));
        } else {
            row["size"] = json!(field.parse::<i64>().expect("size"));
        }
    }
    row
}
