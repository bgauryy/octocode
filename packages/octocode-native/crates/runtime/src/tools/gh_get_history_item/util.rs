//! JSON-value helpers shared across history-item shaping: typed accessors,
//! the shallow merge, and the provider comment mapper.
use crate::response::result::remove_nulls;
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
    value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0) as usize
}
pub(super) fn nonzero(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64).filter(|v| *v > 0)
}
/// Provider comments (discussion or inline review) as comment rows; an
/// inline review comment carries its `path`. An unedited comment states one
/// timestamp: `updatedAt` only when it differs from `createdAt`.
pub(super) fn map_comments(values: Vec<Value>) -> Vec<Value> {
    values
        .into_iter()
        .map(|v| {
            let created = string(v.get("created_at"));
            let updated = v
                .get("updated_at")
                .and_then(Value::as_str)
                .filter(|at| !at.is_empty() && *at != created);
            let mut out = json!({
                "id": v["id"].to_string().trim_matches('"'),
                "author": str_at(&v, "/user/login").unwrap_or("unknown"),
                "body": string(v.get("body")),
                "createdAt": created,
                "updatedAt": updated,
                "path": v.get("path"),
                "inReplyToId": v
                    .get("in_reply_to_id")
                    .filter(|id| !id.is_null())
                    .map(|id| id.to_string().trim_matches('"').to_owned()),
            });
            review_anchor(&v, &mut out);
            remove_nulls(&mut out);
            out
        })
        .collect()
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

#[cfg(test)]
mod tests {
    /// A comment without an edit time (GraphQL discussion comments) carries
    /// no `updatedAt`, so the latest comment time falls back to createdAt.
    #[test]
    fn comments_without_an_edit_time_carry_no_empty_updated_at() {
        let rows = super::map_comments(vec![
            serde_json::json!({"id": 1, "user": {"login": "dev"}, "body": "b",
                "created_at": "2026-06-03T00:00:00Z", "updated_at": null}),
        ]);
        assert!(rows[0].get("updatedAt").is_none(), "{}", rows[0]);
        assert_eq!(rows[0]["createdAt"], "2026-06-03T00:00:00Z");
    }
}
