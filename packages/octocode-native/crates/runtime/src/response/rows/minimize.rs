use crate::id::ToolId;
use octocode_contract_types::ExtraKind;
use serde_json::{Map, Value};

/// Default responses carry the answer and what the next call needs. Fields
/// core classes verbose are already gone (`super::super::verbose`); the
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
    let echo = super::super::extras::wants(query, ExtraKind::Echo);
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
            && (!super::super::pages::is_pagination_key(key)
                || super::super::pages::retain_page_blocks(key, value, walks_on))
    });
}

/// A pagination block that routes somewhere: more pages to read, a page
/// past the first (it names where the page starts, and a byte window's
/// unit), or a page requested past the end. The one presence rule: a tool
/// always states its block ([`super::super::pages::PageFacts`],
/// [`super::super::pages::SpanFacts`]) and the stage keeps it only where it routes
/// or under `extras: ["echo"]`.
fn walks_on(block: &Value) -> bool {
    block["hasMore"] == true
        || block["outOfRange"] == true
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
    super::super::pages::is_pagination_key(key)
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
    for block in super::super::pages::page_blocks_deep_mut(data) {
        if let Some(block) = block.as_object_mut() {
            super::super::pages::slim_pagination(block, limited);
        }
    }
    // A later page keeps its pagination: it names where the page starts
    // (and a byte window's unit, which keeps its text verbatim).
    // The `echo` extra keeps the pagination of a finished single page: the
    // caller asked to see what the request resolved to, `hasMore:false`
    // included.
    data.retain(|key, value| {
        !super::super::pages::is_pagination_key(key)
            || echo
            || super::super::pages::retain_page_blocks(key, value, walks_on)
    });
    // The stats extra asks for every total.
    if reports_scan_totals(tool, data) && !super::super::extras::wants(query, ExtraKind::Stats) {
        minimize_search_totals(data);
    }
}

/// The required top-level fields of each variant of the tool's data
/// contract (one set per union branch, common requirements merged in).
pub(super) fn contract_data_variants(
    tool: ToolId,
) -> &'static [std::collections::BTreeSet<String>] {
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

pub(super) fn data_variants_of(schema: &Value) -> Vec<std::collections::BTreeSet<String>> {
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
pub(super) fn reports_scan_totals(tool: ToolId, data: &Map<String, Value>) -> bool {
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
