//! The runnable next steps of a GitHub file read: its own pages rewritten
//! from the shared localFetch read, and the leads of a match that found
//! nothing.
use super::access;
use octocode_contract_types::{GhGetFileContentQuery, LocalFetchQuery};
pub use octocode_tools_shared::source_view::local_fetch_query;
use serde_json::{Value, json};

use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, query_row};

/// The runnable next steps of a match that found nothing: the file's symbols
/// outline when its language has one (the name may be a declaration the
/// literal spells differently), a ghSearchCode of
/// the repository for the literal (it may live in another file), and the
/// file's history, since the text may live only on a branch or pull
/// request: the commits that changed the path, and pull requests whose
/// text names the literal (approximate: their discussion, not their
/// diffs). A regex has no literal to search.
pub(super) fn empty_match_leads(
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
) -> Vec<(&'static str, Value)> {
    let literals = local_fetch_query::match_strings(local);
    let Some(first) = literals.first().map(|first| first.trim()) else {
        return Vec::new();
    };
    if local_fetch_query::is_regex(local) || first.is_empty() {
        return Vec::new();
    }
    let (owner, repo) = (access::owner(query), access::repo(query));
    let reference = access::reference(query);
    let code = json!({"owner": owner, "repo": repo, "keywords": [first]});
    let mut commits =
        json!({"operation": "commit", "owner": owner, "repo": repo, "path": access::path(query)});
    // Code search explores the default-branch index; only history honors this ref.
    if let Some(reference) = reference {
        commits["ref"] = json!(reference);
    }
    let pulls =
        json!({"operation": "pullRequest", "owner": owner, "repo": repo, "keywords": [first]});
    let mut leads = Vec::new();
    if has_outline(access::path(query)) {
        let mut symbols = json!({
            "owner": owner, "repo": repo, "path": access::path(query), "view": "symbols"
        });
        if let Some(reference) = reference {
            symbols["ref"] = json!(reference);
        }
        leads.push((
            "readSymbols",
            Continuation::related(ToolId::GhGetFileContent, symbols).build(),
        ));
    }
    leads.extend([
        (
            "findCode",
            Continuation::related(ToolId::GhSearchCode, code).build(),
        ),
        (
            "listCommits",
            Continuation::related(ToolId::GhSearchHistory, commits).build(),
        ),
        (
            "findPullRequest",
            Continuation::related(ToolId::GhSearchHistory, pulls).build(),
        ),
    ]);
    leads
}

/// Whether the file's language (by extension) has a symbols outline.
fn has_outline(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            octocode_engine::signatures::languages::find_entry(&extension.to_ascii_lowercase())
                .is_some()
        })
}

/// The runnable repair of a `ranges` read whose every range starts past the
/// file end: the same read of the file's last window, as wide as the widest
/// requested range, at the read commit. `None` when any range reaches into
/// the file (that read is served, clamped) or the file has no line.
pub(super) fn last_window_repair(
    query: &GhGetFileContentQuery,
    local: &LocalFetchQuery,
    total_lines: usize,
    sha: &str,
) -> Option<Value> {
    let (start, end) = local_fetch_query::last_window_span(local, total_lines)?;
    let mut row = query_row(query, &[]);
    row["ref"] = json!(sha);
    row["ranges"] = json!([{"startLine":start,"endLine":end}]);
    Some(Continuation::related(ToolId::GhGetFileContent, row).build())
}

/// One localFetch continuation row as the same read of the GitHub file at
/// `resolved_ref`, preserving the shared reader's canonical view, ranges,
/// matcher and saved `readProgress`. A whole-source recovery keeps the
/// corresponding whole-source reader variant.
fn github_row(
    mut query: serde_json::Map<String, Value>,
    source: &GhGetFileContentQuery,
    resolved_ref: &str,
) -> Value {
    query.insert(
        "owner".into(),
        Value::String(access::owner(source).to_owned()),
    );
    query.insert(
        "repo".into(),
        Value::String(access::repo(source).to_owned()),
    );
    query.insert("ref".into(), Value::String(resolved_ref.to_owned()));
    query.remove("snapshot");
    Value::Object(query)
}

pub(super) fn rewrite_continuations(
    result: &mut octocode_tools_shared::source_view::LocalFetchResult,
    source: &GhGetFileContentQuery,
    resolved_ref: &str,
) -> Option<Value> {
    let mut value = serde_json::to_value(result.next.take()?).ok()?;
    let Value::Object(object) = &mut value else {
        return None;
    };
    // Each localFetch call becomes the same read of the GitHub file. A lead
    // to another local tool (the miss's `findText` localSearch of the
    // file's directory) has no GitHub form here and is dropped; the GitHub
    // miss adds its own repository-level lead.
    object.retain(|_, continuation| continuation["tool"] == ToolId::LocalFetch.as_str());
    for continuation in object.values_mut() {
        let Some(rows) = continuation
            .pointer("/input/queries")
            .and_then(Value::as_array)
        else {
            continue;
        };
        let rows = rows
            .iter()
            .filter_map(Value::as_object)
            .map(|row| github_row(row.clone(), source, resolved_ref))
            .collect::<Vec<_>>();
        let call = if continuation["kind"] == "continue" {
            Continuation::continued_input(ToolId::GhGetFileContent, json!({"queries": rows}))
        } else {
            Continuation::related_input(ToolId::GhGetFileContent, json!({"queries": rows}))
        };
        *continuation = call.build();
    }
    (!object.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests;
