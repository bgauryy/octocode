//! JSON-value helpers shared across history-item shaping: typed accessors,
//! the shallow merge, and the provider comment mapper.
use crate::response::result::remove_nulls;
use crate::tools::gh_shared::dates::utc_date;
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
    crate::num::usize_of(value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0))
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

/// A page's comment rows with human comments first, each group in its
/// own order: bot comments (release and preview bots) often fill most of
/// a discussion page, and every row stays on the page, `bot` marked, its
/// `createdAt` keeping the timeline.
pub(super) fn humans_first(rows: &mut [Value]) {
    rows.sort_by_key(|row| row["bot"] == true);
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

    /// Release and preview bots (changeset-bot, pkg-pr-new) can fill a
    /// discussion page: their comments stay on it, marked `bot`, after the
    /// human ones, each group in GitHub's order.
    #[test]
    fn a_page_lists_human_comments_before_marked_bot_comments() {
        let comment = |id: u64, login: &str, kind: &str| {
            serde_json::json!({"id": id, "user": {"login": login, "type": kind},
                "body": "b", "created_at": "2026-06-03T00:00:00Z"})
        };
        let mut rows = super::map_comments(vec![
            comment(1, "changeset-bot[bot]", "Bot"),
            comment(2, "ada", "User"),
            comment(3, "pkg-pr-new", "Bot"),
            comment(4, "bob", "User"),
            comment(5, "renovate[bot]", "User"),
        ]);
        super::humans_first(&mut rows);
        let order = rows
            .iter()
            .map(|row| (row["id"].as_str().unwrap_or(""), row.get("bot").is_some()))
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            [
                ("2", false),
                ("4", false),
                ("1", true),
                ("3", true),
                ("5", true)
            ]
        );
        assert_eq!(rows[2]["bot"], true);
    }
}
