//! Transport-neutral structured result metadata and lossless path compaction.
use crate::tools::id::ToolId;
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

/// Recovery for an empty or unhinted error row, by tool. Exhaustive over
/// `ToolId` so a new tool must choose its fallback at compile time.
fn fallback_hint(tool: ToolId, query: &Value) -> &'static str {
    match tool {
        ToolId::GhSearch if query["operation"] == "tree" => {
            "Verify owner/repo/branch, or broaden path/depth."
        }
        ToolId::GhSearch => "Broaden keywords or remove filters.",
        ToolId::GhGetFileContent => "Verify owner/repo/branch/path, or remove matchString.",
        ToolId::GhSearchHistory => "Broaden keywords or remove history filters.",
        ToolId::GhGetHistoryItem => "Verify owner/repo and the number, ref, or compare refs.",
        ToolId::ArtifactSearch => "Check packageName, or broaden keywords.",
        ToolId::Clasify => "Inspect resources, typed questions, and OCTOCODE_CLASSIFICATION_API.",
        ToolId::GhCloneRepo => "Verify owner/repo/branch and sparsePath.",
        ToolId::LocalSearch => "Broaden searchText, path, or filters.",
        ToolId::StructureSearch => "Broaden path, depth, or file filters.",
        // A pattern must parse as a complete node: `const $A = $B` misses
        // statements that `const $A = $B;` matches.
        ToolId::AstSearch if query["operation"] == "match" && query["pattern"].is_string() => {
            "Write the pattern as a complete node (keep terminators like `;`), check a tree view, then broaden path or filters."
        }
        ToolId::AstSearch => "Broaden the syntax/name query, path, or filters.",
        ToolId::AstTopology => "Inspect diagnostics, then broaden the graph scope if needed.",
        ToolId::AstRewrite => "Broaden the structural pattern, path, or file filters.",
        ToolId::LocalFetch => "Verify path/range, or remove matchString.",
        ToolId::LspSearch => "Refresh uri/symbolName/lineHint, or broaden workspaceRoot.",
    }
}

const SANDBOX_HINT: &str = "The path is outside the allowed roots: run from inside the workspace, or add it to ALLOWED_PATHS / WORKSPACE_ROOT.";

/// Recovery keyed by the exact `errorCode` values the runtime emits
/// (provider kinds, policy/AST/LSP/clone/classification codes). Codes not
/// listed fall back to the tool hint.
fn error_code_hint(tool: ToolId, code: &str) -> Option<&'static str> {
    Some(match (tool, code) {
        (
            _,
            crate::policy::PATH_OUTSIDE_ALLOWED_ROOTS
            | "ast.policy.outsideAllowedRoots"
            | "ast.policy.symlinkEscape",
        ) => SANDBOX_HINT,
        (_, "authentication") => "Authenticate or correct credentials; do not broaden the query.",
        (_, "permission" | "ast.policy.permissionDenied") => {
            "Verify access and token scopes; do not treat denial as absence."
        }
        (_, "rateLimited" | "rate_limit" | "clone.rateLimited" | "classificationRateLimited") => {
            "Wait for Retry-After or the provider reset before retrying."
        }
        (
            _,
            "timeout"
            | "transport"
            | "network.timeout"
            | "lsp.timeout"
            | "clone.execution.timeout"
            | "clone.cache.lockTimeout"
            | "ast.rewrite.lock_timeout",
        ) => "Retry once; if it persists, narrow scope and verify provider availability.",
        (
            _,
            "staleSnapshot"
            | "structure.snapshot.changed"
            | "ast.snapshot.changed"
            | "lsp.snapshot.changed",
        ) => "Discard prior pages and restart without the stale snapshot.",
        (ToolId::LocalFetch, "fileAccessFailed") => {
            "Verify the path with structureSearch operation:\"files\", then retry the exact path."
        }
        (ToolId::AstSearch, "structural.query.compileFailed" | "ast.query.invalidPattern") => {
            "Make the pattern a complete node (add `;` or the body), or inspect its shape with operation:\"syntaxTree\"."
        }
        (ToolId::AstSearch, "ast.policy.inputTooLarge" | "ast.source.limit") => {
            "Target a smaller file or narrower directory scope."
        }
        (
            ToolId::AstSearch,
            "ast.language.required" | "ast.language.unsupported" | "ast.language.mismatch",
        ) => "Set langType to the grammar of the source files (e.g. \"typescript\", \"rust\").",
        (ToolId::AstSearch, "ast.language.fileRequired") => {
            "For a directory, drop langType: each extension picks its grammar; languageGlobs overrides (e.g. {cpp:[\"**/*.h\"]})."
        }
        (ToolId::AstSearch, "ast.language.directoryRequired") => {
            "For a single file, use langType instead of languageGlobs."
        }
        _ => return None,
    })
}

fn error_fallback_hint(tool: ToolId, query: &Value, row: &Value) -> &'static str {
    let code = row
        .pointer("/data/errorCode")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Local tools report a sandbox refusal under its own code
    // (`pathOutsideAllowedRoots`), so the hint is keyed by code alone.
    error_code_hint(tool, code).unwrap_or_else(|| fallback_hint(tool, query))
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
    let Some(tool) = ToolId::from_name(tool) else {
        return;
    };
    let query = queries.get(index).cloned().unwrap_or(Value::Null);
    let hint = if status == Some("error") {
        error_fallback_hint(tool, &query, row)
    } else {
        fallback_hint(tool, &query)
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

/// Anchor the envelope `base` on the query's scan root so `join(base, path)`
/// is the real file for every row and `base` is identical across pages:
/// - structureSearch and astSearch rows lead with the root's own name, so
///   `base` is its parent.
/// - astTopology row fields (`file`, entrypoints, diagnostics) are relative to
///   the scanned directory, so `base` is that directory and the row `path` is
///   `.` (a file root keeps its parent and file name).
/// - localSearch rows were compacted against the common directory of the rows
///   on this page; re-anchor them on the queried directory.
pub fn attach_query_base(value: &mut Value, tool: &str, query: &Value) {
    let has_error = value["results"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["status"] == "error"));
    if !matches!(
        tool,
        "structureSearch" | "astSearch" | "astTopology" | "localSearch"
    ) || has_error
    {
        return;
    }
    let Some(path) = query.get("path").and_then(Value::as_str) else {
        return;
    };
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return;
    };
    let is_dir = canonical.is_dir();
    let Some(parent) = canonical.parent() else {
        return;
    };
    let root = if is_dir { canonical.as_path() } else { parent };
    match tool {
        "astTopology" => {
            let display = if is_dir {
                Some(".".to_owned())
            } else {
                canonical
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            };
            if let (Some(display), Some(rows)) = (display, value["results"].as_array_mut()) {
                for row in rows {
                    if row["data"].get("path").is_some_and(Value::is_string) {
                        row["data"]["path"] = json!(display);
                    }
                }
            }
            value["base"] = json!(root.to_string_lossy());
        }
        "localSearch" => {
            let absolute = Path::new(path).is_absolute().then_some(path);
            for candidate in [
                Some(root.to_string_lossy().into_owned()),
                absolute.map(str::to_owned),
            ]
            .into_iter()
            .flatten()
            {
                if rebase_row_paths(value, &candidate) {
                    break;
                }
            }
        }
        _ => {
            if value.get("base").is_none() {
                value["base"] = json!(parent.to_string_lossy());
            }
        }
    }
}

/// Re-express the envelope's compacted row paths relative to `root` when the
/// envelope chose a deeper common directory. Returns whether `base` is now
/// `root` (or there was no base to move).
fn rebase_row_paths(value: &mut Value, root: &str) -> bool {
    let Some(base) = value["base"].as_str().map(str::to_owned) else {
        return true;
    };
    if base == root {
        return true;
    }
    let root_prefix = if root.ends_with('/') {
        root.to_owned()
    } else {
        format!("{root}/")
    };
    let Some(prefix) = base.strip_prefix(&root_prefix).map(str::to_owned) else {
        return false;
    };
    if let Some(rows) = value["results"].as_array_mut() {
        for row in rows {
            prefix_relative_paths(&mut row["data"], 0, &prefix);
        }
    }
    value["base"] = json!(root);
    true
}

/// Inverse of [`rewrite_paths`] for one directory prefix: same traversal and
/// exclusions, applied to the relative `path` fields it produced.
fn prefix_relative_paths(value: &mut Value, depth: usize, prefix: &str) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(map) => {
            if let Some(Value::String(path)) = map.get_mut("path")
                && !Path::new(path.as_str()).is_absolute()
            {
                *path = format!("{prefix}/{path}");
            }
            for (key, child) in map {
                if !matches!(key.as_str(), "next" | "location") {
                    prefix_relative_paths(child, depth + 1, prefix);
                }
            }
        }
        Value::Array(array) => {
            for child in array {
                prefix_relative_paths(child, depth + 1, prefix);
            }
        }
        _ => {}
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
        assert!(fallback_hint(ToolId::AstSearch, &pattern).contains("complete node"));
        let rule = json!({"operation": "match", "rule": "id: x"});
        assert_eq!(
            fallback_hint(ToolId::AstSearch, &rule),
            "Broaden the syntax/name query, path, or filters."
        );
    }

    fn error_hint(tool: &str, query: &Value, code: &str, message: &str) -> String {
        let mut row = json!({
            "index": 0,
            "status": "error",
            "data": {"error": message, "errorCode": code}
        });
        apply_hint_policy(&mut row, tool, query);
        row["data"]["hints"][0]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn ast_rewrite_apply_errors_get_no_generic_before_hash_hint() {
        // astRewrite attaches its own code-specific hints; the fallback only
        // covers rows without recovery and never invents a hash instruction.
        let apply = json!({"apply": true, "path": "/repo"});
        for code in ["ast.rewrite.hash_mismatch", "ast.rewrite.io"] {
            let hint = error_hint("astRewrite", &apply, code, "failed");
            assert!(!hint.contains("beforeHash"), "{code}: {hint}");
        }
    }

    #[test]
    fn error_hints_match_exact_codes_not_substrings() {
        let query = json!({"operation":"repositories","keywords":["octocode"]});
        assert!(error_hint("ghSearch", &query, "rateLimited", "slow down").contains("Retry-After"));
        // A code merely containing "auth" is not an authentication failure.
        assert_eq!(
            error_hint("ghSearch", &query, "authorizationPending", "x"),
            "Broaden keywords or remove filters."
        );
        // The sandbox hint keys on the dedicated code, never on message text.
        for tool in ["localSearch", "localFetch", "lspSearch", "astRewrite"] {
            assert!(
                error_hint(
                    tool,
                    &json!({"path":"/etc"}),
                    "pathOutsideAllowedRoots",
                    "denied"
                )
                .contains("ALLOWED_PATHS"),
                "{tool}"
            );
        }
        assert!(
            !error_hint(
                "localSearch",
                &json!({"path":"/etc"}),
                "fileAccessFailed",
                "Path '/etc' is outside allowed directories"
            )
            .contains("ALLOWED_PATHS")
        );
        // Unknown tool names get no invented hint.
        assert_eq!(error_hint("notATool", &query, "timeout", "x"), "");
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
    fn local_search_and_topology_bases_are_the_query_scan_root() {
        let dir = tempfile::tempdir().expect("fixture");
        let root = std::fs::canonicalize(dir.path()).expect("root");
        std::fs::create_dir_all(root.join("sub/a")).expect("dirs");
        std::fs::write(root.join("sub/a/x.rs"), "x").expect("file");
        let root_str = root.to_string_lossy().into_owned();
        // The envelope compacted this page against its deepest common dir.
        let mut output = json!({
            "results": [{"data": {"files": [{"path": "x.rs"}],
                "next": {"nextPage": {"tool": "localSearch", "query": {"path": root_str}}}}}],
            "base": root.join("sub/a").to_string_lossy(),
        });
        attach_query_base(&mut output, "localSearch", &json!({"path": root_str}));
        assert_eq!(output["base"], root_str.as_str());
        let row_path = output["results"][0]["data"]["files"][0]["path"]
            .as_str()
            .expect("row path");
        assert_eq!(row_path, "sub/a/x.rs");
        assert!(root.join(row_path).is_file());
        assert_eq!(
            output["results"][0]["data"]["next"]["nextPage"]["query"]["path"],
            root_str.as_str()
        );

        let mut topology = json!({
            "results": [{"data": {"path": "workspace/relative", "results": [{"file": "sub/a/x.rs"}]}}],
            "base": "/elsewhere",
        });
        attach_query_base(&mut topology, "astTopology", &json!({"path": root_str}));
        assert_eq!(topology["base"], root_str.as_str());
        assert_eq!(topology["results"][0]["data"]["path"], ".");
        let file = topology["results"][0]["data"]["results"][0]["file"]
            .as_str()
            .expect("file");
        assert!(root.join(file).is_file());
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
                .is_some_and(|hint| hint.contains("structureSearch operation:\"files\"")),
            "{missing_path}"
        );
    }

    #[test]
    fn ast_search_error_hints_are_code_specific() {
        let query = json!({"operation":"match","path":"/repo","pattern":"foo($A)"});
        for (code, expected) in [
            ("ast.policy.outsideAllowedRoots", "allowed root"),
            ("structural.query.compileFailed", "syntaxTree"),
            ("ast.policy.inputTooLarge", "smaller"),
            ("ast.language.required", "langType"),
            ("ast.language.unsupported", "langType"),
            ("ast.language.fileRequired", "languageGlobs"),
            ("ast.language.directoryRequired", "langType"),
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
        let security = crate::security::ContentSecurity::new();
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
        let security = crate::security::ContentSecurity::new();
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
        let security = crate::security::ContentSecurity::new();
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
