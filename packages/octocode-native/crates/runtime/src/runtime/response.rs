//! Transport-neutral structured result metadata and lossless path compaction.
use crate::policy::path::PathPolicy;
use crate::tools::id::ToolId;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

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
            "If a match was expected, write the pattern as a complete node with its body, check a tree view, then broaden path or filters."
        }
        ToolId::AstSearch => "Broaden the syntax/name query, path, or filters.",
        ToolId::AstTopology => "Inspect diagnostics, then broaden the graph scope if needed.",
        ToolId::AstRewrite => "Broaden the structural pattern, path, or file filters.",
        ToolId::LocalFetch => "Verify path/range, or remove matchString.",
        ToolId::LspSearch => "Refresh uri/symbolName/lineHint, or broaden workspaceRoot.",
    }
}

/// Roots are widened only from a trusted source, so the hint names where the
/// setting is read: a workspace `.env` or `.octocoderc` is ignored for it.
pub(crate) const SANDBOX_HINT: &str = "Outside allowed roots: add the dir to ALLOWED_PATHS (comma list) in the env or ~/.octocode/.env, not a workspace file.";

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
                // A one-row call continuation carries the setting on its row.
                let rows: Vec<&mut Value> =
                    match next_query.get_mut("queries").and_then(Value::as_array_mut) {
                        Some(rows) => rows.iter_mut().collect(),
                        None => vec![&mut next_query],
                    };
                for row in rows {
                    if let Some(row) = row.as_object_mut() {
                        for field in ["debug"] {
                            if let Some(value) = original_query.get(field) {
                                row.insert(field.into(), value.clone());
                            }
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
            "mainGoal",
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
    // `meta` is verbose: the verbose stage drops it unless the row asked
    // for `debug: true`.
    let mut row = json!({"index":index,"meta":meta,"data":data});
    if let Some(status) = status {
        row["status"] = json!(status);
    }
    row
}

/// Default responses carry the answer and what the next call needs. Fields
/// core classes verbose are already gone (`super::verbose`); the
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
        .filter(|key| is_removable(key))
        .filter_map(|key| data.get(key).map(|value| (key.clone(), value.clone())))
        .collect::<std::collections::BTreeMap<_, _>>();
    minimize_data(data, query);
    if tool.is_github() {
        super::github_output::compact(tool, &mut row["data"], query);
    }
    let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
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
fn is_removable(key: &str) -> bool {
    is_pagination_key(key)
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

fn minimize_data(data: &mut Map<String, Value>, query: &Value) {
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
    // repeats the message's first line. A committer that differs in any
    // field (date or email after a rebase, cherry-pick or web merge) is
    // evidence and stays.
    if data.get("committer").is_some() && data.get("committer") == data.get("author") {
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
    // Per tool: a call builds only its own tool's variants (see
    // `contracts::tool_contract`), never the whole contract's.
    static VARIANTS: [std::sync::OnceLock<Variants>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    let Some(index) = ToolId::ALL.iter().position(|id| *id == tool) else {
        return &[];
    };
    VARIANTS[index].get_or_init(|| {
        let Ok(tool) = crate::contracts::tool_contract(tool) else {
            return Vec::new();
        };
        let schema = &tool["outputSchema"];
        let defs = &schema["$defs"];
        let rows = resolve_ref(&schema["properties"]["results"]["items"], defs);
        let data = resolve_ref(&rows["properties"]["data"], defs);
        variants_of(data, defs, &Default::default(), 0)
    })
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

fn is_pagination_key(key: &str) -> bool {
    key == "pagination" || key.ends_with("Pagination")
}

/// Search statistics: the listed files already carry the counts. Keep the
/// totals only while more pages exist, and the scan scope only when nothing
/// matched (it shows the search ran where intended). Coverage limits and
/// unreadable-path errors remain public even on the final page: they explain
/// evidence that was not returned.
fn minimize_stats(data: &mut Map<String, Value>, more: bool) {
    let has_rows = data
        .get("files")
        .and_then(Value::as_array)
        .is_some_and(|files| !files.is_empty());
    let Some(stats) = data.get_mut("stats").and_then(Value::as_object_mut) else {
        return;
    };
    let capped = stats.get("capped").and_then(Value::as_bool) == Some(true);
    let cap_reason = stats
        .get("capReason")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.trim().is_empty());
    let unreadable = stats
        .get("errorCount")
        .and_then(Value::as_u64)
        .is_some_and(|count| count > 0);
    let keep_cap = |key: &str| {
        (key == "capped" && capped)
            || (key == "capReason" && cap_reason)
            || (unreadable && matches!(key, "errorCount" | "firstError"))
    };
    if !has_rows {
        stats.retain(|key, _| {
            matches!(
                key.as_str(),
                "filesSearched" | "totalOccurrences" | "totalStructuralMatches"
            ) || keep_cap(key)
        });
        return;
    }
    if more {
        stats.retain(|key, _| {
            matches!(
                key.as_str(),
                "totalOccurrences" | "filesMatched" | "totalStructuralMatches"
            ) || keep_cap(key)
        });
    } else {
        stats.retain(|key, _| {
            keep_cap(key)
                || ((capped || cap_reason || unreadable)
                    && matches!(
                        key.as_str(),
                        "totalOccurrences" | "filesMatched" | "totalStructuralMatches"
                    ))
        });
        if stats.is_empty() {
            data.remove("stats");
        }
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
        "invalidInput"
            | "invalidQuery"
            | "invalidRegex"
            | "invalid_query"
            | "validation"
            | "lsp.invalidQuery"
    ) || [
        ".input.invalid",
        ".query.invalid",
        ".query.invalidPattern",
        ".query.compileFailed",
        ".policy.invalidInput",
        ".options.invalidLimit",
        ".rewrite.invalid",
        ".language.required",
        ".language.unsupported",
        ".language.mismatch",
        ".language.fileRequired",
        ".language.directoryRequired",
        ".language.invalidGlob",
    ]
    .iter()
    .any(|suffix| code.ends_with(suffix))
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

/// `next.*` name prefixes (lowercased) that page more of the same result.
const PAGE_CONTINUATION_PREFIXES: &[&str] = &["next", "continue"];
/// Prefixes that resume the same result under a wider bound.
const RESUME_CONTINUATION_PREFIXES: &[&str] = &["expand", "retry"];
/// Recovery routes (restart, narrower or alternate strategy, drill-down read)
/// that satisfy the continuation contract for a bounded row but do not mean
/// more of this result remains.
const RECOVERY_CONTINUATION_PREFIXES: &[&str] =
    &["restart", "narrow", "fallback", "escalate", "read"];

fn has_prefix(name: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| name.starts_with(prefix))
}

/// A `next.*` name (any case) that pages or resumes this result: more of it
/// remains (CLI exit 6). Drill-downs (`get*`, `read*`, `verify*`) and recovery
/// routes are optional follow-ups on a complete result and do not count.
pub fn is_remaining_continuation_name(name: &str) -> bool {
    let name = name.to_lowercase();
    has_prefix(&name, PAGE_CONTINUATION_PREFIXES) || has_prefix(&name, RESUME_CONTINUATION_PREFIXES)
}

/// A `next.*` name (any case) that continues this result after the evidence
/// a row shows (`next*`, `continue*`): following it from an earlier part of
/// a split row would skip the parts between, so it rides the row's last part.
pub fn is_page_continuation_name(name: &str) -> bool {
    has_prefix(&name.to_lowercase(), PAGE_CONTINUATION_PREFIXES)
}

/// Whether `value` holds, at any depth, an executable `{tool, query}` call
/// under a key whose lowercased name satisfies `key_matches`.
pub fn continuation(value: &Value, key_matches: &impl Fn(&str) -> bool) -> bool {
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
    let page = continuation(data, &|k| has_prefix(k, PAGE_CONTINUATION_PREFIXES));
    let expansion = continuation(data, &|k| {
        has_prefix(k, RESUME_CONTINUATION_PREFIXES)
            || has_prefix(k, RECOVERY_CONTINUATION_PREFIXES)
            || k.contains("search")
            || k.contains("completeness")
    });
    if (pageable && !page)
        || ((tree_some(data, &bounded) || partial) && !page && !expansion)
        || clipped_value_unreached(data)
    {
        vec!["continuationMissing".into()]
    } else {
        vec![]
    }
}

/// Whether a value clipped inside a file row (`truncated:true` under a row
/// with a `path`) has no continuation reaching it: neither a read or
/// expansion of that file nor one that widens values (`matchContentLength`).
/// Page continuations (`next*`, `continue*`) list more rows and never widen a
/// shown one.
fn clipped_value_unreached(data: &Value) -> bool {
    fn clipped_paths<'a>(value: &'a Value, path: Option<&'a str>, out: &mut Vec<&'a str>) {
        match value {
            Value::Object(map) => {
                let path = map.get("path").and_then(Value::as_str).or(path);
                if map.get("truncated") == Some(&Value::Bool(true))
                    && let Some(path) = path
                {
                    out.push(path);
                }
                for (key, child) in map {
                    if key != "next" {
                        clipped_paths(child, path, out);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| clipped_paths(item, path, out)),
            _ => {}
        }
    }
    fn reaching<'a>(value: &'a Value, paths: &mut Vec<&'a str>, widened: &mut bool) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if let Some(query) = child.get("query").and_then(Value::as_object)
                        && child.get("tool").is_some_and(Value::is_string)
                        && !has_prefix(&key.to_lowercase(), PAGE_CONTINUATION_PREFIXES)
                    {
                        *widened |= query.contains_key("matchContentLength");
                        if let Some(path) = query.get("path").and_then(Value::as_str) {
                            paths.push(path);
                        }
                    }
                    reaching(child, paths, widened);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| reaching(item, paths, widened)),
            _ => {}
        }
    }
    let mut clipped = Vec::new();
    clipped_paths(data, None, &mut clipped);
    if clipped.is_empty() {
        return false;
    }
    let (mut targets, mut widened) = (Vec::new(), false);
    reaching(data, &mut targets, &mut widened);
    // Rows name files absolutely here; a continuation keeps the caller's
    // (workspace-relative) spelling of the same file.
    let same_file = |row: &str, target: &str| {
        let target = target.trim_start_matches("./");
        row == target
            || row
                .strip_suffix(target)
                .is_some_and(|prefix| prefix.ends_with('/'))
    };
    !widened
        && clipped
            .iter()
            .any(|path| !targets.iter().any(|target| same_file(path, target)))
}

/// Tools whose rows name local files: a relative row path must resolve as-is
/// wherever a local tool takes a `path` (against the workspace root).
/// Every local-family tool except astTopology (anchored separately: fields
/// stay relative to the scanned directory, its `base`) and the CLI-only
/// astRewrite, whose edit rows are emitted as-is.
fn names_local_files(tool: ToolId) -> bool {
    tool.is_local() && !matches!(tool, ToolId::AstTopology | ToolId::AstRewrite)
}

/// Build the public envelope for executed rows; `queries` maps each row
/// position to its validated query (`None` for a rejected row).
///
/// Local file paths are anchored on the workspace root: a path inside it is
/// emitted relative to it, so localFetch and localSearch resolve a copied row
/// path as-is, and `base` is that root; a path outside it stays absolute.
/// structureSearch and astSearch name files relative to the parent of their
/// own query root, so those rows are made absolute first. astTopology fields
/// stay relative to the scanned directory, which becomes `base`.
pub fn envelope_in(
    mut rows: Vec<Value>,
    tool: ToolId,
    queries: &[Option<&Value>],
    paths: &PathPolicy,
) -> Value {
    let roots: Vec<Option<PathBuf>> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| {
            if row["status"] == "error" {
                return None;
            }
            let index = row["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(position);
            let path = queries
                .get(index)
                .copied()
                .flatten()?
                .get("path")?
                .as_str()?;
            paths.validate(path).ok().map(|valid| valid.canonical)
        })
        .collect();
    if tool == ToolId::AstTopology {
        let single = roots
            .first()
            .cloned()
            .flatten()
            .filter(|root| roots.iter().all(|other| other.as_ref() == Some(root)));
        let mut value = envelope(rows);
        if let Some(root) = single {
            let workspace = paths
                .validate(".")
                .ok()
                .map(|valid| valid.canonical.to_string_lossy().into_owned());
            anchor_topology(&mut value, &root, workspace.as_deref());
        }
        return value;
    }
    if !names_local_files(tool) {
        return envelope(rows);
    }
    if matches!(tool, ToolId::StructureSearch | ToolId::AstSearch) {
        for (row, root) in rows.iter_mut().zip(&roots) {
            if let Some(parent) = root.as_deref().and_then(Path::parent) {
                let parent = parent.to_string_lossy();
                prefix_relative_paths(&mut row["data"], 0, &parent);
                if tool == ToolId::StructureSearch {
                    anchor_listing_dirs(&mut row["data"], &parent);
                }
            }
        }
    }
    let workspace = paths
        .validate(".")
        .ok()
        .map(|valid| valid.canonical.to_string_lossy().into_owned())
        .filter(|root| Path::new(root).parent().is_some());
    if let Some(workspace) = workspace.as_deref() {
        for row in &mut rows {
            relativize_continuation_paths(&mut row["data"], workspace, 0);
        }
    }
    let heads = checked_out_heads(tool, &roots);
    let mut value = compact(rows, workspace);
    if tool == ToolId::StructureSearch {
        relativize_listing_dirs(&mut value);
    }
    attach_heads(&mut value, &heads);
    value
}

/// HEAD of each row's root for the tools that read the working tree
/// directly; empty for every other tool. A root is read once per response.
fn checked_out_heads(tool: ToolId, roots: &[Option<PathBuf>]) -> Vec<Option<String>> {
    if !matches!(
        tool,
        ToolId::LocalSearch | ToolId::LocalFetch | ToolId::StructureSearch
    ) {
        return Vec::new();
    }
    let mut seen = std::collections::HashMap::<&Path, Option<String>>::new();
    roots
        .iter()
        .map(|root| {
            let root = root.as_deref()?;
            seen.entry(root)
                .or_insert_with(|| super::git_head::head_sha(root))
                .clone()
        })
        .collect()
}

/// Report the checked-out commit once: `shared.commitSha` when every row
/// with a root reads the same commit, else `commitSha` on each row that has
/// one (roots in different repositories, or some outside any repository).
fn attach_heads(value: &mut Value, heads: &[Option<String>]) {
    let Some(first) = heads.iter().flatten().next() else {
        return;
    };
    let Some(rows) = value["results"].as_array_mut() else {
        return;
    };
    let shared = rows.iter().zip(heads).all(|(row, head)| {
        head.as_ref() == Some(first) || (head.is_none() && row["status"] == "error")
    });
    if shared {
        let first = first.clone();
        match value.get_mut("shared").and_then(Value::as_object_mut) {
            Some(map) => {
                map.insert("commitSha".into(), json!(first));
            }
            None => value["shared"] = json!({ "commitSha": first }),
        }
        return;
    }
    for (row, head) in rows.iter_mut().zip(heads) {
        if let (Some(head), Some(data)) = (head, row["data"].as_object_mut()) {
            data.insert("commitSha".into(), json!(head));
        }
    }
}

/// Query fields that name a local file or directory in a continuation and
/// resolve against the workspace root. lspSearch `uri`/`workspaceRoot` stay
/// absolute: a `uri` is also a file URI, and callers check it as a file.
const CONTINUATION_PATH_FIELDS: [&str; 1] = ["path"];

/// Spell every absolute local path inside a continuation query (`{tool,
/// query}` under `next`, `hints`, or a row) relative to the workspace root,
/// which relative paths resolve against: the same file, without repeating
/// the root the response already names as `base`. Paths outside the
/// workspace, `file://` uris and non-local fields stay as they are.
fn relativize_continuation_paths(value: &mut Value, workspace: &str, depth: usize) {
    if depth > 12 {
        return;
    }
    match value {
        Value::Object(map) => {
            if map.get("tool").is_some_and(Value::is_string)
                && let Some(Value::Object(query)) = map.get_mut("query")
            {
                for field in CONTINUATION_PATH_FIELDS {
                    if let Some(Value::String(path)) = query.get_mut(field)
                        && let Some(relative) = workspace_relative(path, workspace)
                    {
                        *path = relative;
                    }
                }
                return;
            }
            for child in map.values_mut() {
                relativize_continuation_paths(child, workspace, depth + 1);
            }
        }
        Value::Array(items) => {
            for item in items {
                relativize_continuation_paths(item, workspace, depth + 1);
            }
        }
        _ => {}
    }
}

/// `path` relative to `workspace` (`.` for the root itself) when it is an
/// absolute path under it.
fn workspace_relative(path: &str, workspace: &str) -> Option<String> {
    if path == workspace {
        return Some(".".to_owned());
    }
    path.strip_prefix(workspace)?
        .strip_prefix('/')
        .filter(|rest| !rest.is_empty())
        .map(str::to_owned)
}

/// structureSearch `files` groups (`{dir, files}`) in a row's data.
fn listing_groups(data: &mut Value) -> impl Iterator<Item = &mut Map<String, Value>> {
    data.get_mut("files")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
        .filter(|group| group.get("files").is_some_and(Value::is_array))
}

/// A structureSearch group `dir` is relative to the walked root's parent
/// (`""` is that parent), like the rows' `path`: make it absolute.
fn anchor_listing_dirs(data: &mut Value, parent: &str) {
    for group in listing_groups(data) {
        if let Some(Value::String(dir)) = group.get_mut("dir")
            && !Path::new(dir.as_str()).is_absolute()
        {
            *dir = if dir.is_empty() {
                parent.to_owned()
            } else {
                format!("{parent}/{dir}")
            };
        }
    }
}

/// Name every structureSearch group `dir` like a row path: relative to the
/// response `base` under it (`.` for the base itself), else absolute, so
/// `base` + `dir` + entry name resolves each listed entry.
fn relativize_listing_dirs(value: &mut Value) {
    let base = value.get("base").and_then(Value::as_str).map(str::to_owned);
    for row in value
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        for group in listing_groups(&mut row["data"]) {
            let Some(Value::String(dir)) = group.get_mut("dir") else {
                continue;
            };
            let Some(base) = base.as_deref() else {
                continue;
            };
            if dir == base {
                *dir = ".".to_owned();
            } else if let Some(relative) = dir.strip_prefix(&format!("{base}/")) {
                *dir = relative.to_owned();
            }
        }
    }
}

/// astTopology fields (`file`, entrypoints, diagnostics) are relative to the
/// scanned directory (a file root's parent), which groups every row. Like
/// the other local tools, `base` is the workspace root and the row `path`
/// names that directory relative to it (`.` for the root itself), so `base`,
/// `path` and `file` joined resolve each file. A directory outside the
/// workspace becomes `base` itself, with `path` `.`.
fn anchor_topology(value: &mut Value, root: &Path, workspace: Option<&str>) {
    let Some(dir) = (if root.is_dir() {
        Some(root)
    } else {
        root.parent()
    }) else {
        return;
    };
    let dir_text = dir.to_string_lossy();
    let (base, display) = match workspace
        .and_then(|workspace| Some((workspace, workspace_relative(&dir_text, workspace)?)))
    {
        Some((workspace, relative)) => (workspace.to_owned(), relative),
        None => (dir_text.into_owned(), ".".to_owned()),
    };
    if let Some(rows) = value["results"].as_array_mut() {
        for row in rows {
            if row["data"].get("path").is_some_and(Value::is_string) {
                row["data"]["path"] = json!(display);
            }
            relativize_continuation_paths(&mut row["data"], &base, 0);
        }
    }
    value["base"] = json!(base);
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

pub fn envelope(rows: Vec<Value>) -> Value {
    let mut paths = Vec::new();
    for row in &rows {
        visit_paths(&row["data"], 0, &mut paths);
    }
    let base = common_directory(&paths);
    compact(rows, base)
}

/// Rewrite every absolute row path under `base` relative to it (the base
/// itself becomes `.`), hoist shared leaf fields, and wrap the rows. Paths
/// outside `base` stay absolute; without such a path there is no `base`.
fn compact(mut rows: Vec<Value>, base: Option<String>) -> Value {
    let base = base.filter(|base| {
        let inside = format!("{base}/");
        let mut paths = Vec::new();
        for row in &rows {
            visit_paths(&row["data"], 0, &mut paths);
        }
        paths
            .iter()
            .any(|path| path == base || path.starts_with(&inside))
    });
    for row in &mut rows {
        rewrite_paths(&mut row["data"], 0, base.as_deref());
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
        "dir",
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

/// Name every file location by `path`: relative to `base` under it, else
/// absolute (a `file://` uri outside `base` becomes its absolute path).
fn rewrite_paths(value: &mut Value, depth: usize, base: Option<&str>) {
    if depth > 8 {
        return;
    }
    match value {
        Value::Object(map) => {
            let file_uri = map
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| uri.starts_with("file://"));
            if let Some(path) = absolute_path(map).and_then(|p| {
                let relative = base.and_then(|base| {
                    if p == base {
                        Some(".".to_owned())
                    } else {
                        p.strip_prefix(&format!("{base}/")).map(str::to_owned)
                    }
                });
                relative.or_else(|| file_uri.then_some(p))
            }) {
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
        // A clone row requires its checkout, not the caller's own owner/repo.
        let clone = contract_data_variants(ToolId::GhCloneRepo);
        assert!(
            clone
                .iter()
                .any(|variant| variant.contains("location") && variant.contains("totalSize")),
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
            json!({"totalSize":1,"location":{"localPath":"/x"}}),
        );
        assert_eq!(
            clone_row["data"]["location"]["localPath"], "/x",
            "{clone_row}"
        );
        assert_eq!(clone_row["data"]["totalSize"], 1, "{clone_row}");
    }

    fn minimized(tool: ToolId, query: Value, data: Value) -> Value {
        let mut row = json!({"index":0,"data":data,"cache":1});
        // The engine order: the verbose stage, then the minimizer.
        super::super::verbose::prune_row(&mut row, tool, &query);
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

    /// A committer is removed only when it repeats the author whole; a
    /// different date or email (rebase, cherry-pick) and the parent list (a
    /// merge has two) are commit evidence.
    #[test]
    fn commit_rows_keep_a_differing_committer_and_the_parents() {
        let author = json!({"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"});
        let commit = |committer: Value| {
            minimized(
                ToolId::GhGetHistoryItem,
                json!({"operation":"commit","owner":"o","repo":"r","ref":"abc"}),
                json!({"type":"commit","sha":"abc","message":"m","author":author,
                    "committer":committer,"parents":["p1","p2"]}),
            )
        };
        let same = commit(author.clone());
        assert!(same["data"].get("committer").is_none(), "{same}");
        assert_eq!(same["data"]["parents"], json!(["p1", "p2"]), "{same}");
        let mut rebased = author.clone();
        rebased["date"] = json!("2024-02-01T00:00:00Z");
        let kept = commit(rebased.clone());
        assert_eq!(kept["data"]["committer"], rebased, "{kept}");
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
    fn capped_searches_keep_limit_diagnostics_on_first_final_and_empty_pages() {
        for (files, more) in [
            (json!([{ "path": "a.rs" }]), true),
            (json!([{ "path": "a.rs" }]), false),
            (json!([]), false),
        ] {
            let row = minimized(
                ToolId::LocalSearch,
                json!({ "debug": false }),
                json!({
                    "files": files,
                    "stats": {"capped": true, "capReason": "maxCollectedFiles", "totalOccurrences": 10002,
                        "filesMatched": 10002, "filesSearched": 10002, "bytesSearched": 70014},
                    "pagination": {"currentPage": if more { 1 } else { 20 }, "hasMore": more},
                    "isPartial": true,
                    "terminalLimit": !more,
                }),
            );
            assert_eq!(row["data"]["stats"]["capped"], true, "{row}");
            assert_eq!(
                row["data"]["stats"]["capReason"], "maxCollectedFiles",
                "{row}"
            );
            assert_eq!(row["data"]["isPartial"], true, "{row}");
            assert_eq!(row["data"]["stats"]["totalOccurrences"], 10002, "{row}");
            assert_eq!(row["data"]["terminalLimit"] == true, !more, "{row}");
            assert!(row["data"]["stats"].get("bytesSearched").is_none(), "{row}");
            crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
                .expect("minimized cap diagnostics obey the canonical output schema");
        }
    }

    /// The unreadable-path warning names `stats.firstError`: the failure
    /// count and first failure stay public on every page shape.
    #[test]
    fn unreadable_paths_keep_their_error_stats_on_every_page() {
        for (files, more) in [
            (json!([{ "path": "a.rs" }]), true),
            (json!([{ "path": "a.rs" }]), false),
            (json!([]), false),
        ] {
            let row = minimized(
                ToolId::LocalSearch,
                json!({ "debug": false }),
                json!({
                    "files": files,
                    "stats": {"totalOccurrences": 1, "filesMatched": 1, "filesSearched": 2,
                        "bytesSearched": 9, "errorCount": 1,
                        "firstError": "/r/b.txt: Permission denied (os error 13)"},
                    "pagination": {"currentPage": 1, "hasMore": more},
                    "warnings": ["1 path(s) could not be read (see stats.firstError), so absence is not proven."],
                    "isPartial": true,
                    "terminalLimit": !more,
                }),
            );
            let stats = &row["data"]["stats"];
            assert_eq!(stats["errorCount"], 1, "{row}");
            assert_eq!(
                stats["firstError"], "/r/b.txt: Permission denied (os error 13)",
                "{row}"
            );
            assert!(stats.get("bytesSearched").is_none(), "{row}");
            crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
                .expect("minimized error stats obey the canonical output schema");
        }
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
        // A GitHub read row is the same read: its byte accounting is debug-only too.
        let file = json!({"path":"a","content":"x","sourceBytes":1,"returnedBytes":1,"selectedMatchCount":1,"totalLines":9});
        let gh = minimized(
            ToolId::GhGetFileContent,
            json!({}),
            json!({"files":[file.clone()]}),
        );
        assert_eq!(
            gh["data"],
            json!({"files":[{"path":"a","content":"x","totalLines":9}]})
        );
        let debug = minimized(
            ToolId::GhGetFileContent,
            json!({"debug":true}),
            json!({"files":[file.clone()]}),
        );
        assert_eq!(debug["data"]["files"][0], file);
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
            SANDBOX_HINT.contains("~/.octocode/.env")
                && SANDBOX_HINT.contains("not a workspace file")
                && SANDBOX_HINT.chars().count() <= MAX_GUIDANCE_CHARS,
            "the hint names the trusted place to widen roots and is delivered uncut: {SANDBOX_HINT}"
        );
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

    /// A workspace with `packages/app/{src,tests}` and an allowed root
    /// outside it.
    struct Workspace {
        _dirs: (tempfile::TempDir, tempfile::TempDir),
        root: std::path::PathBuf,
        outside: std::path::PathBuf,
        paths: PathPolicy,
    }

    fn workspace() -> Workspace {
        let dir = tempfile::tempdir().expect("fixture");
        let other = tempfile::tempdir().expect("fixture");
        let root = std::fs::canonicalize(dir.path()).expect("root");
        let outside = std::fs::canonicalize(other.path()).expect("outside");
        for file in ["src/a.rs", "src/runtime/b.rs", "tests/x.ts"] {
            let path = root.join("packages/app").join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            std::fs::write(&path, "x").expect("file");
        }
        std::fs::write(outside.join("o.rs"), "x").expect("file");
        let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.join("packages/app")),
            additional_roots: vec![outside.clone()],
            include_home: false,
            home_dir: None,
        })
        .expect("policy");
        Workspace {
            _dirs: (dir, other),
            root: root.join("packages/app"),
            outside,
            paths,
        }
    }

    /// Every relative row path must be what localFetch resolves: the
    /// workspace root joined with the path.
    fn assert_resolvable(ws: &Workspace, value: &Value, path: &str) {
        assert_eq!(value["base"], ws.root.to_string_lossy().as_ref(), "{value}");
        assert!(ws.paths.validate(path).is_ok(), "{path} in {value}");
    }

    /// Continuation queries name local paths relative to the workspace root
    /// (`base`), which resolves them to the same files; outside paths stay.
    #[test]
    fn continuation_query_paths_are_workspace_relative() {
        let ws = workspace();
        let root = ws.root.to_string_lossy().into_owned();
        let outside = ws.outside.join("o.rs").to_string_lossy().into_owned();
        let row = json!({"index":0,"data":{
            "files":[{"path":format!("{root}/src/a.rs")}],
            "next":{"nextPage":{"tool":"localSearch","query":{"path":format!("{root}/src"),"searchText":"x","page":2}}},
            "hints":{"read":{"tool":"localFetch","query":{"path":outside.clone()}},
                     "refs":{"tool":"lspSearch","query":{"uri":format!("{root}/src/a.rs"),"workspaceRoot":root.clone()}}}
        }});
        let value = envelope_in(
            vec![row],
            ToolId::LocalSearch,
            &[Some(&json!({"path": format!("{root}/src")}))],
            &ws.paths,
        );
        let data = &value["results"][0]["data"];
        assert_eq!(data["next"]["nextPage"]["query"]["path"], "src", "{value}");
        assert_eq!(
            data["hints"]["read"]["query"]["path"],
            outside.as_str(),
            "{value}"
        );
        let refs = &data["hints"]["refs"]["query"];
        assert_eq!(refs["uri"], format!("{root}/src/a.rs").as_str(), "{value}");
        assert_eq!(refs["workspaceRoot"], root.as_str(), "{value}");
        assert_resolvable(&ws, &value, "src");
    }

    /// Local reads report the checked-out commit once per response: shared
    /// when every row reads the same repository, per row otherwise, and not
    /// at all outside a repository or for other tools.
    #[test]
    fn local_rows_report_the_checked_out_head_once() {
        let ws = workspace();
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let git = ws.root.join(".git");
        std::fs::create_dir_all(git.join("refs/heads")).expect("git");
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("head");
        std::fs::write(git.join("refs/heads/main"), format!("{sha}\n")).expect("ref");
        let root = ws.root.to_string_lossy().into_owned();
        let row = |index: u64| json!({"index":index,"data":{"files":[{"path":format!("{root}/src/a.rs")}]}});
        let src = json!({"path": format!("{root}/src")});
        let tests = json!({"path": format!("{root}/tests")});
        for tool in [ToolId::LocalSearch, ToolId::LocalFetch, ToolId::StructureSearch] {
            let value = envelope_in(vec![row(0), row(1)], tool, &[Some(&src), Some(&tests)], &ws.paths);
            assert_eq!(value["shared"]["commitSha"], sha, "{tool:?} {value}");
            assert!(value["results"][0]["data"].get("commitSha").is_none(), "{value}");
        }
        let outside = json!({"path": ws.outside.to_string_lossy()});
        let mixed = envelope_in(vec![row(0), row(1)], ToolId::LocalSearch, &[Some(&src), Some(&outside)], &ws.paths);
        assert!(mixed["shared"].get("commitSha").is_none(), "{mixed}");
        assert_eq!(mixed["results"][0]["data"]["commitSha"], sha, "{mixed}");
        assert!(mixed["results"][1]["data"].get("commitSha").is_none(), "{mixed}");
        let other = envelope_in(vec![row(0)], ToolId::AstSearch, &[Some(&src)], &ws.paths);
        assert!(other.get("shared").is_none(), "{other}");
        let plain = envelope_in(vec![row(0)], ToolId::LocalSearch, &[Some(&outside)], &ws.paths);
        assert!(plain.get("shared").is_none(), "{plain}");
    }

    #[test]
    fn structure_and_ast_rows_are_workspace_relative_for_any_query_root() {
        let ws = workspace();
        let root = ws.root.to_string_lossy().into_owned();
        // `.` is the workspace root itself: rows lead with its own name.
        let name = ws
            .root
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        for (tool, query, row, expected) in [
            (
                "structureSearch",
                json!({"path":"."}),
                json!({"path": name, "files":[{"path": format!("{name}/tests/x.ts")}]}),
                (".", "tests/x.ts"),
            ),
            (
                "structureSearch",
                json!({"path": format!("{root}/src")}),
                json!({"path":"src","files":[{"path":"src/runtime/b.rs"}]}),
                ("src", "src/runtime/b.rs"),
            ),
            (
                "astSearch",
                json!({"path":"src/runtime"}),
                json!({"path":"runtime","files":[{"path":"runtime/b.rs"}]}),
                ("src/runtime", "src/runtime/b.rs"),
            ),
            // A file root names the file relative to its parent.
            (
                "astSearch",
                json!({"path":"src/a.rs"}),
                json!({"path":"a.rs","files":[{"path":"a.rs"}]}),
                ("src/a.rs", "src/a.rs"),
            ),
        ] {
            let value = envelope_in(
                vec![json!({"index":0,"data":row})],
                ToolId::from_name(tool).expect("known tool"),
                &[Some(&query)],
                &ws.paths,
            );
            let data = &value["results"][0]["data"];
            assert_eq!(
                (
                    data["path"].as_str().unwrap(),
                    data["files"][0]["path"].as_str().unwrap()
                ),
                expected,
                "{tool} {query}"
            );
            assert_resolvable(&ws, &value, expected.1);
        }
    }

    /// structureSearch `files` groups name their `dir` like row paths, so
    /// `base` + `dir` + entry name resolves every listed entry, and a batch
    /// never hoists a `dir` into `shared`.
    #[test]
    fn structure_listing_dirs_resolve_against_base_for_any_query_root() {
        let ws = workspace();
        let outside = ws.outside.to_string_lossy().into_owned();
        let far_name = ws.outside.file_name().unwrap().to_string_lossy();
        let workspace = json!({"path":"."});
        let src = json!({"path":"src"});
        let file = json!({"path":"src/a.rs"});
        let far = json!({"path": outside});
        let rows = vec![
            json!({"index":0,"data":{"path":"app","files":[
                {"dir":"app","files":["./","src/"]},
                {"dir":"app/tests","files":["x.ts (1)"]}]}}),
            json!({"index":1,"data":{"path":"src","files":[{"dir":"src/runtime","files":["b.rs (1)"]}]}}),
            json!({"index":2,"data":{"path":"a.rs","files":[{"dir":"","files":["a.rs (1)"]}]}}),
            json!({"index":3,"data":{"path":far_name,"files":[{"dir":far_name,"files":["o.rs (1)"]}]}}),
            json!({"index":4,"data":{"path":"src","files":[{"dir":"src/runtime","files":["b.rs (1)"]}]}}),
        ];
        let value = envelope_in(
            rows,
            ToolId::StructureSearch,
            &[
                Some(&workspace),
                Some(&src),
                Some(&file),
                Some(&far),
                Some(&src),
            ],
            &ws.paths,
        );
        let dir = |row: usize, group: usize| {
            value["results"][row]["data"]["files"][group]["dir"]
                .as_str()
                .unwrap_or_else(|| panic!("dir {row}/{group} in {value}"))
                .to_owned()
        };
        assert_eq!(dir(0, 0), ".");
        assert_eq!(dir(0, 1), "tests");
        assert_eq!(dir(1, 0), "src/runtime");
        assert_eq!(dir(2, 0), "src");
        // Outside the workspace a directory stays absolute.
        assert_eq!(dir(3, 0), outside);
        assert_eq!(dir(4, 0), "src/runtime");
        let same = envelope_in(
            vec![
                json!({"index":0,"data":{"path":"src","files":[{"dir":"src","files":["a.rs (1)"]}]}}),
                json!({"index":1,"data":{"path":"src","files":[{"dir":"src","files":["a.rs (1)"]}]}}),
            ],
            ToolId::StructureSearch,
            &[Some(&src), Some(&src)],
            &ws.paths,
        );
        assert!(same.get("shared").is_none(), "{same}");
        assert_eq!(
            same["results"][1]["data"]["files"][0]["dir"], "src",
            "{same}"
        );
        for path in ["tests/x.ts", "src/runtime/b.rs", "src/a.rs"] {
            assert_resolvable(&ws, &value, path);
        }
        assert!(ws.paths.validate(format!("{outside}/o.rs")).is_ok());
    }

    #[test]
    fn mixed_root_batches_resolve_every_row_even_after_a_rejected_row() {
        let ws = workspace();
        let outside = ws.outside.to_string_lossy().into_owned();
        let src = json!({"path":"src"});
        let nested = json!({"path": ws.root.join("src/runtime").to_string_lossy()});
        let far = json!({"path": outside});
        let rows = vec![
            json!({"index":0,"data":{"files":[{"path":"src/a.rs"}]}}),
            json!({"index":1,"status":"error","data":{"error":"x","errorCode":"invalidInput"}}),
            json!({"index":2,"data":{"declarations":[{"path":"runtime/b.rs"}]}}),
            json!({"index":3,"data":{"files":[{"path": format!("{}/o.rs", ws.outside.file_name().unwrap().to_string_lossy())}]}}),
        ];
        let value = envelope_in(
            rows,
            ToolId::AstSearch,
            &[Some(&src), None, Some(&nested), Some(&far)],
            &ws.paths,
        );
        assert_resolvable(&ws, &value, "src/a.rs");
        assert_eq!(value["results"][0]["data"]["files"][0]["path"], "src/a.rs");
        assert_eq!(
            value["results"][2]["data"]["declarations"][0]["path"],
            "src/runtime/b.rs"
        );
        // Outside the workspace a path stays absolute.
        let far_path = value["results"][3]["data"]["files"][0]["path"]
            .as_str()
            .unwrap();
        assert_eq!(far_path, format!("{outside}/o.rs"));
        assert!(ws.paths.validate(far_path).is_ok());
    }

    #[test]
    fn local_search_and_lsp_rows_are_workspace_relative_and_outside_rows_absolute() {
        let ws = workspace();
        let root = ws.root.to_string_lossy().into_owned();
        let row = json!({"index":0,"data":{"files":[{"path": format!("{root}/src/runtime/b.rs")}],
            "next":{"nextPage":{"tool":"localSearch","query":{"path":"src/runtime"}}}}});
        let value = envelope_in(
            vec![row],
            ToolId::LocalSearch,
            &[Some(&json!({"path":"src/runtime"}))],
            &ws.paths,
        );
        assert_eq!(
            value["results"][0]["data"]["files"][0]["path"],
            "src/runtime/b.rs"
        );
        assert_eq!(
            value["results"][0]["data"]["next"]["nextPage"]["query"]["path"],
            "src/runtime"
        );
        assert_resolvable(&ws, &value, "src/runtime/b.rs");

        let lsp = json!({"index":0,"data":{"type":"documentSymbols","uri":format!("file://{root}/src/a.rs")}});
        let value = envelope_in(vec![lsp], ToolId::LspSearch, &[Some(&json!({}))], &ws.paths);
        assert_eq!(value["results"][0]["data"]["path"], "src/a.rs");
        assert_resolvable(&ws, &value, "src/a.rs");

        let outside = format!("{}/o.rs", ws.outside.to_string_lossy());
        let row = json!({"index":0,"data":{"files":[{"path": outside}]}});
        let value = envelope_in(
            vec![row],
            ToolId::LocalSearch,
            &[Some(&json!({"path": ws.outside.to_string_lossy()}))],
            &ws.paths,
        );
        assert!(value.get("base").is_none(), "{value}");
        assert_eq!(
            value["results"][0]["data"]["files"][0]["path"],
            outside.as_str()
        );

        // LSP locations outside the workspace name the same `path` field,
        // absolute, beside workspace-relative ones.
        let lsp = json!({"index":0,"data":{"type":"callers","items":[
            {"name":"inside","uri":format!("file://{root}/src/a.rs")},
            {"name":"outside","uri":format!("file://{outside}")}
        ]}});
        let value = envelope_in(vec![lsp], ToolId::LspSearch, &[Some(&json!({}))], &ws.paths);
        let items = &value["results"][0]["data"]["items"];
        assert_eq!(items[0]["path"], "src/a.rs", "{value}");
        assert_eq!(items[1]["path"], outside.as_str(), "{value}");
        assert!(items[1].get("uri").is_none(), "{value}");
    }

    /// astTopology rows are grouped under the scanned directory: `base` is
    /// the workspace like every local tool, the row `path` names the scanned
    /// directory relative to it, and `base` + `path` + `file` is the file.
    /// Continuations name the root relative to the workspace too.
    #[test]
    fn topology_rows_resolve_from_the_workspace_base() {
        let ws = workspace();
        let root = ws.root.to_string_lossy().into_owned();
        let topology = vec![json!({"index":0,"data": {
            "path": "workspace/relative",
            "results": [{"file": "runtime/b.rs"}],
            "next": {"nextPage": {"tool":"astTopology","query":{"path":format!("{root}/src"),"analysis":"dependents","file":"runtime/b.rs","page":2}}}
        }})];
        let value = envelope_in(
            topology,
            ToolId::AstTopology,
            &[Some(&json!({"path":"src"}))],
            &ws.paths,
        );
        assert_eq!(value["base"], root.as_str(), "{value}");
        let data = &value["results"][0]["data"];
        assert_eq!(data["path"], "src", "{value}");
        let file = data["results"][0]["file"].as_str().expect("file");
        assert!(ws.root.join("src").join(file).is_file());
        assert_eq!(data["next"]["nextPage"]["query"]["path"], "src", "{value}");
        // The workspace root itself is `.`; a root outside it is its own base.
        let value = envelope_in(
            vec![
                json!({"index":0,"data":{"path":"x","results":[{"file":"packages/app/src/a.rs"}]}}),
            ],
            ToolId::AstTopology,
            &[Some(&json!({"path":"."}))],
            &ws.paths,
        );
        assert_eq!(value["results"][0]["data"]["path"], ".", "{value}");
        let outside = ws.outside.to_string_lossy().into_owned();
        let value = envelope_in(
            vec![json!({"index":0,"data":{"path":"x","results":[{"file":"o.rs"}]}})],
            ToolId::AstTopology,
            &[Some(&json!({"path": outside.clone()}))],
            &ws.paths,
        );
        assert_eq!(value["base"], outside.as_str(), "{value}");
        assert_eq!(value["results"][0]["data"]["path"], ".", "{value}");
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
    /// Never-trim invariant: a clipped value must carry a continuation that
    /// reaches it. A read of another file, or a page of more rows, does not.
    #[test]
    fn a_clipped_value_needs_a_continuation_that_reaches_it() {
        let read = |path: &str| json!({"tool":"localFetch","query":{"path":path,"startLine":1,"endLine":2}});
        let page = |next: Value| {
            json!({"files":[
                {"path":"a.rs","matches":[{"line":1,"value":"short"}]},
                {"path":"b.rs","matches":[{"line":4,"value":"long…","truncated":true,"originalChars":900}]}
            ],"next":next})
        };
        let codes = |data: Value| {
            result_row(
                ToolId::from_name("localSearch").expect("known tool"),
                0,
                &json!({"debug":true}),
                data,
                None,
            )
            .pointer("/meta/diagnostics/codes")
            .cloned()
        };
        let missing = Some(json!(["continuationMissing"]));
        assert_eq!(codes(page(json!({"read":read("a.rs")}))), missing);
        assert_eq!(
            codes(page(
                json!({"read":read("a.rs"),"nextPage":{"tool":"localSearch","query":{"path":".","searchText":"x","page":2}}})
            )),
            missing
        );
        assert_eq!(codes(page(json!({"read":read("b.rs")}))), None);
        // An absolute row path and a relative continuation name one file.
        let absolute = |next: Value| {
            let mut data = page(next);
            data["files"][1]["path"] = json!("/repo/src/b.rs");
            data
        };
        assert_eq!(codes(absolute(json!({"read":read("src/b.rs")}))), None);
        assert_eq!(codes(absolute(json!({"read":read("c/b.rs")}))), missing);
        assert_eq!(codes(absolute(json!({"read":read("rc/b.rs")}))), missing);
        assert_eq!(
            codes(page(
                json!({"expandValues":{"tool":"localSearch","query":{"path":".","searchText":"x","matchContentLength":900}}})
            )),
            None
        );
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
        let mut normal = result_row(
            ToolId::from_name("localSearch").expect("known tool"),
            0,
            &json!({"debug":false}),
            json!({"files":[]}),
            None,
        );
        super::super::verbose::prune_row(&mut normal, ToolId::LocalSearch, &json!({"debug":false}));
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
            &json!({"mainGoal": "test", "reasoning":"Read the next exact page.","debug":false}),
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
        // Emitters leave the brief to the engine, which copies it from the input row.
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
            &json!({"mainGoal": "test", "reasoning":"Evaluate captured evidence.","debug":false}),
            json!({
                "context": {
                    "next": {
                        "continue": {
                            "tool": "localFetch",
                            "query": {
                                "path":"/repo/a.rs",
                                "offset":2,
                                "mainGoal": "test", "reasoning":"Read the next exact page.",
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
        // astSearch rejects a missing, unknown, or mismatched grammar and an
        // unparseable pattern: the caller must fix the request (exit 2).
        for code in [
            "ast.language.required",
            "ast.language.unsupported",
            "ast.language.mismatch",
            "ast.language.fileRequired",
            "ast.language.directoryRequired",
            "structural.query.compileFailed",
            "structural.query.invalid",
            "structural.language.unsupported",
            "syntaxTree.language.unsupported",
            "syntaxTree.options.invalidLimit",
            "lsp.invalidQuery",
        ] {
            assert!(is_invalid_input_code(code), "{code}");
        }
        for code in [
            "structural.parse.interrupted",
            "structural.match.depthLimit",
            "ast.source.limit",
            "ast.policy.notFound",
            "ast.execution.io",
        ] {
            assert!(!is_invalid_input_code(code), "{code}");
        }
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
            response_window: None,
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
