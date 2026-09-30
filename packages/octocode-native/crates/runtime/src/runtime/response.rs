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
        ToolId::GhStructure => "Verify owner/repo/branch, or broaden path/depth.",
        ToolId::GhSearchCode => "Broaden keywords or remove filters.",
        ToolId::GhSearchRepo => "Broaden keywords or remove repository filters.",
        ToolId::GhGetFileContent => "Verify owner/repo/branch/path, or remove matchString.",
        ToolId::GhSearchHistory => "Broaden keywords or remove history filters.",
        ToolId::GhGetHistoryItem => "Verify owner/repo and the number, ref, or compare refs.",
        ToolId::ArtifactSearch => "Check packageName, or broaden keywords.",
        ToolId::Clasify => "Inspect resources, typed questions, and OCTOCODE_CLASSIFICATION_API.",
        ToolId::GhCloneRepo => "Verify owner/repo/branch and sparsePath.",
        ToolId::LocalSearch => "Broaden searchText, path, or filters.",
        ToolId::StructureSearch => "Broaden path, depth, or file filters.",
        // A pattern must parse as one complete node of the target grammar.
        ToolId::AstSearch if query["operation"] == "match" && query["pattern"].is_string() => {
            "Write the pattern as a complete node with its body, check a tree view, then broaden path or filters."
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
            | "ast.policy.symlinkEscape"
            | "structure.policy.outsideAllowedRoots"
            | "structure.policy.symlinkEscape",
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
            "Make the pattern one complete, parseable node, or inspect its shape with operation:\"syntaxTree\"."
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
        (_, code) if is_not_found_code(code) => {
            "Verify the path exists (structureSearch on its parent directory), then retry the exact path."
        }
        (_, code) if is_invalid_input_code(code) => {
            "Correct the rejected field named in the error; broadening will not help."
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

fn add_fallback_hint(row: &mut Value, position: usize, tool: ToolId, queries: &[Value]) {
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
pub(super) fn apply_hint_policy(row: &mut Value, tool: ToolId, query: &Value) {
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
    tool: ToolId,
    security: &crate::security::ContentSecurity,
    context: &super::ExecutionContext,
    redact_emails: bool,
) -> Result<(), super::ExecutionError> {
    sanitize_fields(value, security, context)?;
    if redact_emails && tool.is_github() {
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
                    for field in ["debug"] {
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
    tool: ToolId,
    index: usize,
    query: &Value,
    mut data: Value,
    status: Option<&str>,
) -> Value {
    // clasify shapes its own query-level `next.clasify`. Any tool/query
    // pairs inside page receipts belong to the already-finalized hidden read
    // and retain that nested invocation's rationale and debug setting.
    if tool != ToolId::Clasify {
        preserve_continuation_metadata(&mut data, query);
    }
    if let Some(object) = data.as_object_mut() {
        if object.get("isPartial") == Some(&Value::Null) {
            object.insert("isPartial".into(), Value::Bool(false));
        }
        let preserve_compare_status = tool == ToolId::GhGetHistoryItem
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
    if tool == ToolId::GhGetFileContent {
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
    let mut meta = if matches!(
        tool,
        ToolId::GhSearchRepo | ToolId::GhSearchCode | ToolId::GhStructure
    ) {
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

/// Default responses carry the answer and what the next call needs. Each tool
/// declares the fields it computes for diagnosis ([`debug_only_fields`]); the
/// shared rules below remove only structure that asserts nothing: finished
/// single-page pagination, snapshots that every continuation already
/// carries, false/zero defaults, info-level notes, and echoes of the request.
/// `debug: true` keeps everything. Error rows stay whole: every field there
/// explains the failure. Clasify rows never reach this pass; they have their
/// own resource-major projection.
pub fn minimize_row(row: &mut Value, tool: ToolId, query: &Value) {
    if query.get("debug").and_then(Value::as_bool) == Some(true) {
        return;
    }
    if let Some(fields) = row.as_object_mut() {
        fields.remove("cache");
    }
    if row["status"] == "error" {
        return;
    }
    let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    // Minimal never means below the contract. Remember which contract
    // variants the row satisfies; if the rules leave none satisfied, restore
    // the variant that needs the fewest fields back. Only fields a rule may
    // remove are saved, so answer arrays are never copied.
    let variants = contract_data_variants(tool)
        .iter()
        .filter(|variant| variant.iter().all(|key| data.contains_key(key)))
        .collect::<Vec<_>>();
    let saved = variants
        .iter()
        .flat_map(|variant| variant.iter())
        .filter(|key| is_removable(tool, key))
        .filter_map(|key| data.get(key).map(|value| (key.clone(), value.clone())))
        .collect::<std::collections::BTreeMap<_, _>>();
    minimize_data(data, tool, query);
    let still = |variant: &&std::collections::BTreeSet<String>| {
        variant.iter().all(|key| data.contains_key(key))
    };
    if !variants.is_empty() && !variants.iter().any(still) {
        let cheapest = variants.iter().min_by_key(|variant| {
            variant
                .iter()
                .filter(|key| !data.contains_key(*key))
                .count()
        });
        for key in cheapest.into_iter().flat_map(|variant| variant.iter()) {
            if let Some(value) = saved.get(key) {
                data.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

/// Every top-level field a minimization rule can remove for this tool.
fn is_removable(tool: ToolId, key: &str) -> bool {
    is_pagination_key(key)
        || debug_only_fields(tool)
            .iter()
            .any(|path| path.split('.').next() == Some(key))
        || matches!(
            key,
            "truncated"
                | "isPartial"
                | "incompleteResults"
                | "capped"
                | "filesSkipped"
                | "contentView"
                | "operation"
                | "analysis"
                | "type"
                | "owner"
                | "repo"
                | "snapshot"
                | "totalCount"
                | "diagnostics"
                | "stats"
                | "committer"
                | "messageHeadline"
        )
}

fn minimize_data(data: &mut Map<String, Value>, tool: ToolId, query: &Value) {
    for path in debug_only_fields(tool) {
        remove_path(data, path);
    }
    let more = data
        .iter()
        .any(|(key, value)| is_pagination_key(key) && value["hasMore"] == true);
    for flag in ["truncated", "isPartial", "incompleteResults", "capped"] {
        if data.get(flag) == Some(&Value::Bool(false)) {
            data.remove(flag);
        }
    }
    if data.get("filesSkipped").and_then(Value::as_u64) == Some(0) {
        data.remove("filesSkipped");
    }
    if data.get("contentView").and_then(Value::as_str) == Some("none") {
        data.remove("contentView");
    }
    for key in ["operation", "analysis", "type", "owner", "repo"] {
        if data.get(key).is_some() && data.get(key) == query.get(key) {
            data.remove(key);
        }
    }
    data.retain(|key, value| {
        !is_pagination_key(key)
            || value["hasMore"] == true
            || value
                .get("currentPage")
                .or_else(|| value.get("page"))
                .and_then(Value::as_u64)
                .is_some_and(|page| page > 1)
    });
    data.remove("snapshot");
    if !more {
        data.remove("totalCount");
    }
    if let Some(diagnostics) = data.get_mut("diagnostics").and_then(Value::as_array_mut) {
        diagnostics.retain(|entry| entry["severity"] != "info");
        if diagnostics.is_empty() {
            data.remove("diagnostics");
        }
    }
    minimize_stats(data, more);
    // A commit's committer usually repeats its author, and the headline
    // repeats the message's first line.
    if data.get("committer").is_some()
        && data["committer"].get("name") == data.get("author").and_then(|a| a.get("name"))
    {
        data.remove("committer");
    }
    if let (Some(headline), Some(message)) = (
        data.get("messageHeadline").and_then(Value::as_str),
        data.get("message").and_then(Value::as_str),
    ) && message.starts_with(headline)
    {
        data.remove("messageHeadline");
    }
}

/// The required top-level fields of each variant of the tool's data
/// contract (one set per union branch, common requirements merged in).
fn contract_data_variants(tool: ToolId) -> &'static [std::collections::BTreeSet<String>] {
    type Variants = Vec<std::collections::BTreeSet<String>>;
    static VARIANTS: std::sync::OnceLock<std::collections::HashMap<ToolId, Variants>> =
        std::sync::OnceLock::new();
    VARIANTS
        .get_or_init(|| {
            let Ok(contract) = crate::contracts::parsed_contract() else {
                return Default::default();
            };
            contract["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|tool| {
                    let id = ToolId::from_name(tool["name"].as_str()?)?;
                    let schema = &tool["outputSchema"];
                    let defs = &schema["$defs"];
                    let rows = resolve_ref(&schema["properties"]["results"]["items"], defs);
                    let data = resolve_ref(&rows["properties"]["data"], defs);
                    Some((id, variants_of(data, defs, &Default::default(), 0)))
                })
                .collect()
        })
        .get(&tool)
        .map_or(&[], Vec::as_slice)
}

fn resolve_ref<'a>(schema: &'a Value, defs: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .map_or(schema, |name| &defs[name])
}

/// Expand a schema into its required-field variants: `required` and `allOf`
/// members accumulate; each `anyOf`/`oneOf` branch forks a variant. Nested
/// property schemas are not descended (the minimizer removes top-level
/// fields only).
fn variants_of(
    schema: &Value,
    defs: &Value,
    inherited: &std::collections::BTreeSet<String>,
    depth: usize,
) -> Vec<std::collections::BTreeSet<String>> {
    let schema = resolve_ref(schema, defs);
    let mut base = inherited.clone();
    base.extend(
        schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    if depth > 8 {
        return vec![base];
    }
    let mut variants = vec![base];
    for member in schema["allOf"].as_array().into_iter().flatten() {
        variants = variants
            .iter()
            .flat_map(|variant| variants_of(member, defs, variant, depth + 1))
            .collect();
    }
    for union in ["anyOf", "oneOf"] {
        if let Some(branches) = schema[union].as_array().filter(|b| !b.is_empty()) {
            variants = variants
                .iter()
                .flat_map(|variant| {
                    branches
                        .iter()
                        .flat_map(|branch| variants_of(branch, defs, variant, depth + 1))
                })
                .collect();
        }
    }
    variants
}

/// Fields each tool computes to explain how an answer was produced, not the
/// answer itself. Dotted paths name nested fields. Confidence signals (e.g.
/// topology `completeness`/`confidence`, coverage totals) are not listed.
const fn debug_only_fields(tool: ToolId) -> &'static [&'static str] {
    match tool {
        ToolId::LocalSearch | ToolId::AstSearch => &["searchEngine", "filesScanned"],
        ToolId::LocalFetch => &[
            "modified",
            "sourceBytes",
            "returnedBytes",
            "selectedMatchCount",
        ],
        ToolId::StructureSearch => &["filesScanned"],
        ToolId::AstTopology => &["filesScanned"],
        ToolId::GhSearchHistory => &["effectiveQuery", "scope"],
        ToolId::GhGetHistoryItem => &["parents"],
        _ => &[],
    }
}

fn remove_path(data: &mut Map<String, Value>, path: &str) {
    match path.split_once('.') {
        Some((head, rest)) => {
            if let Some(child) = data.get_mut(head).and_then(Value::as_object_mut) {
                remove_path(child, rest);
            }
        }
        None => {
            data.remove(path);
        }
    }
}

fn is_pagination_key(key: &str) -> bool {
    key == "pagination" || key.ends_with("Pagination")
}

/// Search statistics: the listed files already carry the counts. Keep the
/// totals only while more pages exist, and the scan scope only when nothing
/// matched (it shows the search ran where intended).
fn minimize_stats(data: &mut Map<String, Value>, more: bool) {
    let has_rows = data
        .get("files")
        .and_then(Value::as_array)
        .is_some_and(|files| !files.is_empty());
    let Some(stats) = data.get_mut("stats").and_then(Value::as_object_mut) else {
        return;
    };
    if !has_rows {
        stats.retain(|key, _| {
            matches!(
                key.as_str(),
                "filesSearched" | "totalOccurrences" | "totalStructuralMatches"
            )
        });
        return;
    }
    if more {
        stats.retain(|key, _| {
            matches!(
                key.as_str(),
                "totalOccurrences" | "filesMatched" | "totalStructuralMatches"
            )
        });
    } else {
        data.remove("stats");
    }
}

fn evidence_kind(tool: ToolId, query: &Value, data: &Value) -> &'static str {
    match tool {
        ToolId::AstSearch => match query["operation"].as_str() {
            Some("match") => "structural",
            _ => "syntactic",
        },
        ToolId::AstTopology => "syntactic",
        ToolId::LspSearch => match data.pointer("/lsp/source").and_then(Value::as_str) {
            Some("native-graph-facts" | "markdown") => "syntactic",
            _ => "semantic",
        },
        ToolId::LocalSearch => "lexical",
        ToolId::ArtifactSearch
        | ToolId::Clasify
        | ToolId::GhSearchRepo
        | ToolId::GhSearchCode
        | ToolId::GhStructure
        | ToolId::GhGetFileContent
        | ToolId::GhSearchHistory
        | ToolId::GhGetHistoryItem
        | ToolId::GhCloneRepo => "provider",
        ToolId::LocalFetch | ToolId::StructureSearch | ToolId::AstRewrite => "exact",
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

/// Row `errorCode`s that reject the caller's input (CLI exit 2): the request
/// is wrong, so broadening or retrying it cannot help.
pub fn is_invalid_input_code(code: &str) -> bool {
    matches!(
        code,
        "invalidInput" | "invalidQuery" | "invalidRegex" | "invalid_query" | "validation"
    ) || code.ends_with(".input.invalid")
        || code.ends_with(".query.invalidPattern")
}

/// Row `errorCode`s that mean the requested local path does not exist
/// (CLI exit 3, like a GitHub not-found).
pub fn is_not_found_code(code: &str) -> bool {
    code == "pathNotFound" || code.ends_with(".policy.notFound")
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
pub fn attach_query_base(value: &mut Value, tool: ToolId, query: &Value) {
    let has_error = value["results"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["status"] == "error"));
    if !matches!(
        tool,
        ToolId::StructureSearch | ToolId::AstSearch | ToolId::AstTopology | ToolId::LocalSearch
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
        ToolId::AstTopology => {
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
        ToolId::LocalSearch => {
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

/// Batch rows from queries with different roots carry paths relative to their
/// own root, so no shared `base` can resolve them. Make each row's relative
/// paths absolute from its own query first; the envelope then derives one
/// common base for every row.
pub fn absolutize_row_paths(rows: &mut [Value], tool: ToolId, queries: &[Value]) {
    if !matches!(tool, ToolId::StructureSearch | ToolId::AstSearch) {
        return;
    }
    for (position, row) in rows.iter_mut().enumerate() {
        if row["status"] == "error" {
            continue;
        }
        let index = row["index"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .unwrap_or(position);
        let Some(path) = queries
            .get(index)
            .and_then(|query| query.get("path"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Ok(canonical) = std::fs::canonicalize(path) else {
            continue;
        };
        if let Some(root) = canonical.parent() {
            prefix_relative_paths(&mut row["data"], 0, &root.to_string_lossy());
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

fn can_share_field(key: &str, value: &Value) -> bool {
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
    !EXCLUDED.contains(&key)
        && (value.is_number()
            || value.is_boolean()
            || value.as_str().is_some_and(|s| !s.is_empty()))
}

fn shared_leaves_mut(rows: &mut [Value]) -> impl Iterator<Item = &mut Map<String, Value>> {
    rows.iter_mut()
        .filter_map(|row| row["data"].as_object_mut())
        .flat_map(|data| data.values_mut().filter_map(Value::as_array_mut))
        .flatten()
        .filter_map(Value::as_object_mut)
}

/// Restore the canonical evidence view for validation without changing the
/// compact response returned to the caller. Explicit leaf values take priority.
pub(crate) fn restore_shared_fields(output: &mut Value) {
    let shared = match output.get("shared").and_then(Value::as_object) {
        Some(shared) => shared.clone(),
        None => return,
    };
    let Some(rows) = output.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for leaf in shared_leaves_mut(rows) {
        for (key, value) in &shared {
            if can_share_field(key, value) {
                leaf.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

fn hoist_shared_fields(rows: &mut [Value]) -> Option<Map<String, Value>> {
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
            can_share_field(key, value) && leaves.iter().all(|leaf| leaf.get(*key) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if shared.is_empty() {
        return None;
    }
    for leaf in shared_leaves_mut(rows) {
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

    #[test]
    fn minimal_rows_keep_every_field_the_contract_requires() {
        let history = contract_data_variants(ToolId::GhSearchHistory);
        assert!(
            history
                .iter()
                .all(|variant| variant.contains("type") || variant.contains("error")),
            "{history:?}"
        );
        let clone = contract_data_variants(ToolId::GhCloneRepo);
        assert!(
            clone
                .iter()
                .any(|variant| variant.contains("owner") && variant.contains("repo")),
            "{clone:?}"
        );
        let row = minimized(
            ToolId::GhSearchHistory,
            json!({"operation":"issue","owner":"o","repo":"r"}),
            json!({"type":"issues","owner":"o","repo":"r","issues":[],"effectiveQuery":"is:issue"}),
        );
        assert_eq!(row["data"]["type"], "issues", "{row}");
        assert!(row["data"].get("effectiveQuery").is_none(), "{row}");
        let clone_row = minimized(
            ToolId::GhCloneRepo,
            json!({"owner":"o","repo":"r"}),
            json!({"owner":"o","repo":"r","totalSize":1,"location":{"localPath":"/x"}}),
        );
        assert_eq!(clone_row["data"]["owner"], "o", "{clone_row}");
    }

    fn minimized(tool: ToolId, query: Value, data: Value) -> Value {
        let mut row = json!({"index":0,"data":data,"cache":1});
        minimize_row(&mut row, tool, &query);
        row
    }

    #[test]
    fn minimal_rows_drop_what_asserts_nothing_and_keep_what_continues() {
        let row = minimized(
            ToolId::LocalSearch,
            json!({"path":"/r","searchText":"x"}),
            json!({"files":[{"path":"a.rs"}],"stats":{"totalOccurrences":1,"filesSearched":9},
                "pagination":{"currentPage":1,"totalPages":1,"hasMore":false},
                "snapshot":"s","searchEngine":"rg","truncated":false,
                "diagnostics":[{"severity":"info","message":"routine"},{"severity":"warning","message":"keep"}]}),
        );
        assert!(row.get("cache").is_none(), "{row}");
        let data = &row["data"];
        for gone in [
            "stats",
            "pagination",
            "snapshot",
            "searchEngine",
            "truncated",
        ] {
            assert!(data.get(gone).is_none(), "{gone}: {row}");
        }
        assert_eq!(
            data["diagnostics"],
            json!([{"severity":"warning","message":"keep"}])
        );
        assert_eq!(data["files"][0]["path"], "a.rs");
    }

    #[test]
    fn topology_coverage_diagnostics_survive_minimal_rows() {
        // nextDiagnostics pages exist to return these entries; they explain
        // graph gaps (unlinked imports, unsupported layouts).
        let row = minimized(
            ToolId::AstTopology,
            json!({"analysis":"dependents","path":"/r","file":"a.rs"}),
            json!({"results":[],"filesScanned":9,"coverage":{
                "diagnostics":[{"code":"unlinkedImport","file":"b.rs"}],
                "diagnosticsPagination":{"currentPage":1,"totalPages":2,"hasMore":true}}}),
        );
        assert_eq!(
            row["data"]["coverage"]["diagnostics"][0]["file"], "b.rs",
            "{row}"
        );
        assert!(row["data"].get("filesScanned").is_none(), "{row}");
    }

    #[test]
    fn open_pages_keep_pagination_and_totals() {
        let row = minimized(
            ToolId::LocalSearch,
            json!({}),
            json!({"files":[{"path":"a.rs"}],"stats":{"totalOccurrences":40,"filesMatched":9,"bytesSearched":7},
                "pagination":{"currentPage":1,"hasMore":true},"next":{"nextPage":{"tool":"localSearch","query":{"snapshot":"s"}}}}),
        );
        let data = &row["data"];
        assert_eq!(data["pagination"]["hasMore"], true);
        assert_eq!(
            data["stats"],
            json!({"totalOccurrences":40,"filesMatched":9})
        );
        assert_eq!(data["next"]["nextPage"]["query"]["snapshot"], "s");
        let later = minimized(
            ToolId::LocalSearch,
            json!({}),
            json!({"files":[],"pagination":{"currentPage":3,"hasMore":false}}),
        );
        assert_eq!(
            later["data"]["pagination"]["currentPage"], 3,
            "last page stays located"
        );
    }

    #[test]
    fn empty_searches_keep_their_scan_scope() {
        let row = minimized(
            ToolId::LocalSearch,
            json!({}),
            json!({"stats":{"totalOccurrences":0,"filesSearched":176,"bytesSearched":9}}),
        );
        assert_eq!(
            row["data"]["stats"],
            json!({"totalOccurrences":0,"filesSearched":176})
        );
    }

    #[test]
    fn tool_declared_fields_leave_confidence_signals() {
        let topology = minimized(
            ToolId::AstTopology,
            json!({"analysis":"dependencies"}),
            json!({"analysis":"dependencies","confidence":"low",
                "coverage":{"basis":"syntactic","diagnosticCounts":{"x":5},"diagnostics":[{"file":"a"}]},
                "completeness":{"graph":"coverage-incomplete"}}),
        );
        let data = &topology["data"];
        assert!(data.get("analysis").is_none(), "request echo: {topology}");
        assert_eq!(data["confidence"], "low");
        assert_eq!(
            data["coverage"],
            json!({"basis":"syntactic","diagnosticCounts":{"x":5},"diagnostics":[{"file":"a"}]})
        );
        assert_eq!(data["completeness"]["graph"], "coverage-incomplete");
        let fetch = minimized(
            ToolId::LocalFetch,
            json!({}),
            json!({"content":"x","modified":"t","sourceBytes":1,"totalLines":9}),
        );
        assert_eq!(fetch["data"], json!({"content":"x","totalLines":9}));
    }

    #[test]
    fn echoes_go_only_when_they_equal_the_request_and_debug_keeps_all() {
        let data = json!({"owner":"o","repo":"other","files":[]});
        let row = minimized(
            ToolId::GhGetFileContent,
            json!({"owner":"o","repo":"r"}),
            data.clone(),
        );
        assert!(row["data"].get("owner").is_none());
        assert_eq!(
            row["data"]["repo"], "other",
            "a different repo is information"
        );
        let debug = minimized(
            ToolId::GhGetFileContent,
            json!({"owner":"o","debug":true}),
            data.clone(),
        );
        assert_eq!(debug["data"], data);
        let mut error = json!({"index":0,"status":"error","data":{"error":"x","snapshot":"s"}});
        minimize_row(&mut error, ToolId::LocalSearch, &json!({}));
        assert_eq!(error["data"]["snapshot"], "s", "error rows stay whole");
    }

    #[test]
    fn mixed_root_batch_rows_share_one_resolvable_base() {
        let dir = tempfile::tempdir().expect("fixture");
        let canonical = std::fs::canonicalize(dir.path()).expect("canonical root");
        let src = canonical.join("src");
        let nested = src.join("runtime");
        std::fs::create_dir_all(&nested).expect("dirs");
        let queries = vec![
            json!({"path": src.to_string_lossy()}),
            json!({"path": nested.to_string_lossy()}),
        ];
        let mut rows = vec![
            json!({"index":0,"data":{"files":[{"path":"src/a.rs"}]}}),
            json!({"index":1,"data":{"declarations":[{"path":"runtime/b.rs"}]}}),
        ];
        absolutize_row_paths(
            &mut rows,
            ToolId::from_name("astSearch").expect("known tool"),
            &queries,
        );
        let value = envelope(rows);
        let base = value["base"].as_str().expect("common base");
        let first = value["results"][0]["data"]["files"][0]["path"]
            .as_str()
            .unwrap();
        let second = value["results"][1]["data"]["declarations"][0]["path"]
            .as_str()
            .unwrap();
        assert_eq!(
            std::path::Path::new(base).join(first),
            canonical.join("src/a.rs")
        );
        assert_eq!(
            std::path::Path::new(base).join(second),
            src.join("runtime/b.rs")
        );
    }

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
        apply_hint_policy(
            &mut row,
            ToolId::from_name(tool).expect("known tool"),
            query,
        );
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
        let query = json!({"owner":"o","keywords":["octocode"]});
        assert!(
            error_hint("ghSearchCode", &query, "rateLimited", "slow down").contains("Retry-After")
        );
        // A code merely containing "auth" is not an authentication failure.
        assert_eq!(
            error_hint("ghSearchCode", &query, "authorizationPending", "x"),
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
    }

    #[test]
    fn native_language_server_results_are_semantic_evidence() {
        assert_eq!(
            evidence_kind(
                ToolId::LspSearch,
                &json!({}),
                &json!({"lsp": {"source": "native"}}),
            ),
            "semantic"
        );
        assert_eq!(
            evidence_kind(
                ToolId::LspSearch,
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
        attach_query_base(
            &mut output,
            ToolId::from_name("localSearch").expect("known tool"),
            &json!({"path": root_str}),
        );
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
        attach_query_base(
            &mut topology,
            ToolId::from_name("astTopology").expect("known tool"),
            &json!({"path": root_str}),
        );
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
            ToolId::from_name("astSearch").expect("known tool"),
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
            ToolId::from_name("lspSearch").expect("known tool"),
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
            ToolId::from_name("localSearch").expect("known tool"),
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
            ToolId::from_name("localFetch").expect("known tool"),
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
            ToolId::from_name("ghSearchHistory").expect("known tool"),
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
            ToolId::from_name("astSearch").expect("known tool"),
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
            ToolId::from_name("localFetch").expect("known tool"),
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
            ToolId::from_name("ghGetFileContent").expect("known tool"),
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
            ToolId::from_name("ghGetHistoryItem").expect("known tool"),
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
            ToolId::from_name("localSearch").expect("known tool"),
            0,
            &json!({"debug":false}),
            json!({"files":[]}),
            None,
        );
        assert!(normal.get("meta").is_none());

        let debug = result_row(
            ToolId::from_name("localSearch").expect("known tool"),
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
            ToolId::from_name("localFetch").expect("known tool"),
            0,
            &json!({"goal": "test", "reasoning":"Read the next exact page.","debug":false}),
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
        // The brief is inherited (followUp), not copied onto every page.
        assert_eq!(row.pointer("/data/next/continue/query/reasoning"), None);
        assert_eq!(
            row.pointer("/data/next/continue/query/debug"),
            Some(&json!(false))
        );
    }

    #[test]
    fn outer_clasify_metadata_does_not_overwrite_nested_continuation_ownership() {
        let row = result_row(
            ToolId::from_name("clasify").expect("known tool"),
            0,
            &json!({"goal": "test", "reasoning":"Evaluate captured evidence.","debug":false}),
            json!({
                "context": {
                    "next": {
                        "continue": {
                            "tool": "localFetch",
                            "query": {
                                "path":"/repo/a.rs",
                                "offset":2,
                                "goal": "test", "reasoning":"Read the next exact page.",
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
        let query = json!({"owner":"o","keywords":["octocode"]});
        let mut timeout = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"timed out","errorCode":"timeout"}
        });
        apply_hint_policy(
            &mut timeout,
            ToolId::from_name("ghSearchCode").expect("known tool"),
            &query,
        );
        let hint = timeout["data"]["hints"][0].as_str().expect("hint");
        assert!(hint.contains("Retry once"), "{timeout}");
        assert!(!hint.contains("Broaden keywords"), "{timeout}");

        let mut permission = json!({
            "index": 0,
            "status": "error",
            "data": {"error":"forbidden","errorCode":"permission"}
        });
        apply_hint_policy(
            &mut permission,
            ToolId::from_name("ghSearchCode").expect("known tool"),
            &query,
        );
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
        apply_hint_policy(
            &mut owned,
            ToolId::from_name("ghSearchCode").expect("known tool"),
            &query,
        );
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
            ToolId::from_name("localFetch").expect("known tool"),
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
    fn invalid_input_and_missing_path_errors_do_not_suggest_broadening() {
        for (tool, code, expected) in [
            (
                "structureSearch",
                "invalidInput",
                "Correct the rejected field",
            ),
            (
                "structureSearch",
                "structure.input.invalid",
                "Correct the rejected field",
            ),
            ("ghSearchRepo", "validation", "Correct the rejected field"),
            (
                "structureSearch",
                "structure.policy.notFound",
                "Verify the path exists",
            ),
            ("localSearch", "pathNotFound", "Verify the path exists"),
            (
                "structureSearch",
                "structure.policy.outsideAllowedRoots",
                "allowed roots",
            ),
        ] {
            let mut row = json!({
                "index": 0,
                "status": "error",
                "data": {"error":"failed","errorCode":code}
            });
            apply_hint_policy(
                &mut row,
                ToolId::from_name(tool).expect("known tool"),
                &json!({"path":"/repo/nope"}),
            );
            let hint = row["data"]["hints"][0].as_str().expect("hint");
            assert!(hint.contains(expected), "{code}: {hint}");
            assert!(!hint.contains("Broaden"), "{code}: {hint}");
        }
        assert!(is_invalid_input_code("invalidRegex"));
        assert!(!is_invalid_input_code("fileAccessFailed"));
        assert!(is_not_found_code("structure.policy.notFound"));
        assert!(!is_not_found_code("fileAccessFailed"));
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
            apply_hint_policy(
                &mut row,
                ToolId::from_name("astSearch").expect("known tool"),
                &query,
            );
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
            walk_threads: None,
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
