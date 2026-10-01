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
/// Truncates to at most `max` characters (not bytes), appending `...` when
/// shortened. Char-based so multibyte (e.g. CJK) text never splits a code point.
pub(super) fn compact(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.into()
    } else {
        let head: String = value.chars().take(max.saturating_sub(3)).collect();
        format!("{head}...")
    }
}
pub(super) fn map_comments(values: Vec<Value>, kind: &str, include_bots: bool) -> Vec<Value> {
    values.into_iter().filter(|v|include_bots||!is_bot(str_at(v,"/user/login").unwrap_or(""))).map(|v|{
    let mut out=json!({"id":v["id"].to_string().trim_matches('"'),"author":str_at(&v,"/user/login").unwrap_or("unknown"),"body":string(v.get("body")),"createdAt":string(v.get("created_at")),"updatedAt":string(v.get("updated_at")),"commentType":kind,
        "path":v.get("path"),"line":v.get("line").or_else(||v.get("original_line")),"inReplyToId":v.get("in_reply_to_id")});remove_nulls(&mut out);out
}).collect()
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
        crate::contracts::query_schema_max(ToolId::GhGetHistoryItem, None, "charLength"),
    );
    let end = (start + len).min(total);
    let text = value.chars().skip(start).take(end - start).collect();
    (
        text,
        json!({"charOffset":start,"charLength":end-start,"totalChars":total,"hasMore":end<total,"nextCharOffset":(end<total).then_some(end)}),
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

pub(super) fn history_body_view(value: &str, query: &HistoryItemRequest) -> String {
    if minified_view(query) {
        octocode_engine::portable::apply_content_view_minification(value, "history.md")
    } else {
        value.to_owned()
    }
}

/// Window one item body through the body view, remembering the first window
/// that has more text (the surface's body continuation).
pub(super) fn window_body(
    body: &str,
    offset: Option<usize>,
    query: &HistoryItemRequest,
    first_more: &mut Option<Value>,
) -> (String, Value) {
    let view = history_body_view(body, query);
    let (text, page) = paginate_text(&view, offset, query.char_length());
    if page["hasMore"] == true && first_more.is_none() {
        *first_more = Some(page.clone());
    }
    (text, page)
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
        assert_eq!(page["nextCharOffset"], 2);
    }

    #[test]
    fn text_window_honours_the_contract_char_length_maximum() {
        // Contract charLength maximum is 100000; a valid request up to it is
        // served whole rather than silently cut to a smaller native cap.
        let body = "x".repeat(120_000);
        let (value, page) = paginate_text(&body, None, Some(100_000));
        assert_eq!(value.chars().count(), 100_000);
        assert_eq!(page["charLength"], 100_000);
        assert_eq!(page["nextCharOffset"], 100_000);
        let (clamped, _) = paginate_text(&body, None, Some(110_000));
        assert_eq!(clamped.chars().count(), 100_000);
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

    #[test]
    fn compact_truncates_multibyte_text_on_char_boundaries() {
        let body = "修复内存泄漏".repeat(200);
        let out = compact(&body, 500);
        assert!(out.ends_with("..."));
        assert_eq!(out.chars().count(), 500);
        assert_eq!(compact("短", 500), "短");
        assert_eq!(compact("🦀🦀🦀🦀🦀", 4), "🦀...");
    }
}
