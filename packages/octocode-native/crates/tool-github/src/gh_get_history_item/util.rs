//! JSON-value helpers shared across history-item shaping: typed accessors,
//! the shallow merge, and the provider comment mapper.
use octocode_runtime_base::response::cut::{clean_cut, is_clean_cut};
use octocode_runtime_base::response::result::remove_nulls;
use octocode_tools_shared::gh_shared::dates::utc_date;
use serde_json::{Value, json};

pub(super) fn array(value: Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
pub(super) fn string(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or("").to_owned()
}
pub(super) fn str_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}
pub(super) fn usize_at(value: &Value, pointer: &str) -> usize {
    octocode_engine::num::saturating_usize(
        value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0),
    )
}
pub(super) fn nonzero(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64).filter(|v| *v > 0)
}
/// Provider comments (discussion or inline review) as comment rows, in
/// GitHub's order, a bot's marked `bot`; an inline review comment carries
/// its `path` and the review it belongs to (`reviewId`, which the review
/// shaping keeps only where it folds that review into its comments). An
/// unedited comment states one timestamp: `updatedAt` only when it differs
/// from `createdAt`.
pub(super) fn map_comments(values: Vec<Value>) -> Vec<Value> {
    values
        .into_iter()
        .map(|v| {
            let created = utc_date(str_at(&v, "/created_at"));
            let updated = str_at(&v, "/updated_at")
                .map(|at| utc_date(Some(at)))
                .filter(|at| !at.is_empty() && *at != created);
            let mut out = json!({
                "id": v["id"].to_string().trim_matches('"'),
                "author": str_at(&v, "/user/login").unwrap_or("unknown"),
                "bot": is_bot(&v).then_some(true),
                "body": string(v.get("body")),
                "createdAt": created,
                "updatedAt": updated,
                "path": v.get("path"),
                "inReplyToId": v
                    .get("in_reply_to_id")
                    .filter(|id| !id.is_null())
                    .map(|id| id.to_string().trim_matches('"').to_owned()),
                "reviewId": v
                    .get("pull_request_review_id")
                    .filter(|id| !id.is_null())
                    .map(|id| id.to_string().trim_matches('"').to_owned()),
            });
            review_anchor(&v, &mut out);
            remove_nulls(&mut out);
            out
        })
        .collect()
}

/// Whether a GitHub App or bot account wrote a provider comment: GitHub
/// types its user `Bot`, and an app's login ends in `[bot]`.
fn is_bot(comment: &Value) -> bool {
    str_at(comment, "/user/type") == Some("Bot")
        || str_at(comment, "/user/login").is_some_and(|login| login.ends_with("[bot]"))
}

/// The code a review comment points at, readable with ghGetFileContent at
/// `commitSha`: the live `line`/`start_line`/`commit_id`, else (an outdated
/// comment, `line: null`) the `original_*` triple flagged `outdated`. A
/// file-level comment has no line.
fn review_anchor(raw: &Value, out: &mut Value) {
    let present = |key: &str| raw.get(key).filter(|value| !value.is_null()).cloned();
    let (line, start, commit, outdated) = match (present("line"), present("original_line")) {
        (Some(line), _) => (
            Some(line),
            present("start_line"),
            present("commit_id"),
            false,
        ),
        (None, Some(line)) => (
            Some(line),
            present("original_start_line"),
            present("original_commit_id"),
            true,
        ),
        (None, None) => (None, None, present("commit_id"), false),
    };
    out["line"] = line.clone().unwrap_or(Value::Null);
    out["startLine"] = start
        .filter(|start| Some(start) != line.as_ref())
        .unwrap_or(Value::Null);
    out["commitSha"] = commit.unwrap_or(Value::Null);
    if outdated {
        out["outdated"] = Value::Bool(true);
    }
    if raw.get("line").is_some() || raw.get("original_line").is_some() {
        out["side"] = present("side").unwrap_or(Value::Null);
    }
    if str_at(raw, "/subject_type") == Some("file") {
        out["subjectType"] = json!("file");
    }
}
/// Shallow object merge: `right`'s keys overwrite `left`'s.
pub(super) fn merge(mut left: Value, right: Value) -> Value {
    if let (Some(l), Some(r)) = (left.as_object_mut(), right.as_object()) {
        l.extend(r.clone());
    }
    left
}

/// Where a window of `view` that starts at character `from` and would end
/// at character `to` really ends: the shared [`clean_cut`] (never inside a
/// redaction marker or a CRLF pair), or, when that falls on or before the
/// window's start (the marker starts the window), the first clean cut past
/// `to`, so the marker stays whole and a walk advances.
pub(in crate::gh_get_history_item) fn clean_char_cut(view: &str, from: usize, to: usize) -> usize {
    let byte_of = |chars: usize| {
        view.char_indices()
            .nth(chars)
            .map_or(view.len(), |(at, _)| at)
    };
    let mut at = clean_cut(view, byte_of(to));
    if at <= byte_of(from) {
        at = byte_of(to);
        while !is_clean_cut(view, at) {
            at += 1;
        }
    }
    view.char_indices()
        .take_while(|(byte, _)| *byte < at)
        .count()
}

#[cfg(test)]
mod tests;
