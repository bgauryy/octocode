//! Transport-neutral structured result metadata and lossless path compaction.
use serde_json::{Map, Value, json};
use std::path::Path;

pub fn result_row(
    tool: &str,
    index: usize,
    query: &Value,
    mut data: Value,
    status: Option<&str>,
) -> Value {
    if let Some(object) = data.as_object_mut() {
        for key in [
            "status",
            "cache",
            "goal",
            "reasoning",
            "researchSuggestions",
            "query",
        ] {
            object.remove(key);
        }
        if status != Some("error") {
            object.remove("error");
        }
    }
    let kind = evidence_kind(tool, query, &data);
    let reported = data.get("confidence").and_then(Value::as_str);
    let confidence = if status == Some("error") || reported == Some("low") {
        "low"
    } else if let Some(value @ ("medium" | "high")) = reported {
        value
    } else if matches!(kind, "provider" | "lexical" | "syntactic") {
        "medium"
    } else {
        "high"
    };
    let partial = is_partial(&data);
    let mut codes = Vec::new();
    if let Some(code) = data.get("errorCode").and_then(Value::as_str) {
        codes.push(code.to_owned());
    }
    codes.extend(pagination_codes(&data));
    let mut meta = json!({"evidence":{"kind":kind,"confidence":confidence}});
    if partial || !codes.is_empty() {
        let mut diagnostics = Map::new();
        if !codes.is_empty() {
            diagnostics.insert("codes".into(), json!(codes));
        }
        if partial {
            diagnostics.insert("partial".into(), json!(true));
        }
        meta["diagnostics"] = Value::Object(diagnostics);
    }
    let mut row = json!({"index":index,"meta":meta,"data":data});
    if let Some(status) = status {
        row["status"] = json!(status);
    }
    row
}

fn evidence_kind<'a>(tool: &'a str, query: &Value, data: &Value) -> &'a str {
    match tool {
        "astSearch" => match query["operation"].as_str() {
            Some("files") => "exact",
            Some("tree") if query["treeKind"] != "syntax" => "exact",
            Some("match") => "structural",
            _ => "syntactic",
        },
        "lspSearch" => match data.pointer("/lsp/source").and_then(Value::as_str) {
            Some("native" | "native-graph-facts" | "markdown") => "syntactic",
            _ => "semantic",
        },
        "localSearch" => "lexical",
        "artifactSearch" => "provider",
        name if name.starts_with("gh") => "provider",
        _ => "exact",
    }
}

fn tree_some(value: &Value, predicate: &impl Fn(&Map<String, Value>) -> bool) -> bool {
    match value {
        Value::Object(map) => predicate(map) || map.values().any(|v| tree_some(v, predicate)),
        Value::Array(array) => array.iter().any(|v| tree_some(v, predicate)),
        _ => false,
    }
}

fn bounded(record: &Map<String, Value>) -> bool {
    [
        "truncated",
        "capReached",
        "capped",
        "capturesTruncated",
        "totalMatchesCapped",
        "incompleteResults",
        "incompleteTree",
        "possiblyTruncated",
        "truncatedByDepth",
        "truncatedByBudget",
    ]
    .iter()
    .any(|key| record.get(*key) == Some(&Value::Bool(true)))
        || record.get("complete") == Some(&Value::Bool(false))
        || record
            .get("partialTreeFailures")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
}

pub fn is_partial(data: &Value) -> bool {
    tree_some(data, &|map| {
        map.get("isPartial") == Some(&Value::Bool(true))
            || map.get("hasMore") == Some(&Value::Bool(true))
            || bounded(map)
    })
}

fn continuation(value: &Value, key_matches: &impl Fn(&str) -> bool) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            (key_matches(&key.to_lowercase())
                && child.get("tool").is_some_and(Value::is_string)
                && child
                    .get("query")
                    .is_some_and(|v| v.is_object() || v.is_array()))
                || continuation(child, key_matches)
        }),
        Value::Array(array) => array.iter().any(|v| continuation(v, key_matches)),
        _ => false,
    }
}

fn pagination_codes(data: &Value) -> Vec<String> {
    if !is_partial(data) {
        return vec![];
    }
    if tree_some(data, &|m| {
        m.get("terminalLimit") == Some(&Value::Bool(true))
    }) {
        return vec!["terminalLimitReached".into()];
    }
    let pageable = tree_some(data, &|m| m.get("hasMore") == Some(&Value::Bool(true)));
    let partial = tree_some(data, &|m| m.get("isPartial") == Some(&Value::Bool(true)));
    let page = continuation(data, &|k| {
        k.starts_with("next") || k.starts_with("continue")
    });
    let expansion = continuation(data, &|k| {
        [
            "expand", "retry", "restart", "narrow", "fallback", "escalate", "read",
        ]
        .iter()
        .any(|p| k.starts_with(p))
            || k.contains("search")
            || k.contains("completeness")
    });
    if (pageable && !page) || ((tree_some(data, &bounded) || partial) && !page && !expansion) {
        vec!["continuationMissing".into()]
    } else {
        vec![]
    }
}

pub fn envelope(mut rows: Vec<Value>) -> Value {
    let mut paths = Vec::new();
    for row in &rows {
        visit_paths(&row["data"], 0, &mut paths);
    }
    let base = common_directory(&paths);
    if let Some(base) = &base {
        for row in &mut rows {
            rewrite_paths(&mut row["data"], 0, base);
        }
    }
    let mut value = json!({"results":rows});
    if let Some(base) = base {
        value["base"] = json!(base);
    }
    value
}

fn absolute_path(map: &Map<String, Value>) -> Option<String> {
    if let Some(path) = map
        .get("absolutePath")
        .and_then(Value::as_str)
        .filter(|p| Path::new(p).is_absolute())
    {
        return Some(path.into());
    }
    if let Some(uri) = map
        .get("uri")
        .and_then(Value::as_str)
        .filter(|p| p.starts_with("file://"))
    {
        return url::Url::parse(uri)
            .ok()?
            .to_file_path()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
    }
    map.get("path")
        .and_then(Value::as_str)
        .filter(|p| Path::new(p).is_absolute())
        .map(str::to_owned)
}

fn visit_paths(value: &Value, depth: usize, paths: &mut Vec<String>) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(map) => {
            if let Some(path) = absolute_path(map) {
                paths.push(path);
            }
            for (key, child) in map {
                if !matches!(key.as_str(), "next" | "location") {
                    visit_paths(child, depth + 1, paths);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                visit_paths(child, depth + 1, paths);
            }
        }
        _ => {}
    }
}

fn common_directory(paths: &[String]) -> Option<String> {
    let first = paths.first()?;
    let mut length = first.len();
    for path in paths.iter().skip(1) {
        length = first
            .bytes()
            .zip(path.bytes())
            .take(length)
            .take_while(|(a, b)| a == b)
            .count();
    }
    // The common prefix may end inside a UTF-8 character; search bytes for '/'.
    let slash = first.as_bytes()[..length]
        .iter()
        .rposition(|b| *b == b'/')?;
    (slash > 1).then(|| first[..slash].into())
}

fn rewrite_paths(value: &mut Value, depth: usize, base: &str) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(map) => {
            if let Some(path) = absolute_path(map)
                .and_then(|p| p.strip_prefix(&format!("{base}/")).map(str::to_owned))
            {
                map.insert("path".into(), json!(path));
                map.remove("absolutePath");
                map.remove("uri");
            }
            for (key, child) in map {
                if !matches!(key.as_str(), "next" | "location") {
                    rewrite_paths(child, depth + 1, base);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                rewrite_paths(child, depth + 1, base);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_compaction_never_rewrites_evidence_or_executable_queries() {
        let data = json!({"path":"/repo/src/a.ts","content":"/repo/src/a.ts","pagination":{"hasMore":true},"next":{"continue":{"tool":"localFetch","query":{"path":"/repo/src/a.ts","offset":2}}}});
        let output = envelope(vec![result_row("localFetch", 0, &json!({}), data, None)]);
        assert_eq!(output["base"], "/repo/src");
        assert_eq!(output["results"][0]["data"]["path"], "a.ts");
        assert_eq!(output["results"][0]["data"]["content"], "/repo/src/a.ts");
        assert_eq!(
            output.pointer("/results/0/data/next/continue/query/path"),
            Some(&json!("/repo/src/a.ts"))
        );
        assert_eq!(
            output.pointer("/results/0/meta/diagnostics"),
            Some(&json!({"partial":true}))
        );
    }
    #[test]
    fn incomplete_evidence_requires_executable_continuation_or_terminal_diagnostic() {
        let missing = result_row(
            "astSearch",
            0,
            &json!({"operation":"symbols"}),
            json!({"hasMore":true,"nextPage":2}),
            None,
        );
        assert_eq!(
            missing.pointer("/meta/diagnostics/codes"),
            Some(&json!(["continuationMissing"]))
        );
        let terminal = result_row(
            "localFetch",
            0,
            &json!({}),
            json!({"isPartial":true,"terminalLimit":true}),
            None,
        );
        assert_eq!(
            terminal.pointer("/meta/diagnostics/codes"),
            Some(&json!(["terminalLimitReached"]))
        );
    }
}
