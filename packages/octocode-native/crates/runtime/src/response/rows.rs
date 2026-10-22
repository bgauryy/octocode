//! Transport-neutral structured result metadata and lossless path compaction.
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::output::{PathAnchor, SANDBOX_HINT};
use octocode_contract_types::{ErrorCode, ExtraKind};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub(crate) fn attach_diagnostics(
    row: &mut Value,
    diagnostics: crate::response::result::ToolDiagnostics,
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

/// Budget every hint and lead `why` source fits: the runtime never
/// clips guidance, so each source is written within it
/// (`every_hint_source_fits_the_guidance_budget`).
#[cfg(test)]
pub(crate) const MAX_GUIDANCE_CHARS: usize = 120;

/// Row containers whose prose `hints` are agent guidance. `meta` (the
/// `diagnostics` extra) keeps its own hints and never takes the row's one tip.
const METADATA_CONTAINERS: &[&str] = &[
    "data",
    "diagnostics",
    "error",
    "files",
    "directories",
    "nodes",
    "packages",
    "items",
];

/// One guidance line: whitespace collapsed and a closing period. Never
/// clipped: a cut hint loses the recovery it names.
pub(super) fn concise(value: &str) -> String {
    let mut text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() && !matches!(text.chars().last(), Some('.' | '!' | '?' | '…')) {
        text.push('.');
    }
    text
}

pub(crate) fn has_recovery(value: &Value) -> bool {
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
    let has_text = |hints: &Value| {
        hints.as_array().is_some_and(|hints| {
            hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|text| !text.trim().is_empty()))
        })
    };
    if node
        .get("hints")
        .and_then(|hints| hints.get(crate::id::continuation_channels::HINT_TEXT_KEY))
        .is_some_and(has_text)
    {
        return true;
    }
    if ["next", "hints"].iter().any(|key| {
        node.get(*key)
            .and_then(Value::as_object)
            .is_some_and(|calls| calls.values().any(crate::response::result::is_call))
    }) {
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

/// Hints are rendered whole only up to the response guidance limit (120
/// chars); keep every GitHub recovery hint within it.
pub(crate) const GITHUB_AUTH_RECOVERY_HINT: &str = "Run octocode auth login or set GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.";

/// The one recovery for a cancelled call, on every tool and stage.
pub(crate) const CANCELLED_HINT: &str = "Retry only if the result is still needed and what cancelled it changed; otherwise report the cancellation.";

/// The one recovery table for the `errorCode` values every tool shares
/// (provider kinds, policy, locks, input). A tool answers only for the codes
/// it alone emits (`ToolOutput::error_hint`); any other code gets the tool's
/// fallback hint.
pub(crate) fn error_code_hint(tool: ToolId, code: ErrorCode) -> Option<&'static str> {
    Some(match code {
        ErrorCode::OutsideAllowedRoots | ErrorCode::SymlinkEscape => SANDBOX_HINT,
        ErrorCode::Authentication => GITHUB_AUTH_RECOVERY_HINT,
        ErrorCode::Permission | ErrorCode::PermissionDenied => {
            "Verify access and token scopes; do not treat denial as absence."
        }
        ErrorCode::RateLimited | ErrorCode::ClassificationRateLimited => {
            "Wait for Retry-After or the rate-limit reset before retrying."
        }
        ErrorCode::Timeout | ErrorCode::Transport | ErrorCode::Server => {
            "Retry once; if it persists, narrow scope and verify provider availability."
        }
        ErrorCode::Cancelled => CANCELLED_HINT,
        ErrorCode::ResponseTooLarge => "Narrow the requested scope.",
        ErrorCode::RedirectDenied => "Use the canonical allowed host and repository.",
        ErrorCode::Decode => {
            "Retry once; report a provider response incompatibility if it persists."
        }
        ErrorCode::Configuration | ErrorCode::CredentialStoreUnavailable => {
            "Correct the provider authentication and configuration."
        }
        ErrorCode::Unavailable => {
            "The provider blocks this resource for legal reasons; retrying will not help."
        }
        ErrorCode::HttpStatus => {
            "Check httpStatus and the message; this status is not a network failure."
        }
        // One code, one recovery on every tool.
        ErrorCode::PathNotFound => crate::response::output::VERIFY_PATH_HINT,
        ErrorCode::PathPolicyDenied => crate::policy::discovery::WITHHELD_HINT,
        code => {
            if let Some(hint) = tool.output().error_hint(code) {
                hint
            } else if crate::id::error_codes::class(code)
                == crate::id::error_codes::ErrorClass::InvalidInput
            {
                "Correct the rejected field named in the error; broadening will not help."
            } else {
                return None;
            }
        }
    })
}

fn error_fallback_hint(tool: ToolId, query: &Value, row: &Value) -> &'static str {
    // Local tools report a sandbox refusal under its own code
    // (`outsideAllowedRoots`), so the hint is keyed by code alone.
    row.pointer("/data/errorCode")
        .and_then(Value::as_str)
        .and_then(crate::id::error_codes::parse)
        .and_then(|code| error_code_hint(tool, code))
        .unwrap_or_else(|| tool.output().fallback_hint(query))
}

/// The tool's fallback hint on an empty or failed row of `query` that has no
/// recovery of its own: the code's recovery for a failure, the tool's
/// fallback for both (an empty answer never ends bare). The continuation
/// stage calls it again after dropping a lead this surface cannot run or the
/// contract rejects.
pub(crate) fn ensure_recovery(row: &mut Value, tool: ToolId, query: &Value) {
    let status = row.get("status").and_then(Value::as_str);
    if !matches!(status, Some("error" | "empty" | "outOfRange")) || has_recovery(row) {
        return;
    }
    let hint = if status == Some("error") {
        error_fallback_hint(tool, query, row)
    } else {
        // An empty row that names its own category answers for itself: the
        // tool's hint for that category (or none), not the generic fallback.
        row.pointer("/data/category")
            .and_then(Value::as_str)
            .and_then(|category| tool.output().category_hint(category))
            .unwrap_or_else(|| tool.output().fallback_hint(query))
    };
    if hint == crate::response::output::NO_HINT {
        return;
    }
    let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    data.insert("hints".into(), json!([hint]));
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
        Some("error" | "empty" | "outOfRange")
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
            // An ordered list: the first hint is the recovery, later ones are
            // further findings; each keeps the one-hint budget, and a repeat
            // anywhere in the row is stated once.
            let candidates = candidates
                .iter()
                .filter_map(Value::as_str)
                .map(concise)
                .filter(|hint| !hint.is_empty());
            for short in candidates {
                // `hint_budget` sweeps authored literals; this checks each hint
                // as composed (counts, names, joined sentences) in every row a
                // test renders. Production never clips.
                #[cfg(test)]
                assert!(
                    short.chars().count() <= MAX_GUIDANCE_CHARS,
                    "hint over the {MAX_GUIDANCE_CHARS}-char guidance budget as composed: {short}"
                );
                if seen.insert(short.clone()) {
                    hints.push(short);
                }
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
    ensure_recovery(row, tool, query);
    visit(row, false, &mut std::collections::BTreeSet::new());
}

/// Sanitize every string field, including provider diagnostics and metadata.
/// Domain content scans alone cannot protect errors returned by a remote server.
/// A leaf of a result that names a source-code `path` is scanned as source code
/// (see [`ContentSecurity::sanitize_leaf`]); every other leaf, and every
/// credential pattern, applies as before.
pub(crate) fn sanitize_fields(
    value: &mut Value,
    security: &crate::security::ContentSecurity,
    context: &crate::execution::ExecutionContext,
) -> Result<(), crate::execution::ExecutionError> {
    context.check()?;
    crate::security::sanitize_json(value, &mut |text, path| {
        Ok::<_, crate::execution::ExecutionError>(security.sanitize_leaf(text, path).content)
    })
}

/// Opt-in email masking for gh outputs (`output.redactEmails`): applied
/// after secret sanitization so commit-author addresses and similar PII do
/// not leave the runtime when the user asked for redaction.
pub(super) fn redact_email_fields(
    value: &mut Value,
    security: &crate::security::ContentSecurity,
    context: &crate::execution::ExecutionContext,
) -> Result<(), crate::execution::ExecutionError> {
    context.check()?;
    crate::security::sanitize_json(value, &mut |text, _| {
        Ok::<_, crate::execution::ExecutionError>(security.redact_emails(text))
    })
}

/// Apply the shared ordinary-output disclosure policy before output validation
/// or any downstream consumer observes the value.
pub(crate) fn sanitize_output(
    value: &mut Value,
    tool: ToolId,
    security: &crate::security::ContentSecurity,
    context: &crate::execution::ExecutionContext,
    redact_emails: bool,
) -> Result<(), crate::execution::ExecutionError> {
    sanitize_fields(value, security, context)?;
    if redact_emails && tool.is_github() {
        redact_email_fields(value, security, context)?;
    }
    Ok(())
}

pub(crate) fn result_row(
    tool: ToolId,
    index: usize,
    query: &Value,
    mut data: Value,
    status: Option<&str>,
) -> Value {
    if crate::id::error_code_of(&data) == Some(ErrorCode::StaleSnapshot) {
        super::pages::restart_stale(tool, query, &mut data);
    }
    super::notices::drop_on_later_page(query, &mut data);
    let status =
        if status != Some("error") && super::pages::settle_out_of_range(tool, query, &mut data) {
            Some("outOfRange")
        } else {
            status
        };
    // An error row is not a partial result: a page it offers is the way
    // past the error (a bounded read of a too-large file), not more of it.
    let paged = status != Some("error")
        && !tool.output().resource_major()
        && !super::pages::is_complete(&data)
        && super::pages::has_remaining_page(tool, &data);
    // A row with more to read is not empty, whatever the tool decided from
    // the rows this page holds: an executable page or a pagination block
    // stating `hasMore` means the result continues.
    // A retry runs the same page again: an empty row offering only one (an
    // incomplete provider page) is still empty, so the call exits 1.
    let continues = paged && super::pages::has_page_beyond_retry(tool, &data);
    let status = if status == Some("empty") && (continues || states_more(&data)) {
        None
    } else {
        status
    };
    if let Some(object) = data.as_object_mut()
        && object.get("isPartial") == Some(&Value::Null)
    {
        object.insert("isPartial".into(), Value::Bool(false));
    }
    // A remaining page makes the row itself partial, whatever its nested
    // items say. The row's own `pagination.hasMore` with its `next` page
    // already says so, so the flag is left to what they do not say.
    if let Some(object) = data.as_object_mut() {
        if paged && states_remaining_page(object) {
            object.remove("isPartial");
        } else if paged {
            object.insert("isPartial".into(), Value::Bool(true));
        }
    }
    if let Some(object) = data.as_object_mut() {
        // Request echoes: the row answers its query, it does not repeat it.
        let echoes = crate::response::result::INTENT_FIELDS
            .into_iter()
            .chain(["cache", "query"]);
        for key in echoes {
            object.remove(key);
        }
        object.remove("status");
        if status != Some("error") {
            object.remove("error");
        }
    }
    // `meta` is a diagnostics extra: built only for a row that keeps it
    // (`extras: ["diagnostics"]`, or a `fields` pick naming it).
    let mut row = Map::new();
    row.insert("index".into(), json!(index));
    if super::extras::keeps(tool, query, "results[].meta") {
        row.insert("meta".into(), row_meta(tool, query, &data, status));
    }
    row.insert("data".into(), data);
    if let Some(status) = status {
        row.insert("status".into(), json!(status));
    }
    Value::Object(row)
}

/// Whether any page block of the row, its own or a nested item's, states
/// `hasMore`.
fn states_more(data: &Value) -> bool {
    data.as_object().is_some_and(|object| {
        super::pages::page_blocks_deep(object)
            .into_iter()
            .any(|block| block["hasMore"] == true)
    })
}

/// Whether a page block of the row (`pagination`, a `contentPagination`
/// section, its own or a nested item's) reports `hasMore` and nothing beyond
/// the pages makes it partial: `isPartial` would restate it. A
/// `partialReasons` list or a `terminalLimit` is another cause, which the
/// flag still sums up.
fn states_remaining_page(data: &Map<String, Value>) -> bool {
    data.get("terminalLimit") != Some(&Value::Bool(true))
        && data
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        && super::pages::page_blocks_deep(data)
            .into_iter()
            .any(|block| block["hasMore"] == true)
}

/// The diagnostics view of a row: its evidence kind and confidence, and the
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
    codes.extend(super::pages::coverage_codes(data, partial));
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
/// `extras: ["echo"]` keeps the request echoes and `extras: ["stats"]` the
/// full statistics block; every other rule still applies.
/// A failed row keeps every field that explains the failure and drops only
/// what asserts nothing there: empty arrays, the snapshot and a finished
/// page. Clasify rows never reach this pass; they have their own
/// resource-major projection.
pub(crate) fn minimize_row(row: &mut Value, tool: ToolId, query: &Value) {
    if row["status"] == "error" {
        if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
            minimize_error_data(data);
        }
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
    let echo = super::extras::wants(query, ExtraKind::Echo);
    minimize_data(data, tool, query, echo);
    if !echo {
        tool.output().compact(data, query);
    }
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

fn minimize_error_data(data: &mut Map<String, Value>) {
    data.remove("snapshot");
    data.retain(|key, value| {
        !value.as_array().is_some_and(Vec::is_empty)
            && (!super::pages::is_pagination_key(key)
                || super::pages::retain_page_blocks(key, value, walks_on))
    });
}

/// A pagination block that routes somewhere: more pages to read, or a page
/// past the first (it names where the page starts, and a byte window's
/// unit).
fn walks_on(block: &Value) -> bool {
    block["hasMore"] == true
        || block
            .get("currentPage")
            .or_else(|| block.get("page"))
            .and_then(Value::as_u64)
            .is_some_and(|page| page > 1)
        || block
            .get("offset")
            .and_then(Value::as_u64)
            .is_some_and(|offset| offset > 0)
}

/// Every top-level field a minimization rule can remove for this tool.
fn is_removable(key: &str) -> bool {
    super::pages::is_pagination_key(key)
        || matches!(
            key,
            "isPartial"
                | "filesSkipped"
                | "contentView"
                | "operation"
                | "type"
                | "snapshot"
                | "diagnostics"
                | "matchCount"
                | "matchedLineCount"
                | "filesScanned"
        )
}

fn minimize_data(data: &mut Map<String, Value>, tool: ToolId, query: &Value, echo: bool) {
    if data.get("isPartial") == Some(&Value::Bool(false)) {
        data.remove("isPartial");
    }
    if data.get("filesSkipped").and_then(Value::as_u64) == Some(0) {
        data.remove("filesSkipped");
    }
    if data.get("contentView").and_then(Value::as_str) == Some("none") {
        data.remove("contentView");
    }
    if !echo {
        for key in ["operation", "type", "owner", "repo"] {
            if data.get(key).is_some() && data.get(key) == query.get(key) {
                data.remove(key);
            }
        }
    }
    data.remove("snapshot");
    if let Some(diagnostics) = data.get_mut("diagnostics").and_then(Value::as_array_mut) {
        diagnostics.retain(|entry| entry["severity"] != "info");
        if diagnostics.is_empty() {
            data.remove("diagnostics");
        }
    }
    // Nested items' blocks carry no cursor either: `next` holds every page.
    let limited = data.contains_key("providerLimit");
    for block in super::pages::page_blocks_deep_mut(data) {
        if let Some(block) = block.as_object_mut() {
            super::pages::slim_pagination(block, limited);
        }
    }
    // A later page keeps its pagination: it names where the page starts
    // (and a byte window's unit, which keeps its text verbatim).
    // The `echo` extra keeps the pagination of a finished single page: the
    // caller asked to see what the request resolved to, `hasMore:false`
    // included.
    data.retain(|key, value| {
        !super::pages::is_pagination_key(key)
            || echo
            || super::pages::retain_page_blocks(key, value, walks_on)
    });
    // The stats extra asks for every total.
    if reports_scan_totals(tool, data) && !super::extras::wants(query, ExtraKind::Stats) {
        minimize_search_totals(data);
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
    VARIANTS[tool.index()].get_or_init(|| {
        let Ok(tool) = crate::contracts::tool_contract(tool) else {
            return Vec::new();
        };
        data_variants_of(&tool["outputSchema"])
    })
}

use crate::contracts::{property_schemas, resolve_ref};

fn data_variants_of(schema: &Value) -> Vec<std::collections::BTreeSet<String>> {
    let defs = &schema["$defs"];
    property_schemas(schema, defs, "results")
        .into_iter()
        .flat_map(|results| {
            let results = resolve_ref(results, defs);
            property_schemas(&results["items"], defs, "data")
        })
        .flat_map(|data| variants_of(data, defs, &Default::default(), 0))
        .collect()
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

/// Whether the row `data` carries scan totals that default rows trim to the
/// ones the page needs and `extras:["stats"]` keeps whole: the contract's
/// `scanTotals` for the tool says which rows.
fn reports_scan_totals(tool: ToolId, data: &Map<String, Value>) -> bool {
    use crate::id::ScanTotals;
    match tool.scan_totals() {
        Some(ScanTotals::Always) => true,
        Some(ScanTotals::WithMatchCount) => data.contains_key("matchCount"),
        None => false,
    }
}

/// Search totals, flat on `data`. The occurrence and matched-line totals
/// stay on every page, so a view never needs a sum over its rows to say how
/// much matched; the scan scope (`filesScanned`) stays only when nothing
/// matched (it shows the search ran where intended). Coverage limits
/// (`partialReasons`, `terminalLimit`), `binarySkipped` and unreadable-path
/// errors (`fileErrors`) remain public even on the final page: they explain
/// evidence that was not returned.
fn minimize_search_totals(data: &mut Map<String, Value>) {
    let has_rows = data
        .get("files")
        .and_then(Value::as_array)
        .is_some_and(|files| !files.is_empty());
    let keep = |key: &str| {
        if has_rows {
            matches!(key, "matchCount" | "matchedLineCount")
        } else {
            matches!(key, "matchCount" | "filesScanned")
        }
    };
    for key in ["matchCount", "matchedLineCount", "filesScanned"] {
        if !keep(key) {
            data.remove(key);
        }
    }
}

/// Row `errorCode`s that reject the caller's input (CLI exit 2): the request
/// is wrong, so broadening or retrying it cannot help.
pub fn is_invalid_input_code(code: &str) -> bool {
    crate::id::error_class(code) == crate::id::error_codes::ErrorClass::InvalidInput
}

/// Build the public envelope for executed rows; `queries` maps each row
/// position to its validated query (`None` for a rejected row).
///
/// The tool's [`PathAnchor`] decides how file paths are named. Local file
/// paths are anchored on the workspace root: a path inside it is emitted
/// relative to it, so a copied row path resolves as-is, and `root` is that
/// root; a path outside it stays absolute.
pub(crate) fn envelope_in(
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
    if anchor == PathAnchor::Common {
        return envelope(tool, rows);
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
            relativize_continuation_paths(&mut row["data"], workspace);
        }
    }
    // The checked-out commit is provenance: read only for rows that ask
    // for receipts.
    let receipts = rows
        .iter()
        .enumerate()
        .map(|(position, row)| {
            let index = row["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(position);
            queries.get(index).copied().flatten().is_some_and(|query| {
                super::extras::wants(query, octocode_contract_types::ExtraKind::Receipts)
            })
        })
        .collect::<Vec<_>>();
    let heads = checked_out_heads(tool, &roots, &receipts);
    let mut value = compact(tool, rows, workspace);
    attach_heads(&mut value, &heads);
    value
}

/// HEAD of each receipts row's root for the tools that read the working
/// tree directly; empty for every other tool. A root is read once per
/// response.
fn checked_out_heads(
    tool: ToolId,
    roots: &[Option<PathBuf>],
    receipts: &[bool],
) -> Vec<Option<String>> {
    if !tool.output().reads_worktree() || !receipts.contains(&true) {
        return Vec::new();
    }
    let mut seen = std::collections::HashMap::<&Path, Option<String>>::new();
    roots
        .iter()
        .zip(receipts)
        .map(|(root, wanted)| {
            let root = root.as_deref().filter(|_| *wanted)?;
            seen.entry(root)
                .or_insert_with(|| crate::git_head::head_sha(root))
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
fn relativize_continuation_paths(value: &mut Value, workspace: &str) {
    crate::response::result::for_each_call_mut(value, &mut |call| {
        for row in call
            .pointer_mut("/query/queries")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if let Some(Value::String(path)) = row.get_mut("path")
                && let Some(relative) = workspace_relative(path, workspace)
            {
                *path = relative;
            }
        }
    });
}

/// Whether a continuation (`{tool, query}` at any depth) names a relative
/// local `path`: it resolves against the workspace root, which the response
/// then names as `root` even when no row carries a path (an empty row).
fn relative_continuation_path(value: &Value) -> bool {
    let mut relative = false;
    crate::response::result::for_each_call(value, &mut |call| {
        relative |= call
            .pointer("/query/queries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|row| {
                row.get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| !Path::new(path).is_absolute() && !path.contains("://"))
            });
    });
    relative
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

pub fn envelope(tool: ToolId, rows: Vec<Value>) -> Value {
    let mut paths = Vec::new();
    for row in &rows {
        visit_paths(&row["data"], 0, &mut paths);
    }
    let base = common_directory(&paths);
    compact(tool, rows, base)
}

/// Rewrite every absolute row path under `base` relative to it (the base
/// itself becomes `.`), hoist `tool`'s shared leaf fields, and wrap the rows. Paths
/// outside `base` stay absolute; without such a path, or a continuation
/// whose relative `path` resolves against `base`, there is no `base`.
fn compact(tool: ToolId, mut rows: Vec<Value>, base: Option<String>) -> Value {
    let base = base.filter(|base| {
        let inside = format!("{base}/");
        let mut paths = Vec::new();
        for row in &rows {
            visit_paths(&row["data"], 0, &mut paths);
        }
        paths
            .iter()
            .any(|path| path == base || path.starts_with(&inside))
            || rows
                .iter()
                .any(|row| relative_continuation_path(&row["data"]))
    });
    for row in &mut rows {
        rewrite_paths(&mut row["data"], 0, base.as_deref());
    }
    let shared = crate::contracts::shared_fields::hoist(tool, &mut rows);
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
            } else if let Some(Value::String(path)) = map.get_mut("path")
                && let Some(clean) = without_dot_segments(path)
            {
                *path = clean;
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

/// A relative `path` without its `.` segments (`./src/a.ts` → `src/a.ts`),
/// so a row names a file the way the tools spell it whatever the caller
/// typed; `None` when there is nothing to drop. `..` stays: collapsing it is
/// not lexical across symlinks.
fn without_dot_segments(path: &str) -> Option<String> {
    if Path::new(path).is_absolute() || !path.split('/').any(|segment| segment == ".") {
        return None;
    }
    let kept = path
        .split('/')
        .filter(|segment| *segment != ".")
        .collect::<Vec<_>>()
        .join("/");
    Some(if kept.is_empty() {
        ".".to_owned()
    } else {
        kept
    })
}

#[cfg(test)]
mod tests;
