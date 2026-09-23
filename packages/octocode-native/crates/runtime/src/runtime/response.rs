//! Transport-neutral structured result metadata and lossless path compaction.
use serde_json::{Map, Value, json};
use std::path::Path;

pub(super) fn attach_diagnostics(
    row: &mut Value,
    diagnostics: crate::tools::result::ToolDiagnostics,
) {
    if row.get("meta").is_none()
        || (diagnostics.codes.is_empty() && diagnostics.hints.is_empty() && !diagnostics.partial)
    {
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
        "clasify" => Some("Inspect resources, typed questions, and OCTOCODE_CLASSIFICATION_API."),
        "ghCloneRepo" => Some("Verify owner/repo/branch and sparsePath."),
        "localSearch" => Some("Broaden searchText, path, or filters."),
        "astSearch"
            if matches!(query["operation"].as_str(), Some("files"))
                || query["treeKind"] == "filesystem" =>
        {
            Some("Broaden path or file filters.")
        }
        "astTopology" => Some("Inspect diagnostics, then broaden the graph scope if needed."),
        // A pattern must parse as a complete node: `const $A = $B` misses
        // statements that `const $A = $B;` matches.
        "astSearch" if query["operation"] == "match" && query["pattern"].is_string() => Some(
            "Write the pattern as a complete node (keep terminators like `;`), check a tree view, then broaden path or filters.",
        ),
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

fn error_fallback_hint(tool: &str, query: &Value, row: &Value) -> Option<&'static str> {
    let code = row
        .pointer("/data/errorCode")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let message = row
        .pointer("/data/error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // A sandbox refusal is about where the process runs, not the query.
    if message.contains("outside allowed directories") {
        return Some(
            "The path is outside the allowed roots: run from inside the workspace, or add it to ALLOWED_PATHS / WORKSPACE_ROOT.",
        );
    }
    if code.contains("auth") {
        return Some("Authenticate or correct credentials; do not broaden the query.");
    }
    if code.contains("permission") || code.contains("forbidden") {
        return Some("Verify access and token scopes; do not treat denial as absence.");
    }
    if code.contains("rate") {
        return Some("Wait for Retry-After or the provider reset before retrying.");
    }
    if code.contains("timeout") || code.contains("transport") {
        return Some("Retry once; if it persists, narrow scope and verify provider availability.");
    }
    if code.contains("snapshot") {
        return Some("Discard prior pages and restart without the stale snapshot.");
    }
    if tool == "localFetch" && code.contains("fileaccess") {
        return Some(
            "Verify the path with astSearch operation:\"files\", then retry the exact path.",
        );
    }
    if tool == "astSearch"
        && let Some(hint) = ast_search_error_hint(&code)
    {
        return Some(hint);
    }
    fallback_hint(tool, query)
}

/// astSearch failures whose recovery is not "broaden the query".
fn ast_search_error_hint(code: &str) -> Option<&'static str> {
    if code.contains("outsideallowedroots") || code.contains("symlinkescape") {
        return Some("Use a path inside an allowed root; broadening will not help.");
    }
    if code.contains("compilefailed") {
        return Some(
            "Make the pattern a complete node (add `;` or the body), or inspect its shape with treeKind:\"syntax\".",
        );
    }
    if code.contains("inputtoolarge") || code.contains("source.limit") {
        return Some("Target a smaller file or narrower directory scope.");
    }
    if code.starts_with("ast.language.") {
        return Some(
            "Set langType to the grammar of the source files (e.g. \"typescript\", \"rust\").",
        );
    }
    None
}

fn add_fallback_hint(row: &mut Value, position: usize, tool: &str, queries: &[Value]) {
    let status = row.get("status").and_then(Value::as_str);
    if !matches!(status, Some("empty" | "error")) || has_recovery(row) {
        return;
    }
    let index = row
        .get("index")
        .and_then(Value::as_u64)
        .unwrap_or(position as u64) as usize;
    let query = queries.get(index).cloned().unwrap_or(Value::Null);
    let hint = if status == Some("error") {
        error_fallback_hint(tool, &query, row)
    } else {
        fallback_hint(tool, &query)
    };
    let Some(hint) = hint else {
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
pub(super) fn apply_hint_policy(row: &mut Value, tool: &str, query: &Value) {
    add_fallback_hint(row, 0, tool, std::slice::from_ref(query));
    visit(row, false, &mut std::collections::BTreeSet::new());
}

/// Sanitize every string field, including provider diagnostics and metadata.
/// Domain content scans alone cannot protect errors returned by a remote server.
pub(super) fn sanitize_fields(
    value: &mut Value,
    security: &crate::security::ContentSecurity,
    context: &super::ExecutionContext,
) -> Result<(), super::ExecutionError> {
    context.check()?;
    crate::security::sanitize_json(value, &mut |text| {
        Ok::<_, super::ExecutionError>(security.sanitize_text(text, None).content)
    })
}

/// Opt-in email masking for gh outputs (`output.redactEmails`): applied
/// after secret sanitization so commit-author addresses and similar PII do
/// not leave the runtime when the user asked for redaction.
pub(super) fn redact_email_fields(
    value: &mut Value,
    security: &crate::security::ContentSecurity,
    context: &super::ExecutionContext,
) -> Result<(), super::ExecutionError> {
    context.check()?;
    crate::security::sanitize_json(value, &mut |text| {
        Ok::<_, super::ExecutionError>(security.redact_emails(text))
    })
}

/// Apply the shared ordinary-output disclosure policy before output validation
/// or any downstream consumer observes the value.
pub(super) fn finalize_output_fields(
    value: &mut Value,
    tool: &str,
    security: &crate::security::ContentSecurity,
    context: &super::ExecutionContext,
    redact_emails: bool,
) -> Result<(), super::ExecutionError> {
    sanitize_fields(value, security, context)?;
    if redact_emails && tool.starts_with("gh") {
        redact_email_fields(value, security, context)?;
    }
    Ok(())
}

fn preserve_continuation_metadata(value: &mut Value, original_query: &Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                preserve_continuation_metadata(value, original_query);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                preserve_continuation_metadata(value, original_query);
            }
            let continuation_tool = object
                .get("tool")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let (Some(_tool), Some(mut next_query)) = (continuation_tool, object.remove("query"))
            {
                if let Some(next_query_object) = next_query.as_object_mut() {
                    for field in ["reasoning", "debug"] {
                        if let Some(value) = original_query.get(field) {
                            next_query_object.insert(field.into(), value.clone());
                        }
                    }
                }
                object.insert("query".into(), next_query);
            }
        }
        _ => {}
    }
}

pub fn result_row(
    tool: &str,
    index: usize,
    query: &Value,
    mut data: Value,
    status: Option<&str>,
) -> Value {
    // clasify shapes its own query-level `next.clasify`. Any tool/query
    // pairs inside page receipts belong to the already-finalized hidden read
    // and retain that nested invocation's rationale and debug setting.
    if tool != "clasify" {
        preserve_continuation_metadata(&mut data, query);
    }
    if let Some(object) = data.as_object_mut() {
        if object.get("isPartial") == Some(&Value::Null) {
            object.insert("isPartial".into(), Value::Bool(false));
        }
        let preserve_compare_status = tool == "ghGetHistoryItem"
            && object.get("type").and_then(Value::as_str) == Some("compare");
        for key in [
            "cache",
            "goal",
            "reasoning",
            "debug",
            "researchSuggestions",
            "query",
        ] {
            object.remove(key);
        }
        if !preserve_compare_status {
            object.remove("status");
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
    let mut meta = if tool == "ghSearch" {
        json!({"evidence":{"confidence":confidence,"kind":kind}})
    } else {
        json!({"evidence":{"kind":kind,"confidence":confidence}})
    };
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
    let mut row = json!({"index":index,"data":data});
    if query.get("debug").and_then(Value::as_bool) == Some(true) {
        row["meta"] = meta;
    }
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
        "astTopology" => "syntactic",
        "lspSearch" => match data.pointer("/lsp/source").and_then(Value::as_str) {
            Some("native-graph-facts" | "markdown") => "syntactic",
            _ => "semantic",
        },
        "localSearch" => "lexical",
        "artifactSearch" | "clasify" => "provider",
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

/// AST tools format row paths relative to the queried root before the shared
/// envelope compactor runs. Restore the same absolute base emitted by the
/// TypeScript finalizer so consumers can resolve those paths losslessly.
pub fn attach_query_base(value: &mut Value, tool: &str, query: &Value) {
    let has_error = value["results"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["status"] == "error"));
    if !matches!(tool, "astSearch" | "astTopology") || has_error {
        return;
    }
    let Some(path) = query.get("path").and_then(Value::as_str) else {
        return;
    };
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return;
    };
    if tool == "astTopology" {
        let display_name = canonical
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        if let (Some(name), Some(rows)) = (display_name, value["results"].as_array_mut()) {
            for row in rows {
                if row["data"]["path"] == "." {
                    row["data"]["path"] = json!(name);
                }
            }
        }
    }
    if value.get("base").is_none()
        && let Some(parent) = canonical.parent()
    {
        value["base"] = json!(parent.to_string_lossy());
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
        "number",
        "title",
        "state",
        "author",
        "labels",
        "createdAt",
        "mergedAt",
        "commentsCount",
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
        // Per-entry match-row accounting must stay on each entry: hoisting it
        // whenever the values happen to coincide (typical on page 1) makes the
        // row shape depend on the data, so identical queries drift between
        // pages and between CLI and MCP consumers.
        "totalMatchRows",
        "returnedMatchRows",
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
                map.shift_remove("absolutePath");
                map.shift_remove("uri");
                map.insert("path".into(), json!(path));
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
    fn empty_pattern_match_hints_at_complete_nodes() {
        let pattern = json!({"operation": "match", "pattern": "const $A = $B"});
        assert!(
            fallback_hint("astSearch", &pattern).is_some_and(|hint| hint.contains("complete node"))
        );
        let rule = json!({"operation": "match", "rule": "id: x"});
        assert_eq!(
            fallback_hint("astSearch", &rule),
            Some("Broaden the syntax/name query, path, or filters.")
        );
    }

    #[test]
    fn native_language_server_results_are_semantic_evidence() {
        assert_eq!(
            evidence_kind(
                "lspSearch",
                &json!({}),
                &json!({"lsp": {"source": "native"}}),
            ),
            "semantic"
        );
        assert_eq!(
            evidence_kind(
                "lspSearch",
                &json!({}),
                &json!({"lsp": {"source": "native-graph-facts"}}),
            ),
            "syntactic"
        );
    }

    #[test]
    fn ast_search_query_base_uses_the_canonical_parent() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let mut output = json!({"results":[]});
        attach_query_base(
            &mut output,
            "astSearch",
            &json!({"path":manifest.to_string_lossy()}),
        );
        let expected = std::fs::canonicalize(manifest)
            .expect("manifest path")
            .parent()
            .expect("manifest parent")
            .to_string_lossy()
            .into_owned();
        assert_eq!(output["base"], expected);
    }

    #[test]
    fn uri_path_compaction_preserves_field_order_and_appends_path() {
        let output = envelope(vec![result_row(
            "lspSearch",
            0,
            &json!({}),
            json!({"type":"documentSymbols","uri":"file:///repo/src/a.ts","lsp":{},"pagination":{"hasMore":false}}),
            None,
        )]);
        let keys = output["results"][0]["data"]
            .as_object()
            .expect("data object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys, ["type", "lsp", "pagination", "path"]);
    }

    /// localSearch per-file entries must always carry their own
    /// totalMatchRows/returnedMatchRows: hoisting them into `shared` when the
    /// values coincide (typical on page 1) made the row shape data-dependent,
    /// so CLI page-1 output drifted from MCP and from page-2 output.
    #[test]
    fn match_row_accounting_is_never_hoisted_into_shared() {
        let output = envelope(vec![result_row(
            "localSearch",
            0,
            &json!({}),
            json!({"files":[
                {"path":"/repo/a.txt","matches":[{"line":1,"column":0,"value":"x"}],"totalMatchRows":1,"returnedMatchRows":1},
                {"path":"/repo/b.txt","matches":[{"line":1,"column":0,"value":"x"}],"totalMatchRows":1,"returnedMatchRows":1}
            ]}),
            None,
        )]);
        for file in output["results"][0]["data"]["files"]
            .as_array()
            .expect("files")
        {
            assert_eq!(file["totalMatchRows"], 1, "{output}");
            assert_eq!(file["returnedMatchRows"], 1, "{output}");
        }
        assert!(
            output
                .get("shared")
                .and_then(|shared| shared.get("totalMatchRows"))
                .is_none(),
            "{output}"
        );
    }

    #[test]
    fn path_compaction_never_rewrites_evidence_or_executable_queries() {
        let data = json!({"path":"/repo/src/a.ts","content":"/repo/src/a.ts","pagination":{"hasMore":true},"next":{"continue":{"tool":"localFetch","query":{"path":"/repo/src/a.ts","offset":2}}}});
        let output = envelope(vec![result_row(
            "localFetch",
            0,
            &json!({"debug":true}),
            data,
            None,
        )]);
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
    fn shared_compaction_preserves_required_pull_request_row_fields() {
        let output = envelope(vec![result_row(
            "ghSearchHistory",
            0,
            &json!({"operation":"pullRequest"}),
            json!({
                "type":"pullRequests",
                "pullRequests":[
                    {"number":1,"title":"One","state":"merged","author":"octocode","labels":[],"createdAt":"2026-01-01","mergedAt":"2026-01-02","commentsCount":0},
                    {"number":2,"title":"Two","state":"merged","author":"octocode","labels":[],"createdAt":"2026-01-03","mergedAt":"2026-01-04","commentsCount":0}
                ]
            }),
            None,
        )]);
        for row in output["results"][0]["data"]["pullRequests"]
            .as_array()
            .expect("pull requests")
        {
            for field in [
                "number",
                "title",
                "state",
                "author",
                "labels",
                "createdAt",
                "mergedAt",
                "commentsCount",
            ] {
                assert!(row.get(field).is_some(), "missing {field}: {output}");
            }
        }
    }
    #[test]
    fn incomplete_evidence_requires_executable_continuation_or_terminal_diagnostic() {
        let missing = result_row(
            "astSearch",
            0,
            &json!({"operation":"symbols","debug":true}),
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
            &json!({"debug":true}),
            json!({"isPartial":true,"terminalLimit":true}),
            None,
        );
        assert_eq!(
            terminal.pointer("/meta/diagnostics/codes"),
            Some(&json!(["terminalLimitReached"]))
        );
    }

    #[test]
    fn result_rows_normalize_null_is_partial_to_false() {
        let row = result_row(
            "ghGetFileContent",
            0,
            &json!({"debug":false}),
            json!({"files":[],"isPartial":null}),
            None,
        );
        assert_eq!(row.pointer("/data/isPartial"), Some(&json!(false)));
    }

    #[test]
    fn result_rows_preserve_compare_status_as_contract_data() {
        let row = result_row(
            "ghGetHistoryItem",
            0,
            &json!({"operation":"compare"}),
            json!({"type":"compare","status":"ahead","commits":[]}),
            None,
        );
        assert_eq!(row.pointer("/data/status"), Some(&json!("ahead")));
    }

    #[test]
    fn result_metadata_requires_debug() {
        let normal = result_row(
            "localSearch",
            0,
            &json!({"debug":false}),
            json!({"files":[]}),
            None,
        );
        assert!(normal.get("meta").is_none());

        let debug = result_row(
            "localSearch",
            0,
            &json!({"debug":true}),
            json!({"files":[]}),
            None,
        );
        assert_eq!(debug["meta"]["evidence"]["kind"], "lexical");
    }

    #[test]
    fn result_rows_preserve_invocation_metadata_in_continuations() {
        let row = result_row(
            "localFetch",
            0,
            &json!({"reasoning":"Read the next exact page.","debug":false}),
            json!({
                "next": {
                    "continue": {
                        "tool": "localFetch",
                        "query": {"path":"/repo/a.rs","offset":2}
                    }
                }
            }),
            None,
        );
        assert_eq!(
            row.pointer("/data/next/continue/query/reasoning"),
            Some(&json!("Read the next exact page."))
        );
        assert_eq!(
            row.pointer("/data/next/continue/query/debug"),
            Some(&json!(false))
        );
    }

    #[test]
    fn outer_clasify_metadata_does_not_overwrite_nested_continuation_ownership() {
        let row = result_row(
            "clasify",
            0,
            &json!({"reasoning":"Evaluate captured evidence.","debug":false}),
            json!({
                "context": {
                    "next": {
                        "continue": {
                            "tool": "localFetch",
                            "query": {
                                "path":"/repo/a.rs",
                                "offset":2,
                                "reasoning":"Read the next exact page.",
                                "debug":true
                            }
                        }
                    }
                }
            }),
            None,
        );
        assert_eq!(
            row.pointer("/data/context/next/continue/query/reasoning"),
            Some(&json!("Read the next exact page."))
        );
        assert_eq!(
            row.pointer("/data/context/next/continue/query/debug"),
            Some(&json!(true))
        );
    }

    #[test]
    fn error_fallbacks_are_failure_class_aware_and_preserve_domain_recovery() {
        let query = json!({"operation":"repositories","keywords":["octocode"]});
        let mut timeout = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"timed out","errorCode":"timeout"}
        });
        apply_hint_policy(&mut timeout, "ghSearch", &query);
        let hint = timeout["data"]["hints"][0].as_str().expect("hint");
        assert!(hint.contains("Retry once"), "{timeout}");
        assert!(!hint.contains("Broaden keywords"), "{timeout}");

        let mut permission = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"forbidden","errorCode":"permission"}
        });
        apply_hint_policy(&mut permission, "ghSearch", &query);
        assert!(
            permission["data"]["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("token scopes")),
            "{permission}"
        );

        let mut owned = json!({
            "index": 0,
            "status": "error",
            "data": {
                "error":"missing",
                "errorCode":"notFound",
                "hints":["Inspect the repository tree."]
            }
        });
        apply_hint_policy(&mut owned, "ghSearch", &query);
        assert_eq!(
            owned["data"]["hints"],
            json!(["Inspect the repository tree."])
        );

        let mut missing_path = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"missing","errorCode":"fileAccessFailed"}
        });
        apply_hint_policy(
            &mut missing_path,
            "localFetch",
            &json!({"path":"/repo/missing.rs"}),
        );
        assert!(
            missing_path["data"]["hints"][0]
                .as_str()
                .is_some_and(|hint| hint.contains("astSearch operation:\"files\"")),
            "{missing_path}"
        );
    }

    #[test]
    fn ast_search_error_hints_are_code_specific() {
        let query = json!({"operation":"match","path":"/repo","pattern":"foo($A)"});
        for (code, expected) in [
            ("ast.policy.outsideAllowedRoots", "allowed root"),
            ("structural.query.compileFailed", "treeKind"),
            ("ast.policy.inputTooLarge", "smaller"),
            ("ast.language.required", "langType"),
            ("ast.language.unsupported", "langType"),
        ] {
            let mut row = json!({
                "index": 0,
                "status": "error",
                "data": {"error":"failed","errorCode":code}
            });
            apply_hint_policy(&mut row, "astSearch", &query);
            let hint = row["data"]["hints"][0].as_str().expect("hint");
            assert!(hint.contains(expected), "{code}: {hint}");
            assert!(!hint.contains("Broaden the syntax"), "{code}: {hint}");
        }
    }

    fn sanitize_context() -> crate::runtime::ExecutionContext {
        crate::runtime::ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(1),
            output_bytes: 16_000,
        }
    }

    #[test]
    fn sanitizer_preserves_non_secret_next_query_and_location() {
        // A benign, non-secret-shaped string passes through unchanged — the
        // continuation stays executable and the location is untouched.
        let mut value = json!({
            "content": "token ghp_secretvalue12",
            "next": {
                "continue": {
                    "tool": "localFetch",
                    "query": {"path": "/repo/ghp_secretvalue12.rs", "offset": 2},
                    "confidence": "exact"
                }
            },
            "location": {"localPath": "/tmp/ghp_secretvalue12/repo"}
        });
        let context = sanitize_context();
        let security = crate::security::ContentSecurity::new(std::sync::Arc::new(
            crate::security::SecurityRegistry::default(),
        ));
        sanitize_fields(&mut value, &security, &context).expect("sanitize");
        assert_eq!(
            value.pointer("/next/continue/query/path"),
            Some(&json!("/repo/ghp_secretvalue12.rs"))
        );
        assert_eq!(
            value.pointer("/location/localPath"),
            Some(&json!("/tmp/ghp_secretvalue12/repo"))
        );
        // The executable tool identifier survives verbatim.
        assert_eq!(
            value.pointer("/next/continue/query/offset"),
            Some(&json!(2))
        );
    }

    #[test]
    fn email_redaction_is_opt_in_and_masks_commit_authors() {
        let commit_row = || {
            json!({
                "commits": [{
                    "sha": "abc123",
                    "author": {"name": "Dev One", "email": "dev.one+git@example.co.uk"},
                    "message": "fix: reported by user@example.com"
                }]
            })
        };
        let context = sanitize_context();
        let security = crate::security::ContentSecurity::new(std::sync::Arc::new(
            crate::security::SecurityRegistry::default(),
        ));
        // Default path (no opt-in): emails pass through unchanged.
        let mut untouched = commit_row();
        sanitize_fields(&mut untouched, &security, &context).expect("sanitize");
        assert_eq!(untouched, commit_row(), "default output must be unchanged");
        // Opt-in path: every email leaf is masked, structure preserved.
        let mut redacted = commit_row();
        redact_email_fields(&mut redacted, &security, &context).expect("redact");
        assert_eq!(
            redacted.pointer("/commits/0/author/email"),
            Some(&json!("[REDACTED-EMAIL]"))
        );
        assert_eq!(
            redacted.pointer("/commits/0/message"),
            Some(&json!("fix: reported by [REDACTED-EMAIL]"))
        );
        assert_eq!(
            redacted.pointer("/commits/0/author/name"),
            Some(&json!("Dev One"))
        );
    }

    #[test]
    fn sanitizer_redacts_real_secret_in_next_query_and_location() {
        // A real GitHub token embedded in a continuation query or a location
        // leaf is now redacted — better to break the continuation than to leak.
        let token = format!("ghp_{}", "a".repeat(37));
        let mut value = json!({
            "next": {
                "continue": {
                    "tool": "localFetch",
                    "query": {"path": format!("/repo/{token}.rs")}
                }
            },
            "location": {"localPath": format!("/tmp/{token}/repo")}
        });
        let context = sanitize_context();
        let security = crate::security::ContentSecurity::new(std::sync::Arc::new(
            crate::security::SecurityRegistry::default(),
        ));
        sanitize_fields(&mut value, &security, &context).expect("sanitize");
        // Tool preserved so the call is still routable.
        assert_eq!(
            value.pointer("/next/continue/tool"),
            Some(&json!("localFetch"))
        );
        let query_path = value
            .pointer("/next/continue/query/path")
            .and_then(Value::as_str)
            .expect("query path");
        assert!(!query_path.contains("ghp_"), "token leaked in query path");
        let local_path = value
            .pointer("/location/localPath")
            .and_then(Value::as_str)
            .expect("localPath");
        assert!(!local_path.contains("ghp_"), "token leaked in location");
    }
}
