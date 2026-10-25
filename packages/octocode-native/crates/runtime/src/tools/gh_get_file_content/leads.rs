//! The runnable next steps of a GitHub file read: its own pages rewritten
//! from the shared localFetch read, and the leads of a match that found
//! nothing.
use super::access;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::{GhGetFileContentQuery, LocalFetchQuery};
use serde_json::{Value, json};

use crate::id::ToolId;
use crate::response::result::{Continuation, query_row};

/// The runnable next steps of a match that found nothing: a ghSearchCode of
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
    let mut commits = json!({"operation": "commit", "owner": owner, "repo": repo, "pathPrefix": access::path(query)});
    // Code search explores the default-branch index; only history honors this ref.
    if let Some(reference) = reference {
        commits["ref"] = json!(reference);
    }
    let pulls =
        json!({"operation": "pullRequest", "owner": owner, "repo": repo, "keywords": [first]});
    vec![
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
    ]
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
    if let Some(force) = access::force_refresh(source) {
        query.insert("forceRefresh".into(), Value::Bool(force));
    }
    Value::Object(query)
}

pub(super) fn rewrite_continuations(
    result: &mut crate::tools::source_view::LocalFetchResult,
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
mod tests {
    use super::*;
    use crate::tools::source_view::{FetchCall, LocalFetchResult, NextCalls, ReadStatus};

    fn read(fields: Value) -> FetchCall {
        let mut row = json!({"path":"README.md"});
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        FetchCall::continued(serde_json::from_value(row).expect("read"))
    }

    #[test]
    fn empty_match_search_uses_the_index_while_history_keeps_the_ref() {
        for reference in ["main", "0123456789abcdef0123456789abcdef01234567"] {
            let source: GhGetFileContentQuery = serde_json::from_value(json!({
                "owner":"o", "repo":"r", "path":"README.md", "ref":reference,
                "matchString":["missing literal"]
            }))
            .expect("query");
            let local: LocalFetchQuery = serde_json::from_value(json!({
                "path":"README.md", "matchString":["missing literal"]
            }))
            .expect("local query");
            let leads = empty_match_leads(&source, &local);
            let code = &leads
                .iter()
                .find(|(name, _)| *name == "findCode")
                .expect("code")
                .1;
            assert_eq!(code["kind"], "related");
            let code_query = &code["input"]["queries"][0];
            assert_eq!(code_query["keywords"], json!(["missing literal"]));
            assert!(code_query.get("ref").is_none(), "{code}");
            let history = &leads
                .iter()
                .find(|(name, _)| *name == "listCommits")
                .expect("history")
                .1;
            assert_eq!(history["input"]["queries"][0]["ref"], reference);
        }
    }

    /// The whole-source recovery of a whole read keeps that scope
    /// on GitHub; range and saved-progress reads retain their bounds, and
    /// every row of a multi-row call becomes a GitHub read.
    #[test]
    fn a_whole_view_lead_keeps_full_content_and_every_row_is_rewritten() {
        let source: GhGetFileContentQuery =
            serde_json::from_value(json!({"owner":"o","repo":"r","path":"README.md"}))
                .expect("query");
        let mut rest = serde_json::Map::new();
        rest.insert(
            "unfilteredUnknown2".into(),
            Continuation::related_input(
                ToolId::LocalFetch,
                json!({"queries":[{"path":"README.md","ranges":[{"startLine": 1, "endLine": 2}]},
                    {"path":"README.md","ranges":[{"startLine": 9, "endLine": 9}]}]}),
            )
            .build(),
        );
        let mut result = LocalFetchResult::blank(ReadStatus::Success);
        result.next = Some(NextCalls {
            unfiltered: Some(read(json!({}))),
            unfiltered_unknown: Some(read(json!({"ranges":[{"startLine": 3, "endLine": 4}]}))),
            continue_window: Some(read(json!({"readProgress":{"offset":50,"unit":"lines"}}))),
            unfiltered_unknown_rest: rest,
            ..NextCalls::default()
        });
        let next = rewrite_continuations(&mut result, &source, "abc123").expect("next");
        let whole = &next["unfiltered"];
        assert_eq!(whole["tool"], "ghGetFileContent");
        assert_eq!(whole["input"]["queries"][0]["view"], "content");
        let window = &next["continueWindow"]["input"]["queries"][0];
        assert_eq!(window["readProgress"]["offset"], 50);
        let rows = next["unfilteredUnknown2"]["input"]["queries"]
            .as_array()
            .expect("rows");
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| row["owner"] == "o" && row["ref"] == "abc123")
        );
    }
}
