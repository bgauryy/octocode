//! Transport-neutral structured result metadata and lossless path compaction.
use crate::policy::path::PathPolicy;
use crate::tools::id::ToolId;
use crate::tools::output::{PathAnchor, SANDBOX_HINT};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub(crate) fn attach_diagnostics(
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

/// Budget every hint and lead `why` source fits (X11): the runtime never
/// clips guidance, so each source is written within it
/// (`every_hint_source_fits_the_guidance_budget`).
#[cfg(test)]
pub(crate) const MAX_GUIDANCE_CHARS: usize = 120;

/// Row containers whose prose `hints` are agent guidance. `meta` is debug
/// diagnostics: it keeps its own hints and never takes the row's one tip.
const METADATA_CONTAINERS: &[&str] = &[
    "data",
    "diagnostics",
    "error",
    "files",
    "directories",
    "results",
    "packages",
    "entries",
    "items",
];

/// One guidance line: whitespace collapsed and a closing period. Never
/// clipped: a cut hint loses the recovery it names (X11).
pub(super) fn concise(value: &str) -> String {
    let mut text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() && !matches!(text.chars().last(), Some('.' | '!' | '?' | '…')) {
        text.push('.');
    }
    text
}

pub(super) fn has_executable_call(value: &Value) -> bool {
    let Some(call) = value.as_object() else {
        return false;
    };
    call.get("tool").and_then(Value::as_str).is_some()
        && call.get("query").is_some_and(Value::is_object)
}

fn has_recovery(value: &Value) -> bool {
    if let Some(values) = value.as_array() {
        return values.iter().any(has_recovery);
    }
    let Some(node) = value.as_object() else {
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
        .and_then(Value::as_object)
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

/// Recovery keyed by the exact `errorCode` values the runtime emits
/// (provider kinds, policy/AST/LSP/clone/classification codes). Codes not
/// listed fall back to the tool hint.
pub(super) fn error_code_hint(tool: ToolId, code: &str) -> Option<&'static str> {
    Some(match code {
        "outsideAllowedRoots" | "symlinkEscape" => SANDBOX_HINT,
        "authentication" => "Authenticate or correct credentials; do not broaden the query.",
        "permission" | "permissionDenied" => {
            "Verify access and token scopes; do not treat denial as absence."
        }
        "rateLimited" | "classificationRateLimited" => {
            "Wait for Retry-After or the provider reset before retrying."
        }
        "timeout" | "transport" | "lockTimeout" => {
            "Retry once; if it persists, narrow scope and verify provider availability."
        }
        "staleSnapshot" => "Discard prior pages and restart without the stale snapshot.",
        // One code, one recovery on every tool.
        crate::policy::PATH_NOT_FOUND => crate::tools::output::VERIFY_PATH_HINT,
        crate::policy::PATH_POLICY_DENIED => crate::policy::discovery::WITHHELD_HINT,
        code => {
            if let Some(hint) = tool.output().error_hint(code) {
                hint
            } else if is_invalid_input_code(code) {
                "Correct the rejected field named in the error; broadening will not help."
            } else {
                return None;
            }
        }
    })
}

fn error_fallback_hint(tool: ToolId, query: &Value, row: &Value) -> &'static str {
    let code = row
        .pointer("/data/errorCode")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Local tools report a sandbox refusal under its own code
    // (`outsideAllowedRoots`), so the hint is keyed by code alone.
    error_code_hint(tool, code).unwrap_or_else(|| tool.output().fallback_hint(query))
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
        tool.output().fallback_hint(&query)
    };
    let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    data.insert("hints".into(), json!([hint]));
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
    let Some(node) = value.as_object_mut() else {
        return;
    };
    let needs_help = matches!(
        node.get("status").and_then(Value::as_str),
        Some("error" | "empty")
    ) || recovery;
    let keys: Vec<String> = node.keys().cloned().collect();
    for key in keys {
        if METADATA_CONTAINERS.contains(&key.as_str())
            && let Some(child) = node.get_mut(&key)
        {
            visit(child, needs_help, seen);
        }
        if key == "repositories"
            && let Some(repos) = node.get_mut("repositories").and_then(Value::as_object_mut)
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

/// One concise recovery hint on an empty or failed row (continuation calls
/// are shaped by the continuation stage).
pub(crate) fn apply_hint_policy(row: &mut Value, tool: ToolId, query: &Value) {
    add_fallback_hint(row, 0, tool, std::slice::from_ref(query));
    visit(row, false, &mut std::collections::BTreeSet::new());
}

/// Sanitize every string field, including provider diagnostics and metadata.
/// Domain content scans alone cannot protect errors returned by a remote server.
pub(crate) fn sanitize_fields(
    value: &mut Value,
    security: &crate::security::ContentSecurity,
    context: &crate::runtime::ExecutionContext,
) -> Result<(), crate::runtime::ExecutionError> {
    context.check()?;
    crate::security::sanitize_json(value, &mut |text| {
        Ok::<_, crate::runtime::ExecutionError>(security.sanitize_text(text, None).content)
    })
}

/// Opt-in email masking for gh outputs (`output.redactEmails`): applied
/// after secret sanitization so commit-author addresses and similar PII do
/// not leave the runtime when the user asked for redaction.
pub(super) fn redact_email_fields(
    value: &mut Value,
    security: &crate::security::ContentSecurity,
    context: &crate::runtime::ExecutionContext,
) -> Result<(), crate::runtime::ExecutionError> {
    context.check()?;
    crate::security::sanitize_json(value, &mut |text| {
        Ok::<_, crate::runtime::ExecutionError>(security.redact_emails(text))
    })
}

/// Apply the shared ordinary-output disclosure policy before output validation
/// or any downstream consumer observes the value.
pub(crate) fn sanitize_output(
    value: &mut Value,
    tool: ToolId,
    security: &crate::security::ContentSecurity,
    context: &crate::runtime::ExecutionContext,
    redact_emails: bool,
) -> Result<(), crate::runtime::ExecutionError> {
    sanitize_fields(value, security, context)?;
    if redact_emails && tool.is_github() {
        redact_email_fields(value, security, context)?;
    }
    Ok(())
}

pub fn result_row(
    tool: ToolId,
    index: usize,
    query: &Value,
    mut data: Value,
    status: Option<&str>,
) -> Value {
    if data.get("errorCode").and_then(Value::as_str) == Some("staleSnapshot") {
        super::pages::restart_stale(tool, query, &mut data);
    }
    let paged = !tool.output().resource_major()
        && !super::pages::is_complete(&data)
        && super::pages::has_remaining_page(tool, &data);
    if let Some(object) = data.as_object_mut()
        && object.get("isPartial") == Some(&Value::Null)
    {
        object.insert("isPartial".into(), Value::Bool(false));
    }
    // A remaining page makes the row itself partial, whatever its nested
    // items say.
    if paged
        && data.get("isPartial") != Some(&Value::Bool(true))
        && let Some(object) = data.as_object_mut()
    {
        object.insert("isPartial".into(), Value::Bool(true));
    }
    if let Some(object) = data.as_object_mut() {
        // Request echoes: the row answers its query, it does not repeat it.
        let echoes = crate::tools::result::INTENT_FIELDS.into_iter().chain([
            "cache",
            "researchSuggestions",
            "query",
        ]);
        for key in echoes {
            object.remove(key);
        }
        object.remove("status");
        if status != Some("error") {
            object.remove("error");
        }
    }
    // `meta` is verbose: the verbose stage drops it unless the row asked
    // for `debug: true`.
    let meta = row_meta(tool, query, &data, status);
    let mut row = json!({"index":index,"meta":meta,"data":data});
    if let Some(status) = status {
        row["status"] = json!(status);
    }
    row
}

/// The debug view of a row: its evidence kind and confidence, and the
/// diagnostics of a partial or failed row.
fn row_meta(tool: ToolId, query: &Value, data: &Value, status: Option<&str>) -> Value {
    let partial = super::pages::is_partial(data);
    let kind = tool.output().evidence_kind(query, data);
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
    let mut codes = Vec::new();
    if let Some(code) = data.get("errorCode").and_then(Value::as_str) {
        codes.push(code.to_owned());
    }
    if partial {
        codes.extend(super::pages::pagination_codes(data));
    }
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
    meta
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
    tool.output().compact(data, query);
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
    super::pages::is_pagination_key(key)
        || matches!(
            key,
            "truncated"
                | "isPartial"
                | "capped"
                | "filesSkipped"
                | "contentView"
                | "operation"
                | "type"
                | "owner"
                | "repo"
                | "snapshot"
                | "diagnostics"
                | "stats"
                | "committer"
                | "messageHeadline"
        )
}

fn minimize_data(data: &mut Map<String, Value>, query: &Value) {
    let more = data
        .iter()
        .any(|(key, value)| super::pages::is_pagination_key(key) && value["hasMore"] == true);
    for flag in ["truncated", "isPartial", "capped"] {
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
    for key in ["operation", "type", "owner", "repo"] {
        if data.get(key).is_some() && data.get(key) == query.get(key) {
            data.remove(key);
        }
    }
    let limited = data.contains_key("providerLimit");
    for (_, block) in data
        .iter_mut()
        .filter(|(key, _)| super::pages::is_pagination_key(key))
    {
        if let Some(block) = block.as_object_mut() {
            super::pages::slim_pagination(block, limited);
        }
    }
    // A later page keeps its pagination: it names where the page starts
    // (and a byte window's unit, which keeps its text verbatim).
    data.retain(|key, value| {
        !super::pages::is_pagination_key(key)
            || value["hasMore"] == true
            || value
                .get("currentPage")
                .or_else(|| value.get("page"))
                .and_then(Value::as_u64)
                .is_some_and(|page| page > 1)
            || value
                .get("offset")
                .and_then(Value::as_u64)
                .is_some_and(|offset| offset > 0)
    });
    data.remove("snapshot");
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

use crate::contracts::resolve_ref;

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

/// Search statistics: the listed files already carry the counts. Keep the
/// totals (occurrences, matched lines, files) only while more pages exist,
/// and the scan scope only when nothing
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
            matches!(key.as_str(), "filesScanned" | "matchCount") || keep_cap(key)
        });
        return;
    }
    if more {
        stats.retain(|key, _| {
            matches!(
                key.as_str(),
                "matchCount" | "matchedLineCount" | "fileCount"
            ) || keep_cap(key)
        });
    } else {
        stats.retain(|key, _| {
            keep_cap(key)
                || ((capped || cap_reason || unreadable)
                    && matches!(key.as_str(), "matchCount" | "fileCount"))
        });
        if stats.is_empty() {
            data.remove("stats");
        }
    }
}

/// Row `errorCode`s that reject the caller's input (CLI exit 2): the request
/// is wrong, so broadening or retrying it cannot help.
pub fn is_invalid_input_code(code: &str) -> bool {
    crate::tools::id::error_class(code) == crate::tools::id::error_codes::ErrorClass::InvalidInput
}

/// Build the public envelope for executed rows; `queries` maps each row
/// position to its validated query (`None` for a rejected row).
///
/// The tool's [`PathAnchor`] decides how file paths are named. Local file
/// paths are anchored on the workspace root: a path inside it is emitted
/// relative to it, so a copied row path resolves as-is, and `root` is that
/// root; a path outside it stays absolute.
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
    let anchor = tool.output().path_anchor();
    if anchor == PathAnchor::ScannedDir {
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
    if anchor == PathAnchor::Common {
        return envelope(rows);
    }
    if anchor == PathAnchor::QueryParent {
        for (row, root) in rows.iter_mut().zip(&roots) {
            if let Some(parent) = root.as_deref().and_then(Path::parent) {
                let parent = parent.to_string_lossy();
                prefix_relative_paths(&mut row["data"], 0, &parent);
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
    attach_heads(&mut value, &heads);
    value
}

/// HEAD of each row's root for the tools that read the working tree
/// directly; empty for every other tool. A root is read once per response.
fn checked_out_heads(tool: ToolId, roots: &[Option<PathBuf>]) -> Vec<Option<String>> {
    if !tool.output().reads_worktree() {
        return Vec::new();
    }
    let mut seen = std::collections::HashMap::<&Path, Option<String>>::new();
    roots
        .iter()
        .map(|root| {
            let root = root.as_deref()?;
            seen.entry(root)
                .or_insert_with(|| crate::runtime::git_head::head_sha(root))
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

/// Spell every absolute local `path` of a continuation's rows (`{tool,
/// query}` under `next`, `hints`, or a row) relative to the workspace root,
/// which relative paths resolve against: the same file, without repeating
/// the root the response already names as `root`. Paths outside the
/// workspace, `file://` uris and non-local fields stay as they are.
fn relativize_continuation_paths(value: &mut Value, workspace: &str, depth: usize) {
    if depth > 12 {
        return;
    }
    match value {
        Value::Object(map) => {
            if map.get("tool").is_some_and(Value::is_string)
                && let Some(Value::Array(rows)) = map
                    .get_mut("query")
                    .and_then(|query| query.get_mut("queries"))
            {
                for row in rows {
                    if let Some(Value::String(path)) = row.get_mut("path")
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

/// [`PathAnchor::ScannedDir`] fields (`file`, entrypoints, diagnostics) are
/// relative to the scanned directory (a file root's parent), which groups every row. Like
/// the other local tools, `root` is the workspace root and the row `path`
/// names that directory relative to it (`.` for the root itself), so `root`,
/// `path` and `file` joined resolve each file. A directory outside the
/// workspace becomes `root` itself, with `path` `.`.
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
    value["root"] = json!(base);
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
    let shared = crate::contracts::shared_fields::hoist(&mut rows);
    let mut value = json!({"results":rows});
    if let Some(base) = base {
        value["root"] = json!(base);
    }
    if let Some(shared) = shared {
        value["shared"] = Value::Object(shared);
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
                .all(|variant| variant.contains("pullRequests")
                    || variant.contains("issues")
                    || variant.contains("commits")
                    || variant.contains("error")),
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
            json!({"owner":"o","repo":"r","issues":[],"effectiveQuery":"is:issue"}),
        );
        assert_eq!(row["data"]["issues"], json!([]), "{row}");
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

    /// The last page of a window read keeps its pagination: its offset
    /// places the text, and a byte unit keeps that text verbatim.
    #[test]
    fn a_later_window_page_keeps_its_pagination() {
        let page = json!({"unit":"bytes","offset":8000,"length":12,"hasMore":false});
        let last = minimized(
            ToolId::GhGetFileContent,
            json!({}),
            json!({"path":"a","content":"x","pagination":page.clone()}),
        );
        assert_eq!(last["data"]["pagination"], page, "{last}");
        let whole = minimized(
            ToolId::LocalFetch,
            json!({}),
            json!({"path":"a","content":"x","pagination":{"unit":"lines","offset":0,"length":1,"hasMore":false}}),
        );
        assert!(whole["data"].get("pagination").is_none(), "{whole}");
    }

    /// Every tool's pagination block keeps the same keys on every page
    /// (where it stands, its size, what is known of the total, whether more
    /// exists) and drops only what `next` carries or what asserts nothing.
    #[test]
    fn pagination_blocks_keep_stable_keys_on_every_tool() {
        let next = json!({"nextPage":{"tool":"ghSearchCode","query":{"queries":[{"owner":"o","page":2}]}}});
        for tool in [ToolId::GhSearchCode, ToolId::LocalSearch, ToolId::LspSearch] {
            let row = minimized(
                tool,
                json!({}),
                json!({"files":[{"path":"a"}],"next":next.clone(),"pagination":{"currentPage":1,"totalPages":2,"pageSize":5,"totalItems":7,
                    "totalItemsCapped":false,"hasMore":true,"nextPage":2,"snapshot":"s","resultId":"r"}}),
            );
            assert_eq!(
                row["data"]["pagination"],
                json!({"currentPage":1,"totalPages":2,"pageSize":5,"totalItems":7,"hasMore":true}),
                "{tool}"
            );
        }
        let capped = minimized(
            ToolId::GhSearchRepo,
            json!({}),
            json!({"repositories":[],"pagination":{"totalItems":1000,"totalItemsCapped":true,"hasMore":true,"currentPage":3},
                "providerLimit":{"reason":"providerResultCap","maxResults":1000}}),
        );
        assert_eq!(
            capped["data"]["pagination"],
            json!({"totalItems":1000,"hasMore":true,"currentPage":3}),
            "a true cap repeats providerLimit"
        );
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
            json!({"path":"/r","matchString":"x"}),
            json!({"files":[{"path":"a.rs"}],"stats":{"matchCount":1,"filesScanned":9},
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
        // nextDiagnosticPage pages exist to return these entries; they explain
        // graph gaps (unlinked imports, unsupported layouts).
        let row = minimized(
            ToolId::AstTopology,
            json!({"operation":"dependents","path":"/r","source":"a.rs"}),
            json!({"results":[],"filesScanned":9,"coverage":{
                "diagnostics":[{"code":"unlinkedImport","file":"b.rs"}],
                "diagnosticPagination":{"currentPage":1,"totalPages":2,"hasMore":true}}}),
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
            json!({"files":[{"path":"a.rs"}],"stats":{"matchCount":40,"matchedLineCount":31,"fileCount":9,"bytesSearched":7},
                "pagination":{"currentPage":1,"hasMore":true},"next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"snapshot":"s"}]}}}}),
        );
        let data = &row["data"];
        assert_eq!(data["pagination"]["hasMore"], true);
        // Occurrences and matched lines (the rows a walk shows) are both
        // named, so the two totals never read as one disagreeing count.
        assert_eq!(
            data["stats"],
            json!({"matchCount":40,"matchedLineCount":31,"fileCount":9})
        );
        assert_eq!(
            data["next"]["nextPage"]["query"]["queries"][0]["snapshot"],
            "s"
        );
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
            json!({"stats":{"matchCount":0,"filesScanned":176,"bytesSearched":9}}),
        );
        assert_eq!(
            row["data"]["stats"],
            json!({"matchCount":0,"filesScanned":176})
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
                    "stats": {"capped": true, "capReason": "maxCollectedFiles", "matchCount": 10002,
                        "fileCount": 10002, "filesScanned": 10002, "bytesSearched": 70014},
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
            assert_eq!(row["data"]["stats"]["matchCount"], 10002, "{row}");
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
                    "stats": {"matchCount": 1, "fileCount": 1, "filesScanned": 2,
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
            json!({"operation":"dependencies"}),
            json!({"operation":"dependencies","confidence":"low",
                "coverage":{"basis":"syntactic","diagnosticCounts":{"x":5},"diagnostics":[{"file":"a"}]},
                "completeness":{"graph":"coverage-incomplete"}}),
        );
        let data = &topology["data"];
        assert!(data.get("source").is_none(), "request echo: {topology}");
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
        let gh = minimized(ToolId::GhGetFileContent, json!({}), file.clone());
        assert_eq!(gh["data"], json!({"path":"a","content":"x","totalLines":9}));
        let debug = minimized(
            ToolId::GhGetFileContent,
            json!({"debug":true}),
            file.clone(),
        );
        assert_eq!(debug["data"], file);
    }

    #[test]
    fn echoes_go_only_when_they_equal_the_request_and_debug_keeps_all() {
        let data = json!({"owner":"o","repo":"other","path":"a.rs"});
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
        for code in ["hashMismatch", "fileAccessFailed"] {
            let hint = error_hint("astRewrite", &apply, code, "failed");
            assert!(!hint.contains("beforeHash"), "{code}: {hint}");
        }
    }

    #[test]
    fn ast_rewrite_unreachable_root_points_at_the_path_not_the_pattern() {
        let query = json!({"path": "/repo/nope"});
        let hint = error_hint("astRewrite", &query, "rootUnavailable", "x");
        assert!(hint.contains("Verify the path exists"), "{hint}");
        assert!(!hint.contains("Broaden"), "{hint}");
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
                    "outsideAllowedRoots",
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
        assert_eq!(value["root"], ws.root.to_string_lossy().as_ref(), "{value}");
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
            "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":format!("{root}/src"),"matchString":"x","page":2}]}}},
            "hints":{"read":{"tool":"localFetch","query":{"queries":[{"path":outside.clone()}]}},
                     "refs":{"tool":"lspSearch","query":{"queries":[{"path":format!("{root}/src/a.rs"),"workspaceRoot":root.clone()}]}}}
        }});
        let value = envelope_in(
            vec![row],
            ToolId::LocalSearch,
            &[Some(&json!({"path": format!("{root}/src")}))],
            &ws.paths,
        );
        let data = &value["results"][0]["data"];
        assert_eq!(
            data["next"]["nextPage"]["query"]["queries"][0]["path"], "src",
            "{value}"
        );
        assert_eq!(
            data["hints"]["read"]["query"]["queries"][0]["path"],
            outside.as_str(),
            "{value}"
        );
        let refs = &data["hints"]["refs"]["query"]["queries"][0];
        assert_eq!(refs["path"], "src/a.rs", "{value}");
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
        for tool in [
            ToolId::LocalSearch,
            ToolId::LocalFetch,
            ToolId::StructureSearch,
        ] {
            let value = envelope_in(
                vec![row(0), row(1)],
                tool,
                &[Some(&src), Some(&tests)],
                &ws.paths,
            );
            assert_eq!(value["shared"]["commitSha"], sha, "{tool:?} {value}");
            assert!(
                value["results"][0]["data"].get("commitSha").is_none(),
                "{value}"
            );
        }
        let outside = json!({"path": ws.outside.to_string_lossy()});
        let mixed = envelope_in(
            vec![row(0), row(1)],
            ToolId::LocalSearch,
            &[Some(&src), Some(&outside)],
            &ws.paths,
        );
        assert!(mixed["shared"].get("commitSha").is_none(), "{mixed}");
        assert_eq!(mixed["results"][0]["data"]["commitSha"], sha, "{mixed}");
        assert!(
            mixed["results"][1]["data"].get("commitSha").is_none(),
            "{mixed}"
        );
        let other = envelope_in(vec![row(0)], ToolId::AstSearch, &[Some(&src)], &ws.paths);
        assert!(other.get("shared").is_none(), "{other}");
        let plain = envelope_in(
            vec![row(0)],
            ToolId::LocalSearch,
            &[Some(&outside)],
            &ws.paths,
        );
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

    /// structureSearch `files` groups name `dir` relative to the row's
    /// `path` (like `tree`), so the envelope leaves them as they are: only
    /// `path` resolves against `base`, and a batch never hoists a `dir`.
    #[test]
    fn structure_listing_dirs_stay_relative_to_the_row_path() {
        let ws = workspace();
        let src = json!({"path":"src"});
        let rows = vec![
            json!({"index":0,"data":{"path":"src","files":["a.rs (1)",
                {"dir":"runtime","files":["b.rs (1)"]}]}}),
            json!({"index":1,"data":{"path":"src","files":[{"dir":"runtime","files":["b.rs (1)"]}]}}),
        ];
        let value = envelope_in(
            rows,
            ToolId::StructureSearch,
            &[Some(&src), Some(&src)],
            &ws.paths,
        );
        assert!(value.get("shared").is_none(), "{value}");
        for row in 0..2 {
            let data = &value["results"][row]["data"];
            let files = data["files"].as_array().expect("files");
            let group = files.iter().find(|item| item.is_object()).expect("group");
            assert_eq!(group["dir"], "runtime", "{value}");
            assert_eq!(data["path"], "src", "{value}");
        }
        for path in ["src/a.rs", "src/runtime/b.rs"] {
            assert_resolvable(&ws, &value, path);
        }
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
            json!({"index":2,"data":{"symbols":[{"path":"runtime/b.rs"}]}}),
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
            value["results"][2]["data"]["symbols"][0]["path"],
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
            "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"path":"src/runtime"}]}}}}});
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
            value["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0]["path"],
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
        assert!(value.get("root").is_none(), "{value}");
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
            "next": {"nextPage": {"tool":"astTopology","query":{"queries":[{"path":format!("{root}/src"),"operation":"dependents","source":"runtime/b.rs","page":2}]}}}
        }})];
        let value = envelope_in(
            topology,
            ToolId::AstTopology,
            &[Some(&json!({"path":"src"}))],
            &ws.paths,
        );
        assert_eq!(value["root"], root.as_str(), "{value}");
        let data = &value["results"][0]["data"];
        assert_eq!(data["path"], "src", "{value}");
        let file = data["results"][0]["file"].as_str().expect("file");
        assert!(ws.root.join("src").join(file).is_file());
        assert_eq!(
            data["next"]["nextPage"]["query"]["queries"][0]["path"], "src",
            "{value}"
        );
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
        assert_eq!(value["root"], outside.as_str(), "{value}");
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
        let data = json!({"path":"/repo/src/a.ts","content":"/repo/src/a.ts","pagination":{"hasMore":true},"next":{"continue":{"tool":"localFetch","query":{"queries":[{"path":"/repo/src/a.ts","offset":2}]}}}});
        let output = envelope(vec![result_row(
            ToolId::from_name("localFetch").expect("known tool"),
            0,
            &json!({"debug":true}),
            data,
            None,
        )]);
        assert_eq!(output["root"], "/repo/src");
        assert_eq!(output["results"][0]["data"]["path"], "a.ts");
        assert_eq!(output["results"][0]["data"]["content"], "/repo/src/a.ts");
        assert_eq!(
            output.pointer("/results/0/data/next/continue/query/queries/0/path"),
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
        let read = |path: &str| json!({"tool":"localFetch","query":{"queries":[{"path":path,"ranges":["1-2"]}]}});
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
                json!({"read":read("a.rs"),"nextPage":{"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","page":2}]}}})
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
                json!({"expandValues":{"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","matchContentLength":900}]}}})
            )),
            None
        );
    }

    /// Every tool's stale snapshot is the same failure: one text and a
    /// `next.restart` that reads page 1 of the same query on the current
    /// source. A restart the tool built is kept.
    #[test]
    fn a_stale_snapshot_always_restarts_with_one_text() {
        let query =
            json!({"path":"src","operation":"match","pattern":"f($A)","page":3,"snapshot":"s1"});
        let row = result_row(
            ToolId::AstSearch,
            0,
            &query,
            json!({"errorCode":"staleSnapshot","error":"tool wording","snapshot":"s2","isPartial":true}),
            Some("error"),
        );
        let data = &row["data"];
        assert_eq!(
            data["error"],
            super::super::pages::STALE_SNAPSHOT_ERROR,
            "{data}"
        );
        assert!(
            data.get("isPartial").is_none(),
            "a restart is not a remaining page: {data}"
        );
        let restart = &data["next"]["restart"];
        assert_eq!(restart["tool"], "astSearch", "{data}");
        assert_eq!(
            restart["query"]["queries"][0],
            json!({"path":"src","operation":"match","pattern":"f($A)"}),
            "page 1 of the same query: {data}"
        );
        let own = json!({"tool":"localFetch","query":{"queries":[{"path":"a","offset":0}]}});
        let row = result_row(
            ToolId::LocalFetch,
            0,
            &json!({"path":"a","offset":90,"snapshot":"s"}),
            json!({"errorCode":"staleSnapshot","error":"x","next":{"restart":own.clone()}}),
            Some("error"),
        );
        assert_eq!(row["data"]["next"]["restart"], own);
        assert_eq!(
            row["data"]["error"],
            super::super::pages::STALE_SNAPSHOT_ERROR
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

    /// A row that offers more of its own result (`next.next*`, `continue*`,
    /// `expand*`, `retry*`) is not complete, whatever the tool reported.
    #[test]
    fn a_row_with_a_next_page_is_never_complete() {
        let page = json!({"tool":"astSearch","query":{"queries":[{"path":".","pattern":"f($A)","captureText":true}]}});
        let row = result_row(
            ToolId::AstSearch,
            0,
            &json!({"debug":true}),
            json!({"files":[],"next":{"expandCaptures":page}}),
            None,
        );
        assert_eq!(row.pointer("/data/isPartial"), Some(&json!(true)), "{row}");
        assert_eq!(
            row.pointer("/meta/diagnostics/partial"),
            Some(&json!(true)),
            "{row}"
        );
        let flagless = result_row(
            ToolId::LocalFetch,
            0,
            &json!({}),
            json!({"path":"a","isPartial":false,"next":{"continue":page}}),
            None,
        );
        assert_eq!(
            flagless.pointer("/data/isPartial"),
            Some(&json!(true)),
            "{flagless}"
        );
        // Only a file's own match pagination says more: the row says so too.
        let nested = result_row(
            ToolId::AstSearch,
            0,
            &json!({}),
            json!({"files":[{"path":"a","matches":[],"pagination":{"hasMore":true}}],
                "next":{"nextMatchPage":page}}),
            None,
        );
        assert_eq!(
            nested.pointer("/data/isPartial"),
            Some(&json!(true)),
            "{nested}"
        );
        // A guarded apply or a restart is a follow-up, not unread rest.
        let preview = result_row(
            ToolId::AstRewrite,
            0,
            &json!({}),
            json!({"complete":true,"next":{"apply":page,"restart":page}}),
            None,
        );
        assert_eq!(
            preview.pointer("/data/complete"),
            Some(&json!(true)),
            "{preview}"
        );
        assert!(preview.pointer("/data/isPartial").is_none(), "{preview}");
    }

    #[test]
    fn result_rows_normalize_null_is_partial_to_false() {
        let row = result_row(
            ToolId::from_name("ghGetFileContent").expect("known tool"),
            0,
            &json!({"debug":false}),
            json!({"path":"a","content":"x","isPartial":null}),
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
            json!({"compareStatus":"ahead","commits":[]}),
            None,
        );
        assert_eq!(row.pointer("/data/compareStatus"), Some(&json!("ahead")));
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

    /// Debug diagnostics (`meta`) never take the row's one recovery hint: an
    /// empty code search keeps its cause-specific tip.
    #[test]
    fn diagnostic_hints_never_displace_the_rows_recovery_tip() {
        let mut row = json!({"index":0,"status":"empty",
            "meta":{"diagnostics":{"hints":["GitHub reported an incomplete search index result; retry."]}},
            "data":{"hints":["The repository is missing, private, or hidden from this token; check owner/repo spelling."]}});
        apply_hint_policy(&mut row, ToolId::GhSearchCode, &json!({}));
        assert_eq!(
            row["data"]["hints"],
            json!([
                "The repository is missing, private, or hidden from this token; check owner/repo spelling."
            ])
        );
        assert_eq!(
            row["meta"]["diagnostics"]["hints"].as_array().map(Vec::len),
            Some(1)
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

    /// E6: one code, one recovery hint. A missing path gets the same hint
    /// on every tool that reports `pathNotFound`.
    #[test]
    fn path_errors_have_one_hint_on_every_tool() {
        for (code, expected) in [
            (
                crate::policy::PATH_NOT_FOUND,
                crate::tools::output::VERIFY_PATH_HINT,
            ),
            (
                crate::policy::PATH_POLICY_DENIED,
                crate::policy::discovery::WITHHELD_HINT,
            ),
        ] {
            let hints: std::collections::BTreeSet<String> = [
                "localFetch",
                "localSearch",
                "structureSearch",
                "astSearch",
                "astTopology",
                "lspSearch",
                "ghCloneRepo",
            ]
            .into_iter()
            .map(|tool| {
                let mut row = json!({
                    "index": 0,
                    "status": "error",
                    "data": {"error":"withheld","errorCode":code}
                });
                apply_hint_policy(
                    &mut row,
                    ToolId::from_name(tool).expect("known tool"),
                    &json!({"path":"/repo/missing.rs"}),
                );
                row["data"]["hints"][0].as_str().expect("hint").to_owned()
            })
            .collect();
            assert_eq!(hints.into_iter().collect::<Vec<_>>(), [expected], "{code}");
        }
    }

    #[test]
    fn invalid_input_and_missing_path_errors_do_not_suggest_broadening() {
        for (tool, code, expected) in [
            (
                "structureSearch",
                "invalidInput",
                "Correct the rejected field",
            ),
            ("ghSearchRepo", "invalidInput", "Correct the rejected field"),
            ("structureSearch", "pathNotFound", "Verify the path exists"),
            ("localSearch", "pathNotFound", "Verify the path exists"),
            ("structureSearch", "outsideAllowedRoots", "allowed roots"),
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
        assert!(is_invalid_input_code("invalidPattern"));
        assert!(!is_invalid_input_code("fileAccessFailed"));
        // astSearch rejects a missing, unknown, or mismatched grammar and an
        // unparseable pattern: the caller must fix the request (exit 2).
        for code in [
            "languageRequired",
            "languageUnsupported",
            "languageMismatch",
            "languageFileRequired",
            "languageDirectoryRequired",
            "invalidPattern",
            "invalidGlob",
            "invalidInput",
        ] {
            assert!(is_invalid_input_code(code), "{code}");
        }
        for code in [
            "timeout",
            "executionFailed",
            "fileTooLarge",
            "pathNotFound",
            "fileAccessFailed",
            // Undeclared spellings are no class at all, never invalid input.
            "structural.language.unsupported",
        ] {
            assert!(!is_invalid_input_code(code), "{code}");
        }
    }

    #[test]
    fn ast_search_error_hints_are_code_specific() {
        let query = json!({"operation":"match","path":"/repo","pattern":"foo($A)"});
        for (code, expected) in [
            ("outsideAllowedRoots", "allowed root"),
            ("structural.query.compileFailed", "syntaxTree"),
            ("fileTooLarge", "smaller"),
            ("languageRequired", "language"),
            ("languageUnsupported", "language"),
            ("languageFileRequired", "languageGlobs"),
            ("languageDirectoryRequired", "language"),
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
            walk_threads: None,
            response_window: None,
            github_credential: None,
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
                    "query": {"queries":[{"path": "/repo/ghp_secretvalue12.rs", "offset": 2}]},
                    "confidence": "exact"
                }
            },
            "location": {"localPath": "/tmp/ghp_secretvalue12/repo"}
        });
        let context = sanitize_context();
        let security = crate::security::ContentSecurity::new();
        sanitize_fields(&mut value, &security, &context).expect("sanitize");
        assert_eq!(
            value.pointer("/next/continue/query/queries/0/path"),
            Some(&json!("/repo/ghp_secretvalue12.rs"))
        );
        assert_eq!(
            value.pointer("/location/localPath"),
            Some(&json!("/tmp/ghp_secretvalue12/repo"))
        );
        // The executable tool identifier survives verbatim.
        assert_eq!(
            value.pointer("/next/continue/query/queries/0/offset"),
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
                    "query": {"queries":[{"path": format!("/repo/{token}.rs")}]}
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
            .pointer("/next/continue/query/queries/0/path")
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
