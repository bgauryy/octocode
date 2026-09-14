//! Transport-neutral structured result metadata and lossless path compaction.
use serde_json::{Map, Value, json};
use std::path::Path;

pub(super) fn attach_diagnostics(
    row: &mut Value,
    diagnostics: crate::tools::result::ToolDiagnostics,
) {
    if diagnostics.codes.is_empty() && diagnostics.hints.is_empty() && !diagnostics.partial {
        return;
    }
    for (field, values) in [("codes", diagnostics.codes), ("hints", diagnostics.hints)] {
        if values.is_empty() {
            continue;
        }
        let target = &mut row["meta"]["diagnostics"][field];
        if !target.is_array() {
            *target = json!([]);
        }
        if let Some(existing) = target.as_array_mut() {
            for value in values {
                let value = Value::String(value);
                if !existing.contains(&value) {
                    existing.push(value);
                }
            }
        }
    }
    if diagnostics.partial {
        row["meta"]["diagnostics"]["partial"] = json!(true);
    }
}

const MAX_GUIDANCE_CHARS: usize = 120;

const ADVISORY_CALLS: &[&str] = &[
    "fetch",
    "getLines",
    "readSite",
    "viewDeeper",
    "viewStructure",
    "viewTree",
    "searchCode",
    "searchRepositoryCode",
    "cloneRepo",
    "cloneForSemantics",
    "lspDefinition",
    "lspReferences",
    "readIssue",
    "prDetail",
];

const METADATA_CONTAINERS: &[&str] = &[
    "data",
    "meta",
    "diagnostics",
    "error",
    "files",
    "directories",
    "results",
    "packages",
    "entries",
    "items",
];

fn concise(value: &str) -> String {
    let mut text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() && !matches!(text.chars().last(), Some('.' | '!' | '?' | '…')) {
        text.push('.');
    }
    if text.chars().count() <= MAX_GUIDANCE_CHARS {
        return text;
    }
    let prefix: String = text.chars().take(MAX_GUIDANCE_CHARS - 1).collect();
    let boundary = prefix.rfind(' ').unwrap_or(0);
    let cut = if boundary > 60 {
        boundary
    } else {
        MAX_GUIDANCE_CHARS - 1
    };
    format!("{}…", prefix.chars().take(cut).collect::<String>())
}

fn record(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

fn record_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
    value.as_object_mut()
}

fn has_executable_call(value: &Value) -> bool {
    let Some(call) = record(value) else {
        return false;
    };
    call.get("tool").and_then(Value::as_str).is_some()
        && call.get("query").and_then(record).is_some()
}

fn has_recovery(value: &Value) -> bool {
    if let Some(values) = value.as_array() {
        return values.iter().any(has_recovery);
    }
    let Some(node) = record(value) else {
        return false;
    };
    if node
        .get("hints")
        .and_then(Value::as_array)
        .is_some_and(|hints| {
            hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|text| !text.trim().is_empty()))
        })
    {
        return true;
    }
    if node
        .get("next")
        .and_then(record)
        .is_some_and(|next| next.values().any(has_executable_call))
    {
        return true;
    }
    for (key, child) in node {
        if METADATA_CONTAINERS.contains(&key.as_str()) && has_recovery(child) {
            return true;
        }
        if key == "repositories"
            && child
                .as_object()
                .is_some_and(|repos| repos.values().any(has_recovery))
        {
            return true;
        }
    }
    false
}

fn fallback_hint(tool: &str, query: &Value) -> Option<&'static str> {
    match tool {
        "ghSearch" if query["operation"] == "tree" => {
            Some("Verify owner/repo/branch, or broaden path/depth.")
        }
        "ghSearch" => Some("Broaden keywords or remove filters."),
        "ghGetFileContent" => Some("Verify owner/repo/branch/path, or remove matchString."),
        "ghSearchHistory" => Some("Broaden keywords or remove history filters."),
        "ghGetHistoryItem" => Some("Verify owner/repo and the number, ref, or compare refs."),
        "artifactSearch" => Some("Check packageName, or broaden keywords."),
        "ghCloneRepo" => Some("Verify owner/repo/branch and sparsePath."),
        "localSearch" => Some("Broaden searchText, path, or filters."),
        "astSearch"
            if matches!(query["operation"].as_str(), Some("files"))
                || query["treeKind"] == "filesystem" =>
        {
            Some("Broaden path or file filters.")
        }
        "astSearch" if query["operation"] == "topology" => {
            Some("Inspect diagnostics, then broaden the graph scope if needed.")
        }
        "astSearch" => Some("Broaden the syntax/name query, path, or filters."),
        "astRewrite" if query["apply"] == true => {
            Some("Preview again and copy every current beforeHash before applying.")
        }
        "astRewrite" => Some("Broaden the structural pattern, path, or file filters."),
        "localFetch" => Some("Verify path/range, or remove matchString."),
        "lspSearch" => Some("Refresh uri/symbolName/lineHint, or broaden workspaceRoot."),
        _ => None,
    }
}

fn add_fallback_hint(row: &mut Value, position: usize, tool: &str, queries: &[Value]) {
    if row.get("status").and_then(Value::as_str) != Some("empty") {
        return;
    }
    if has_recovery(row) {
        return;
    }
    let index = row
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(position as u64) as usize;
    let query = queries.get(index).cloned().unwrap_or(Value::Null);
    let Some(hint) = fallback_hint(tool, &query) else {
        return;
    };
    let Some(data) = row.get_mut("data").and_then(record_mut) else {
        return;
    };
    data.insert("hints".into(), json!([hint]));
}

fn shape_next(next: &mut Value, recovery: bool) {
    let Some(object) = record_mut(next) else {
        return;
    };
    let keys: Vec<String> = object.keys().cloned().collect();
    for key in keys {
        let Some(call) = object.get(&key).and_then(record) else {
            continue;
        };
        if call.get("tool").and_then(Value::as_str).is_none()
            || call.get("query").and_then(record).is_none()
        {
            continue;
        }
        let advisory = key
            .split(':')
            .next()
            .is_some_and(|name| ADVISORY_CALLS.contains(&name));
        if !recovery && advisory {
            object.remove(&key);
            continue;
        }
        if let Some(call) = object.get_mut(&key).and_then(record_mut) {
            if recovery {
                if let Some(why) = call.get("why").and_then(Value::as_str) {
                    let short = concise(why);
                    call.insert("why".into(), json!(short));
                }
            } else {
                call.remove("why");
            }
        }
    }
}

fn actionable(hint: &str) -> bool {
    const WORDS: &[&str] = &[
        "broaden", "check", "choose", "correct", "disable", "enable", "pass", "provide", "refresh",
        "remove", "retry", "run", "select", "set", "specify", "supply", "try", "use", "verify",
        "wait",
    ];
    hint.split(|character: char| !character.is_ascii_alphabetic())
        .any(|part| WORDS.iter().any(|word| part.eq_ignore_ascii_case(word)))
}

fn visit(value: &mut Value, recovery: bool, seen: &mut std::collections::BTreeSet<String>) {
    if let Some(values) = value.as_array_mut() {
        for child in values {
            visit(child, recovery, seen);
        }
        return;
    }
    let Some(node) = record_mut(value) else {
        return;
    };
    let needs_help = matches!(
        node.get("status").and_then(Value::as_str),
        Some("error" | "empty")
    ) || recovery;
    if node.contains_key("next")
        && let Some(next) = node.get_mut("next")
    {
        shape_next(next, needs_help);
    }
    let keys: Vec<String> = node.keys().cloned().collect();
    for key in keys {
        if METADATA_CONTAINERS.contains(&key.as_str())
            && let Some(child) = node.get_mut(&key)
        {
            visit(child, needs_help, seen);
        }
        if key == "repositories"
            && let Some(repos) = node.get_mut("repositories").and_then(record_mut)
        {
            let names: Vec<String> = repos.keys().cloned().collect();
            for name in names {
                if let Some(repo) = repos.get_mut(&name) {
                    visit(repo, needs_help, seen);
                }
            }
        }
    }
    if node.contains_key("hints") {
        let mut hints = Vec::new();
        if needs_help && let Some(candidates) = node.get("hints").and_then(Value::as_array) {
            let mut candidates: Vec<String> = candidates
                .iter()
                .filter_map(Value::as_str)
                .map(concise)
                .filter(|hint| !hint.is_empty())
                .collect();
            candidates.sort_by_key(|hint| std::cmp::Reverse(actionable(hint)));
            for short in candidates {
                if seen.contains(&short) || !seen.is_empty() {
                    continue;
                }
                seen.insert(short.clone());
                hints.push(short);
            }
        }
        if hints.is_empty() {
            node.remove("hints");
        } else {
            node.insert("hints".into(), json!(hints));
        }
    }
}

/// Match the frozen Node hint policy: one concise recovery hint, and `why`
/// only on recovery continuations.
pub(super) fn apply_hint_policy(rows: &mut [Value], tool: &str, queries: &[Value]) {
    for (index, row) in rows.iter_mut().enumerate() {
        add_fallback_hint(row, index, tool, queries);
        visit(row, false, &mut std::collections::BTreeSet::new());
    }
}

/// Sanitize every string field, including provider diagnostics and metadata.
/// Domain content scans alone cannot protect errors returned by a remote server.
pub(super) fn sanitize_fields(
    value: &mut Value,
    security: &crate::security::ContentSecurity,
    context: &super::ExecutionContext,
) -> Result<(), super::ExecutionError> {
    context.check()?;
    match value {
        Value::String(text) => *text = security.sanitize_text(text, None).content,
        Value::Array(values) => {
            for value in values {
                sanitize_fields(value, security, context)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                sanitize_fields(value, security, context)?;
            }
        }
        _ => {}
    }
    Ok(())
}

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
    if tool == "ghGetFileContent" {
        for file in data
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(code) = file.get("errorCode").and_then(Value::as_str)
                && !codes.iter().any(|existing| existing == code)
            {
                codes.push(code.to_owned());
            }
        }
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
    let shared = hoist_shared_fields(&mut rows);
    let mut value = json!({"results":rows});
    if let Some(base) = base {
        value["base"] = json!(base);
    }
    if let Some(shared) = shared {
        value["shared"] = Value::Object(shared);
    }
    value
}

fn hoist_shared_fields(rows: &mut [Value]) -> Option<Map<String, Value>> {
    const EXCLUDED: &[&str] = &[
        "path",
        "uri",
        "absolutePath",
        "owner",
        "repo",
        "name",
        "id",
        "type",
        "kind",
        "reason",
        "isPartial",
        "startLine",
        "endLine",
        "start",
        "end",
        "startColumn",
        "endColumn",
        "startByte",
        "endByte",
        "line",
        "column",
        "character",
        "parentId",
        "parent",
        "named",
        "exported",
    ];
    let leaves: Vec<&Map<String, Value>> = rows
        .iter()
        .filter_map(|row| row["data"].as_object())
        .flat_map(|data| data.values().filter_map(Value::as_array))
        .flatten()
        .filter_map(Value::as_object)
        .collect();
    if leaves.len() < 2 {
        return None;
    }
    let shared: Map<String, Value> = leaves[0]
        .iter()
        .filter(|(key, value)| {
            !EXCLUDED.contains(&key.as_str())
                && (value.is_number()
                    || value.is_boolean()
                    || value.as_str().is_some_and(|s| !s.is_empty()))
                && leaves.iter().all(|leaf| leaf.get(*key) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if shared.is_empty() {
        return None;
    }
    for leaf in rows
        .iter_mut()
        .filter_map(|row| row["data"].as_object_mut())
        .flat_map(|data| data.values_mut().filter_map(Value::as_array_mut))
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        for key in shared.keys() {
            leaf.remove(key);
        }
    }
    Some(shared)
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
