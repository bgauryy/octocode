//! Pure-value utility helpers shared across history-item shaping functions.
use super::{HistoryItemRequest, ItemOperation};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use serde_json::{Map, Value, json};

pub(super) fn content_flag(value: Option<&Map<String, Value>>, key: &str) -> bool {
    value.and_then(|v| v.get(key)).and_then(Value::as_bool) == Some(true)
}
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
pub(super) fn map_comments(values: Vec<Value>, kind: &str, include_bots: bool) -> Vec<Value> {
    values.into_iter().filter(|v|include_bots||!is_bot(str_at(v,"/user/login").unwrap_or(""))).map(|v|{
    let mut out=json!({"id":v["id"].to_string().trim_matches('"'),"author":str_at(&v,"/user/login").unwrap_or("unknown"),"body":string(v.get("body")),"createdAt":string(v.get("created_at")),"updatedAt":string(v.get("updated_at")),"commentType":kind,
        "path":v.get("path"),"inReplyToId":v.get("in_reply_to_id").filter(|id|!id.is_null()).map(|id|id.to_string().trim_matches('"').to_owned())});
    review_anchor(&v,&mut out);remove_nulls(&mut out);out
}).collect()
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
pub(super) fn compare_identity(
    raw: &Value,
    requested_base: &str,
    requested_head: &str,
) -> (String, String) {
    let permalink = raw
        .get("permalink_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let pair = permalink
        .rsplit('/')
        .next()
        .and_then(|tail| tail.split_once("..."));
    // The permalink spells both sides `owner:abbrev`; the echo keeps the
    // requested form (an `owner:` prefix only when the caller wrote one) and
    // expands an abbreviation to the full SHA it uniquely names.
    let expand = |requested: &str, parsed: Option<&str>| {
        let candidate = parsed.unwrap_or(requested);
        let abbrev = candidate.rsplit(':').next().unwrap_or(candidate);
        let prefix = requested
            .rsplit_once(':')
            .map_or(String::new(), |(owner, _)| format!("{owner}:"));
        if abbrev.len() == 40 {
            return format!("{prefix}{abbrev}");
        }
        let mut known = Vec::new();
        if let Some(sha) = raw.pointer("/base_commit/sha").and_then(Value::as_str) {
            known.push(sha);
        }
        if let Some(sha) = raw
            .pointer("/merge_base_commit/sha")
            .and_then(Value::as_str)
        {
            known.push(sha);
        }
        for commit in raw
            .get("commits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(sha) = commit.get("sha").and_then(Value::as_str) {
                known.push(sha);
            }
        }
        // base_commit and merge_base_commit are often the same commit.
        known.sort_unstable();
        known.dedup();
        let matches: Vec<_> = known
            .into_iter()
            .filter(|sha| {
                sha.to_ascii_lowercase()
                    .starts_with(&abbrev.to_ascii_lowercase())
            })
            .collect();
        if matches.len() == 1 {
            format!("{prefix}{}", matches[0])
        } else {
            requested.to_owned()
        }
    };
    (
        expand(requested_base, pair.map(|(base, _)| base)),
        expand(requested_head, pair.map(|(_, head)| head)),
    )
}

pub(super) fn is_bot(login: &str) -> bool {
    let v = login.to_ascii_lowercase();
    v.ends_with("[bot]")
        || v == "bot"
        || matches!(
            v.as_str(),
            "vercel"
                | "pkg-pr-new"
                | "coderabbitai"
                | "github-actions"
                | "codecov"
                | "changeset-bot"
                | "netlify"
                | "sonarcloud"
                | "socket-security"
        )
}

pub(super) fn paginate_text(
    value: &str,
    offset: Option<usize>,
    length: Option<usize>,
) -> (String, Value) {
    let total = value.chars().count();
    let start = offset.unwrap_or(0).min(total);
    let len = length.unwrap_or(super::DEFAULT_TEXT_WINDOW).clamp(
        1,
        crate::contracts::query_schema_max(ToolId::GhGetHistoryItem, None, "length"),
    );
    let end = (start + len).min(total);
    let text = value.chars().skip(start).take(end - start).collect();
    (
        text,
        json!({"offset":start,"length":end-start,"totalChars":total,"hasMore":end<total,"nextOffset":(end<total).then_some(end)}),
    )
}

/// Lower-cased `matchString`, the needle every content filter matches.
pub(super) fn needle(query: &HistoryItemRequest) -> Option<String> {
    query.match_string().map(str::to_lowercase)
}

/// Whether an item's `body` contains `needle` (always true without one).
pub(super) fn body_matches(value: &Value, needle: Option<&str>) -> bool {
    needle.is_none_or(|n| string(value.get("body")).to_lowercase().contains(n))
}

/// Pull-request text is minified unless `minify:"none"` or a `matchString`
/// asks for the verbatim text.
pub(super) fn minified_view(query: &HistoryItemRequest) -> bool {
    matches!(query.operation(), ItemOperation::PullRequest)
        && query.minify().as_deref() != Some("none")
        && query.match_string().is_none()
}

/// The text view of a PR/issue body, comment or review. A minified view
/// that only reflows whitespace keeps the source text, so offsets stay
/// GitHub's own; a view that drops text is marked (`bodyView`) and has a raw
/// re-read.
pub(super) fn history_body_view(value: &str, query: &HistoryItemRequest) -> String {
    if !minified_view(query) {
        return value.to_owned();
    }
    let view = octocode_engine::portable::apply_content_view_minification(value, "history.md");
    if view_dropped_text(value, &view) {
        view
    } else {
        value.to_owned()
    }
}

/// Whether a body view dropped text: any non-whitespace character differs
/// from the raw body (whitespace-only normalization keeps every word).
pub(super) fn view_dropped_text(raw: &str, view: &str) -> bool {
    let words = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    raw != view && words(raw) != words(view)
}

/// Window one item body through the body view, remembering the first window
/// that has more text (the surface's body continuation) and whether the view
/// dropped text (`bodyView` + `next.readRawBody`).
pub(super) fn window_body(
    body: &str,
    offset: Option<usize>,
    query: &HistoryItemRequest,
    first_more: &mut Option<Value>,
    dropped: &mut bool,
) -> (String, Value) {
    let view = history_body_view(body, query);
    *dropped |= view_dropped_text(body, &view);
    let (text, page) = paginate_text(&view, offset, query.char_length());
    if page["hasMore"] == true && first_more.is_none() {
        *first_more = Some(page.clone());
    }
    (text, page)
}

/// A commit date in UTC (`…Z`), as every history date is; text that is not
/// a full timestamp passes through.
pub(super) fn utc_date(value: Option<&str>) -> String {
    let value = value.unwrap_or("");
    crate::providers::github::utc_timestamp(value).unwrap_or_else(|| value.to_owned())
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
    use super::*;
    #[test]
    fn text_windows_are_unicode_safe() {
        let (value, page) = paginate_text("a🦀b", Some(1), Some(1));
        assert_eq!(value, "🦀");
        assert_eq!(page["nextOffset"], 2);
    }

    #[test]
    fn text_window_honours_the_contract_char_length_maximum() {
        // Contract length maximum is 100000; a valid request up to it is
        // served whole rather than silently cut to a smaller native cap.
        let body = "x".repeat(120_000);
        let (value, page) = paginate_text(&body, None, Some(100_000));
        assert_eq!(value.chars().count(), 100_000);
        assert_eq!(page["length"], 100_000);
        assert_eq!(page["nextOffset"], 100_000);
        let (clamped, _) = paginate_text(&body, None, Some(110_000));
        assert_eq!(clamped.chars().count(), 100_000);
    }

    /// Whitespace normalization is not a dropped-text view; a removed HTML
    /// comment or badge is.
    #[test]
    fn body_views_that_drop_words_are_detected() {
        assert!(!view_dropped_text("a\r\n\n\n b  ", "a\n\nb"));
        assert!(view_dropped_text("a <!-- hidden --> b", "a b"));
        let raw = "Fixes #1\n<!-- checklist: tests added -->\n[![ci](https://x/badge.svg)](https://x)\nBody";
        let pr = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("pr query");
        let mut first_more = None;
        let mut dropped = false;
        let (text, _) = window_body(raw, None, &pr, &mut first_more, &mut dropped);
        assert!(dropped, "{text}");
        assert!(!text.contains("checklist"), "{text}");
        let raw_pr = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1,
            "minify":"none"
        }))
        .expect("raw pr query");
        let mut dropped = false;
        let (text, _) = window_body(raw, None, &raw_pr, &mut first_more, &mut dropped);
        assert!(!dropped);
        assert_eq!(text, raw);
    }

    /// A view that only reflows whitespace keeps the body verbatim, so body
    /// offsets are GitHub's own character offsets.
    #[test]
    fn whitespace_only_views_keep_the_source_text() {
        let pr = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("pr query");
        let raw = "Intro  \r\n\n\n\nDetails\n\n\n- item\n";
        assert_eq!(history_body_view(raw, &pr), raw);
        let mut first_more = None;
        let mut dropped = false;
        let (text, page) = window_body(raw, None, &pr, &mut first_more, &mut dropped);
        assert_eq!(text, raw);
        assert_eq!(page["totalChars"], raw.chars().count());
        assert!(!dropped);
    }

    #[test]
    fn commit_dates_are_utc() {
        assert_eq!(
            utc_date(Some("2024-01-01T02:30:00+02:00")),
            "2024-01-01T00:30:00Z"
        );
        assert_eq!(
            utc_date(Some("2024-01-01T00:00:00Z")),
            "2024-01-01T00:00:00Z"
        );
        assert_eq!(utc_date(None), "");
    }

    #[test]
    fn filters_bots() {
        assert!(is_bot("ci[bot]"));
        assert!(is_bot("coderabbitai"));
        assert!(!is_bot("robotics"));
    }

    #[test]
    fn compare_identity_expands_permalink_abbreviations() {
        let raw = json!({
            "permalink_url": "https://github.com/a/b/compare/abc1234...def5678",
            "base_commit": {"sha": "abc1234aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
            "commits": [{"sha": "def5678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]
        });
        let (base, head) = compare_identity(&raw, "main", "feature");
        assert!(base.starts_with("abc1234"));
        assert!(head.starts_with("def5678"));
    }

    /// D10: GitHub's permalink spells both sides `owner:abbrev`; the echo
    /// must keep the caller's bare-SHA form on both sides, and a base that is
    /// both base_commit and merge_base_commit still expands.
    #[test]
    fn compare_identity_echoes_the_requested_form_without_owner_prefix() {
        let base_sha = "af20f667fd2536c9502f69d99fe6bdedfcc839cb";
        let head_sha = "c265e3f9413161c900cbf4aa70d451b8e6b3920a";
        let raw = json!({
            "permalink_url": "https://github.com/o/r/compare/o:af20f66...o:c265e3f",
            "base_commit": {"sha": base_sha},
            "merge_base_commit": {"sha": base_sha},
            "commits": [{"sha": head_sha}]
        });
        assert_eq!(
            compare_identity(&raw, base_sha, head_sha),
            (base_sha.to_owned(), head_sha.to_owned())
        );
        assert_eq!(
            compare_identity(&raw, "af20f66", "c265e3f"),
            (base_sha.to_owned(), head_sha.to_owned())
        );
        // A caller-written fork prefix is kept.
        assert_eq!(
            compare_identity(&raw, base_sha, "fork:c265e3f").1,
            format!("fork:{head_sha}")
        );
    }
}
