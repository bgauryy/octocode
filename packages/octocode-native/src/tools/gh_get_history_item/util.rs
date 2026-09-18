//! Pure-value utility helpers shared across history-item shaping functions.
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
pub(super) fn compact(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.into()
    } else {
        format!("{}...", &value[..max - 3])
    }
}
pub(super) fn map_comments(values: Vec<Value>, kind: &str, include_bots: bool) -> Vec<Value> {
    values.into_iter().filter(|v|include_bots||!is_bot(str_at(v,"/user/login").unwrap_or(""))).map(|v|{
    let mut out=json!({"id":v["id"].to_string().trim_matches('"'),"author":str_at(&v,"/user/login").unwrap_or("unknown"),"body":string(v.get("body")),"createdAt":string(v.get("created_at")),"updatedAt":string(v.get("updated_at")),"commentType":kind,
        "path":v.get("path"),"line":v.get("line").or_else(||v.get("original_line")),"inReplyToId":v.get("in_reply_to_id")});super::remove_nulls(&mut out);out
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
    let expand = |requested: &str, parsed: Option<&str>| {
        let candidate = parsed.unwrap_or(requested);
        let abbrev = candidate.rsplit(':').next().unwrap_or(candidate);
        if abbrev.len() == 40 {
            return candidate.to_owned();
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
        let matches: Vec<_> = known
            .into_iter()
            .filter(|sha| {
                sha.to_ascii_lowercase()
                    .starts_with(&abbrev.to_ascii_lowercase())
            })
            .collect();
        if matches.len() == 1 {
            let prefix = &candidate[..candidate.len().saturating_sub(abbrev.len())];
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
    let len = length
        .unwrap_or(super::DEFAULT_TEXT_WINDOW)
        .clamp(1, 50_000);
    let end = (start + len).min(total);
    let text = value.chars().skip(start).take(end - start).collect();
    (
        text,
        json!({"charOffset":start,"charLength":end-start,"totalChars":total,"hasMore":end<total,"nextCharOffset":(end<total).then_some(end)}),
    )
}
