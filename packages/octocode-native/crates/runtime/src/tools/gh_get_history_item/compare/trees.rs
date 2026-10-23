//! A comparison's changed paths past GitHub's compare file cap, from the
//! merge base's and the head's recursive trees.
use crate::tools::gh_get_history_item::HistoryItemRequest;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::util::str_at;
use octocode_github::{GitHubTransport, RequestContext};
use serde_json::{Value, json};

/// The comparison's changed paths past GitHub's compare file list, from the
/// merge base's and the head's recursive trees: every path whose blob (or
/// submodule commit) differs, as provider-shaped entries without patch or
/// line counts, skipping the paths the compare list already holds. `None`
/// when either tree is truncated or unreadable (the cap then stands).
pub(super) async fn tree_changed_files(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    raw: &Value,
    head_sha: &str,
    context: &RequestContext,
) -> Option<Vec<Value>> {
    let merge_base = str_at(raw, "/merge_base_commit/sha")?;
    let request = |reference: &str| octocode_github::TreeRequest {
        owner: gh_get_history_item_query::owner(query).to_owned(),
        repo: gh_get_history_item_query::repo(query).to_owned(),
        reference: reference.to_owned(),
        recursive: true,
    };
    let (base_tree, head_tree) = (request(merge_base), request(head_sha));
    let (base, head) = tokio::join!(
        transport.get_tree(&base_tree, context),
        transport.get_tree(&head_tree, context)
    );
    let (base, head) = (base.ok()?, head.ok()?);
    if base.truncated || head.truncated {
        return None;
    }
    Some(tree_diff(&base.tree, &head.tree, raw))
}

/// The changed entries between two recursive trees that `raw`'s compare
/// file list does not name (by path, or a rename's previous path).
fn tree_diff(
    base: &[octocode_github::TreeEntry],
    head: &[octocode_github::TreeEntry],
    raw: &Value,
) -> Vec<Value> {
    use std::collections::{BTreeMap, HashSet};
    let listed: HashSet<&str> = raw
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|file| {
            [
                str_at(file, "/filename"),
                str_at(file, "/previous_filename"),
            ]
        })
        .flatten()
        .collect();
    let content = |entries: &[octocode_github::TreeEntry]| {
        entries
            .iter()
            .filter(|entry| entry.kind == "blob" || entry.kind == "commit")
            .map(|entry| {
                (
                    entry.path.clone(),
                    (entry.sha.clone().unwrap_or_default(), entry.mode.clone()),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let (base, head) = (content(base), content(head));
    let mut changed = Vec::new();
    for (path, side) in &head {
        let status = match base.get(path) {
            None => "added",
            Some(old) if old != side => "modified",
            Some(_) => continue,
        };
        changed.push((path, status, &side.0));
    }
    for (path, side) in &base {
        if !head.contains_key(path) {
            changed.push((path, "removed", &side.0));
        }
    }
    changed.sort_by(|a, b| a.0.cmp(b.0));
    changed
        .into_iter()
        .filter(|(path, _, _)| !listed.contains(path.as_str()))
        .map(|(path, status, sha)| json!({"filename": path, "status": status, "sha": sha}))
        .collect()
}
