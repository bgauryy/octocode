use super::*;

/// Test query from wire JSON over a base query (or a minimal valid one);
/// `null` removes a field so its contract default applies.
fn ls_query(fields: serde_json::Value, base: Option<&LocalSearchQuery>) -> LocalSearchQuery {
    let mut value = base.map_or_else(
            || serde_json::json!({"path": "_", "matchString": "_", "mainGoal": "test", "reasoning": "test"}),
            |base| serde_json::to_value(base).expect("query serializes"),
        );
    let object = value.as_object_mut().expect("query object");
    for (key, field) in fields.as_object().expect("fields object") {
        if field.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), field.clone());
        }
    }
    serde_json::from_value(value).expect("valid localSearch query")
}
use crate::{execution::cancel::NeverCancel, policy::path::PathPolicy, security::ContentSecurity};
use std::fs;

/// Replace `path`'s bytes with `content` of the same length and restore
/// its nanosecond modification time, so size and time cannot reveal the
/// edit.
fn rewrite_keeping_stamp(path: &std::path::Path, content: &str) {
    let before = fs::metadata(path).expect("fixture metadata");
    assert_eq!(before.len(), content.len() as u64, "same-size rewrite");
    let modified = before.modified().expect("mtime");
    fs::write(path, content).expect("rewrite");
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(modified))
        .expect("restore mtime");
    let after = fs::metadata(path).expect("fixture metadata");
    assert_eq!(
        (after.len(), after.modified().expect("mtime")),
        (before.len(), modified)
    );
}

/// Settle the fixture writes so far (on this thread's settle clock): a scan is
/// stored for its later pages only when nothing its walk searched was
/// written within it (see `manifest`).
fn settle() {
    octocode_engine::stamp::settle_written_so_far();
}

/// A continuation of `request` cut from the stored scan it took before
/// `between` ran. The stored walk's proof held when the page took the scan,
/// so `between` changes the tree in the window no stamp closes: after the
/// proof, before the page reads or shows a file. What the page checks then
/// (each read and shown file's read policy and stored digest) is all that
/// stands between the change and the page.
fn stored_page_after(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
    response_window: Option<usize>,
    between: impl FnOnce(),
) -> Result<LocalSearchResult, crate::response::result::ToolError> {
    let scan = super::executor::scan_query(request, policy, &NeverCancel, None, response_window)
        .expect("scan");
    assert!(scan.digests.is_some(), "the page took the stored scan");
    between();
    super::executor::page_of(request, policy, security, &NeverCancel, None, scan, None)
}

fn values(result: &LocalSearchResult) -> String {
    serde_json::to_string(&result.files).expect("files serialize")
}

fn search_fixture(files: &[(&str, &str)], request: LocalSearchQuery) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture directory");
    for (name, body) in files {
        let path = root.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture dir");
        }
        fs::write(path, body).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
        Some(&request),
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    serde_json::to_value(&result).expect("serialize")
}

/// The declaration row `index` of `file` sits in: its own `enclosing`, or
/// the nearest earlier row whose `enclosing.line` is its `enclosingLine`.
fn owner_of(file: &serde_json::Value, index: usize) -> Option<serde_json::Value> {
    let rows = file["matches"].as_array().expect("rows");
    if let Some(own) = rows[index].get("enclosing") {
        return Some(own.clone());
    }
    let line = rows[index].get("enclosingLine")?;
    let owner = rows[..index]
        .iter()
        .rev()
        .filter_map(|row| row.get("enclosing"))
        .find(|enclosing| enclosing["line"] == *line);
    Some(
        owner
            .unwrap_or_else(|| panic!("row {index} refs line {line} no earlier row names: {file}"))
            .clone(),
    )
}

/// Search `root` with `fields` (path set to `root`), default security and
/// no cancellation.
fn search_root(root: &std::path::Path, fields: serde_json::Value) -> LocalSearchResult {
    let (policy, security) = policy_for(root);
    let mut fields = fields;
    fields["path"] = root.to_string_lossy().into_owned().into();
    let request = ls_query(fields, None);
    execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search")
}

/// Run a structureSearch row (a lead's query) and return its JSON text.
fn list_structure(
    mut query: serde_json::Value,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> String {
    query["mainGoal"] = serde_json::json!("test");
    query["reasoning"] = serde_json::json!("test");
    let query: crate::tools::structure_search::StructureSearchQuery =
        serde_json::from_value(query).expect("structureSearch query");
    crate::tools::structure_search::execute_structure(&query, policy, security, &NeverCancel, None)
        .expect("listing runs")
        .to_string()
}

fn policy_for(root: &std::path::Path) -> (PathPolicy, ContentSecurity) {
    let policy = crate::tools::test_support::workspace_policy(root);
    (policy, ContentSecurity::new())
}

/// Run every row of the `expandValues` read of a search result and return
/// how many rows ran and the concatenated content they fetched.
fn follow_expansions(result: &LocalSearchResult, policy: &PathPolicy) -> (usize, String) {
    let next = result.next.clone().unwrap_or_default();
    let mut reads = 0;
    let mut content = String::new();
    for (name, read) in next.as_object().into_iter().flatten() {
        if !name.starts_with("expandValues") {
            continue;
        }
        assert_eq!(read["tool"], "localFetch", "{read}");
        for query in read["query"]["queries"].as_array().into_iter().flatten() {
            reads += 1;
            let mut query = query.clone();
            query["mainGoal"] = serde_json::json!("test");
            query["reasoning"] = serde_json::json!("test");
            let query: crate::tools::source_view::LocalFetchQuery =
                serde_json::from_value(query).expect("localFetch query");
            let fetched = crate::tools::local_fetch::execute_local_fetch(
                &query,
                policy,
                &ContentSecurity::new(),
                &NeverCancel,
                None,
                None,
            );
            assert_eq!(fetched.error, None, "{read}");
            content.push_str(fetched.content.as_deref().unwrap_or(""));
        }
    }
    (reads, content)
}

/// Runs `start` (a localSearch query row) in the workspace `root`, and every
/// page lead its pages offer, under `checks::walk`: each call once, each row
/// once, every page and its leads within the contract. Rows are `path:line`
/// (`path` on list views). Returns the walk and each page's result, in walk
/// order.
fn walk_search(
    root: &std::path::Path,
    start: serde_json::Value,
    window: Option<usize>,
) -> (
    crate::tools::test_support::checks::Walk,
    Vec<serde_json::Value>,
) {
    let (policy, security) = policy_for(root);
    let mut bodies = Vec::new();
    let walk = crate::tools::test_support::checks::walk(
        "localSearch",
        serde_json::json!({"queries": [start]}),
        |call| {
            let row = &call["queries"][0];
            let query: LocalSearchQuery =
                serde_json::from_value(row.clone()).expect("localSearch row");
            let result =
                execute_local_search(&query, &policy, &security, &NeverCancel, None, window, None)
                    .unwrap_or_else(|error| panic!("{row}: {error:?}"));
            let body = serde_json::to_value(&result).expect("result");
            let page = crate::tools::test_support::checks::Page {
                output: crate::tools::test_support::published(
                    crate::id::ToolId::LocalSearch,
                    row,
                    &body,
                ),
                rows: row_ids(&body),
            };
            bodies.push(body);
            page
        },
    );
    (walk, bodies)
}

/// Each shown row of a page as `path:line`, or `path` for a file without
/// rows.
fn row_ids(body: &serde_json::Value) -> Vec<String> {
    let mut ids = Vec::new();
    for file in body["files"].as_array().into_iter().flatten() {
        let path = file["path"].as_str().expect("path");
        match file["matches"].as_array() {
            Some(rows) => ids.extend(rows.iter().map(|row| format!("{path}:{}", row["line"]))),
            None => ids.push(path.to_owned()),
        }
    }
    ids
}

mod binaries;
mod empty_pages;
mod match_list;
mod pages;
mod scope;
mod secrets;
#[cfg(unix)]
mod special_files;
mod stored;

fn skipped_source_path(root: &std::path::Path, path: &str) -> serde_json::Value {
    serde_json::json!(root.canonicalize().expect("canonical fixture").join(path))
}

/// Deterministic selection backend for pagination and snapshot tests.
fn select_all_search(
    query: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
    cancel: &NeverCancel,
    walk_threads: Option<u32>,
    response_window: Option<usize>,
) -> Result<LocalSearchResult, crate::response::result::ToolError> {
    use crate::tools::test_support::selection_fake;
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let selection = selection_fake::runtime(
        selection_fake::SelectAll,
        executor.handle().clone(),
        std::time::Duration::from_secs(5),
    );
    execute_local_search(
        query,
        policy,
        security,
        cancel,
        walk_threads,
        response_window,
        Some(&selection),
    )
}

/// A generated Rust file of `functions` functions with five `let ` lines
/// each: an engine-like hot file whose hits all sit inside declarations.
fn let_heavy_rust(prefix: &str, functions: usize) -> String {
    (0..functions)
        .map(|n| {
            format!(
                "pub fn {prefix}_{n:03}(input: &Request) -> Result<Response, Error> {{\n    let header = input.header();\n    let body = parse_body(&header)?;\n    let validated = validate(body)?;\n    let mapped = map_fields(&validated);\n    let response = Response::new(mapped);\n    Ok(response)\n}}\n\n"
            )
        })
        .collect()
}
