//! Result rows: provider items mapped to the published row shapes.
use crate::providers::github::{ProviderError, ProviderErrorKind, utc_timestamp};
use crate::security::scan::ContentScan;
use crate::tools::result::remove_null_fields;
use serde_json::{Value, json};
use std::path::Path;

/// Redacts secrets in the free text of every provider item (titles, bodies,
/// commit messages) before any row is built from it.
pub(super) fn sanitize_items(
    items: &mut [Value],
    security: &impl ContentScan,
) -> Result<(), ProviderError> {
    let clean = |text: &str| {
        security
            .sanitize(text, Path::new("github-history"))
            .map(|(text, _)| text)
            .map_err(|(message, _)| ProviderError::new(ProviderErrorKind::Validation, message))
    };
    for item in items {
        for key in ["title", "body"] {
            if let Some(text) = item.get(key).and_then(Value::as_str) {
                item[key] = json!(clean(text)?);
            }
        }
        if let Some(text) = item.pointer("/commit/message").and_then(Value::as_str) {
            item["commit"]["message"] = json!(clean(text)?);
        }
    }
    Ok(())
}

/// A provider timestamp in UTC `Z` form; a value that is not a timestamp
/// stays as sent.
fn utc(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => json!(utc_timestamp(text).unwrap_or_else(|| text.clone())),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn labels(v: &Value) -> Vec<String> {
    v.get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|x| x.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

pub(super) fn map_pr(v: Value) -> Value {
    let merged_at = v
        .get("merged_at")
        .filter(|value| !value.is_null())
        .or_else(|| {
            v.pointer("/pull_request/merged_at")
                .filter(|value| !value.is_null())
        });
    let state = if merged_at.is_some() {
        json!("merged")
    } else {
        v["state"].clone()
    };
    let mut row = json!({
        "number":v["number"],
        "title":v.get("title").and_then(Value::as_str).unwrap_or(""),
        "state":state,
        "mergedAt":utc(merged_at),
        "author":v.pointer("/user/login").and_then(Value::as_str).unwrap_or(""),
        "createdAt":utc(v.get("created_at")),
    });
    if row["createdAt"].is_null() {
        row["createdAt"] = json!("");
    }
    let labels = labels(&v);
    if !labels.is_empty() {
        row["labels"] = json!(labels);
    }
    if let Some(count) = v.get("comments").and_then(Value::as_u64).filter(|n| *n > 0) {
        row["commentsCount"] = json!(count);
    }
    remove_null_fields(&mut row);
    row
}

pub(super) fn concise_row(v: &Value) -> Value {
    json!(format!(
        "#{} {}",
        v.get("number").and_then(Value::as_u64).unwrap_or(0),
        v.get("title").and_then(Value::as_str).unwrap_or("")
    ))
}

/// An issue row: `updatedAt` only when the rows are sorted by it; labels
/// only when present.
pub(super) fn map_issue(v: Value, by_update: bool) -> Value {
    let mut row = json!({"number":v["number"],"title":v.get("title"),"state":v.get("state"),"author":v.pointer("/user/login"),"createdAt":utc(v.get("created_at"))});
    let labels = labels(&v);
    if !labels.is_empty() {
        row["labels"] = json!(labels);
    }
    if by_update {
        row["updatedAt"] = utc(v.get("updated_at"));
    }
    remove_null_fields(&mut row);
    row
}

/// A commit person as one string: the GitHub login, else the git name
/// (emails stay out of default rows).
fn person(v: &Value, kind: &str) -> Value {
    v.pointer(&format!("/{kind}/login"))
        .and_then(Value::as_str)
        .or_else(|| {
            v.pointer(&format!("/commit/{kind}/name"))
                .and_then(Value::as_str)
        })
        .map_or(Value::Null, |name| json!(name))
}

fn headline(v: &Value) -> &str {
    v.pointer("/commit/message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
}

/// A commit-search row. No per-row html_url: it is owner/repo/commit/sha,
/// all already in the row and its envelope (issue and PR rows omit it too).
pub(super) fn map_commit(v: Value) -> Value {
    let mut row = json!({"sha":v["sha"],"messageHeadline":headline(&v),"date":utc(v.pointer("/commit/author/date")),"author":person(&v, "author")});
    remove_null_fields(&mut row);
    row
}

/// A commit-list row: headline, date and the author's login; the full
/// message (and emails) come from the commit read. The committer appears
/// only when it is a different person (not the GitHub web-flow bot).
pub(super) fn map_commit_list(v: Value) -> Value {
    let author = person(&v, "author");
    let committer = person(&v, "committer");
    let same = committer == author
        || committer == "web-flow"
        || v.pointer("/committer/login").and_then(Value::as_str) == Some("web-flow")
        || v.pointer("/commit/committer/name") == v.pointer("/commit/author/name");
    let mut row = json!({
        "sha": v["sha"],
        "date": utc(v.pointer("/commit/author/date")),
        "messageHeadline": headline(&v),
        "author": author,
    });
    if !same {
        row["committer"] = committer;
    }
    remove_null_fields(&mut row);
    row
}

/// The row a default read targets: the first readable row `prefer` picks
/// (a merged PR, a completed issue), else the first readable row (an index
/// into `items`).
pub(super) fn read_target(items: &[Value], prefer: fn(&Value) -> bool) -> Option<usize> {
    let readable = |index: &usize| item_number(&items[*index]).is_some();
    (0..items.len())
        .filter(readable)
        .find(|index| prefer(&items[*index]))
        .or_else(|| (0..items.len()).find(readable))
}

pub(super) fn item_number(item: &Value) -> Option<u64> {
    item.get("number").and_then(Value::as_u64)
}

/// `owner`/`repo` of a search item, from its `repository_url`
/// (`…/repos/{owner}/{repo}`).
pub(super) fn item_repo(item: &Value) -> Option<(String, String)> {
    let url = item.get("repository_url").and_then(Value::as_str)?;
    let (owner, repo) = url.rsplit_once("/repos/")?.1.split_once('/')?;
    (!owner.is_empty() && !repo.is_empty() && !repo.contains('/'))
        .then(|| (owner.to_owned(), repo.to_owned()))
}

pub(super) fn is_merged(item: &Value) -> bool {
    item.get("merged_at")
        .or_else(|| item.pointer("/pull_request/merged_at"))
        .is_some_and(|value| !value.is_null())
}

pub(super) fn is_completed(item: &Value) -> bool {
    item.get("state").and_then(Value::as_str) == Some("closed")
        && item.get("state_reason").and_then(Value::as_str) == Some("completed")
}
