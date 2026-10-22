//! Server selection metadata: operation → provider capability, the Rust
//! build-context overlay, and the provider receipt attached to every row.

use super::LspSearchQuery;
use super::anchor::is_anchored;
use crate::tools::lsp_search::lsp_search_query;
use octocode_engine::digest::canonical_json_keep_nulls;
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::types::LspLanguageServerConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn provider_for_operation(operation: &str) -> Option<&'static str> {
    match operation {
        "definition" => Some("definitionProvider"),
        "references" => Some("referencesProvider"),
        "hover" => Some("hoverProvider"),
        "typeDefinition" => Some("typeDefinitionProvider"),
        "implementation" => Some("implementationProvider"),
        "documentSymbols" => Some("documentSymbolProvider"),
        "workspaceSymbol" => Some("workspaceSymbolProvider"),
        "diagnostic" => Some("diagnosticProvider"),
        "callers" | "callees" => Some("callHierarchyProvider"),
        "supertypes" | "subtypes" => Some("typeHierarchyProvider"),
        _ => None,
    }
}

pub(super) fn required_capability(operation: &str) -> Option<&'static str> {
    // Diagnostics may arrive as server pushes even when diagnosticProvider is
    // not advertised. Every other operation uses the same provider metadata
    // for capability admission and for the returned receipt.
    (operation != "diagnostic")
        .then(|| provider_for_operation(operation))
        .flatten()
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct RustBuildContext {
    #[serde(default)]
    features: Option<Value>,
    #[serde(default)]
    no_default_features: Option<bool>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    cfgs: Option<Vec<String>>,
    #[serde(default)]
    build_scripts: Option<bool>,
    #[serde(default)]
    proc_macros: Option<bool>,
}

/// rust-analyzer's own defaults for the `rustContext` keys the headless
/// defaults do not set: an unset key and an explicit default are the same
/// analysis, so neither changes the server configuration.
const RUST_ANALYZER_DEFAULT_CFGS: [&str; 2] = ["debug_assertions", "miri"];

/// Overlay an explicit `rustContext` onto the resolved rust-analyzer options.
/// The engine already merged its headless defaults (`cargo.buildScripts`,
/// `procMacro`, `checkOnSave`, `cachePriming`, `cargo.targetDir`) into the
/// config. A context overrides only the fields it names, and only where the
/// named value differs from the effective one (the configured value, else
/// rust-analyzer's default): everything else, `cfg(test)` code included,
/// stays as the default server analyzes it. So a context equal to the
/// defaults leaves the configuration, and with it the pooled server,
/// unchanged. `cfgs` adds to the effective list (`"!name"` disables one),
/// as rust-analyzer's own list replaces its defaults.
pub(super) fn apply_rust_context(
    config: &mut LspLanguageServerConfig,
    query: &LspSearchQuery,
) -> Result<(), String> {
    let Some(value) = &lsp_search_query::rust_context(query) else {
        return Ok(());
    };
    if config.language_id.as_deref() != Some("rust") {
        return Err("rustContext requires a Rust language server.".into());
    }
    let context: RustBuildContext =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if context.proc_macros == Some(true) && context.build_scripts != Some(true) {
        return Err(
            "Rust procMacros requires buildScripts:true; rust-analyzer builds procedural macros through Cargo."
                .into(),
        );
    }
    let mut options = match config.initialization_options.clone() {
        Some(options @ Value::Object(_)) => options,
        _ => json!({}),
    };
    let mut set = |pointer: &str, value: Value, default: Value| {
        if options.pointer(pointer).unwrap_or(&default) != &value {
            set_pointer(&mut options, pointer, value);
        }
    };
    if let Some(features) = &context.features {
        set("/cargo/features", features.clone(), json!([]));
    }
    if let Some(off) = context.no_default_features {
        set("/cargo/noDefaultFeatures", json!(off), json!(false));
    }
    if let Some(target) = &context.target {
        set("/cargo/target", json!(target), Value::Null);
    }
    if let Some(build_scripts) = context.build_scripts {
        set(
            "/cargo/buildScripts/enable",
            json!(build_scripts),
            json!(false),
        );
    }
    if let Some(proc_macros) = context.proc_macros {
        set("/procMacro/enable", json!(proc_macros), json!(false));
    }
    if let Some(cfgs) = &context.cfgs {
        let effective = options
            .pointer("/cargo/cfgs")
            .and_then(Value::as_array)
            .map(|cfgs| {
                cfgs.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_else(|| RUST_ANALYZER_DEFAULT_CFGS.map(str::to_owned).to_vec());
        let mut merged = effective.clone();
        for cfg in cfgs {
            if !merged.contains(cfg) {
                merged.push(cfg.clone());
            }
        }
        if merged != effective {
            set_pointer(&mut options, "/cargo/cfgs", json!(merged));
        }
    }
    // Code execution stays off unless the context opted in: a server the
    // engine did not recognize as rust-analyzer gets the same guards.
    for (pointer, guard) in [
        ("/checkOnSave", json!(false)),
        ("/cargo/targetDir", json!(true)),
    ] {
        if options.pointer(pointer).is_none() {
            set_pointer(&mut options, pointer, guard);
        }
    }
    config.initialization_options = Some(options);
    Ok(())
}

/// Set `value` at a `/a/b` JSON pointer, creating objects on the way.
fn set_pointer(root: &mut Value, pointer: &str, value: Value) {
    let mut node = root;
    for key in pointer.trim_start_matches('/').split('/') {
        if !node.is_object() {
            *node = json!({});
        }
        let Value::Object(fields) = node else {
            return;
        };
        node = fields.entry(key.to_owned()).or_insert(Value::Null);
    }
    *node = value;
}

pub(super) fn rust_fingerprint(context: &Value) -> String {
    format!("rust-v1:{}", crate::digest::json_sha256(context))
}

pub(super) fn attach_provider_context(
    value: &mut Value,
    query: &LspSearchQuery,
    canonical_uri: &str,
    resolved_symbol: Option<Value>,
    config: &LspLanguageServerConfig,
    client: &NativeLspClient,
    debug: bool,
) {
    let Some(envelope) = value.as_object_mut() else {
        return;
    };
    envelope.insert(
        "path".into(),
        json!(super::render::uri_to_path(canonical_uri)),
    );
    let anchored = is_anchored(lsp_search_query::operation(query));
    // The row's `path` names the anchor file once; the anchor receipt and a
    // locations restate it only when they point elsewhere.
    if let Some(resolved_symbol) = public_resolved_symbol(query, resolved_symbol, canonical_uri) {
        envelope.insert("resolvedSymbol".into(), resolved_symbol);
    }
    let lsp = envelope
        .entry("lsp")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(lsp) = lsp {
        lsp.insert("serverAvailable".into(), json!(true));
        settle_provider(lsp, lsp_search_query::operation(query));
        settle_source(lsp, anchored);
        // `lsp.receipt` is verbose (core field class): the verbose stage
        // drops it unless the row's `extras` names `receipts`.
        let mut receipt = resolved_server_receipt(config, client);
        // Anchored rows already carry `workspaceRoot` at the top level.
        if anchored && let Some(receipt) = receipt.as_object_mut() {
            receipt.remove("workspaceRoot");
        }
        lsp.insert("receipt".into(), receipt);
    }
    // The provider receipt answers "how", not "what": a healthy language-server
    // answer omits it unless `debug` asks. A degraded source stays visible.
    if !debug
        && envelope.get("lsp").is_some_and(|lsp| {
            lsp["serverAvailable"] == true && lsp.get("source").is_none_or(|source| source == "lsp")
        })
    {
        envelope.shift_remove("lsp");
    }
    if anchored {
        let mut ordered = serde_json::Map::new();
        for key in ["path", "resolvedSymbol", "lsp"] {
            if let Some(value) = envelope.shift_remove(key) {
                ordered.insert(key.into(), value);
            }
        }
        ordered.append(envelope);
        ordered.insert("workspaceRoot".into(), json!(config.workspace_root));
        *envelope = ordered;
    }
}

/// The anchor receipt a row shows: what the request did not say, the line a
/// symbol moved to off its `line`. A symbol found on its `line`
/// (only its column is new) restates the request: no receipt.
/// A server answer is `source:"lsp"`, implied on anchored rows and stated
/// on the others. A degraded source another stage set (a native scan that
/// stood in for the server) is kept: it is what the row's evidence rests on.
/// The row's provider is the capability that answered: a fallback names its
/// own (`implementationProvider` for `subtypes`); every other row names the
/// operation's.
pub(super) fn settle_provider(lsp: &mut serde_json::Map<String, Value>, operation: &str) {
    if lsp.get("provider").is_some_and(Value::is_string) {
        return;
    }
    if let Some(provider) = provider_for_operation(operation) {
        lsp.insert("provider".into(), json!(provider));
    }
}

fn settle_source(lsp: &mut serde_json::Map<String, Value>, anchored: bool) {
    if lsp
        .get("source")
        .and_then(Value::as_str)
        .is_some_and(|source| source != "lsp")
    {
        return;
    }
    if anchored {
        lsp.remove("source");
    } else {
        lsp.insert("source".into(), json!("lsp"));
    }
}

pub(super) fn public_resolved_symbol(
    query: &LspSearchQuery,
    resolved_symbol: Option<Value>,
    canonical_uri: &str,
) -> Option<Value> {
    let mut resolved_symbol = resolved_symbol?;
    drop_same_uri(&mut resolved_symbol, canonical_uri);
    let moved = lsp_search_query::symbol_name(query).is_some()
        && resolved_symbol.as_object().is_some_and(|fields| {
            fields
                .keys()
                .any(|key| !matches!(key.as_str(), "foundAtCharacter" | "orderHint"))
        });
    moved.then_some(resolved_symbol)
}

/// Remove `value.path` (or an internal `uri`) when it names the same file
/// as `canonical_uri`.
pub(super) fn drop_same_uri(value: &mut Value, canonical_uri: &str) {
    let file = |uri: &str| {
        url::Url::parse(uri)
            .ok()
            .and_then(|url| url.to_file_path().ok())
            .or_else(|| {
                let path = std::path::PathBuf::from(uri);
                path.is_absolute().then_some(path)
            })
    };
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in ["uri", "path"] {
        let same = object.get(key).and_then(Value::as_str).is_some_and(|uri| {
            uri == canonical_uri || file(uri).is_some_and(|path| Some(path) == file(canonical_uri))
        });
        if same {
            object.shift_remove(key);
        }
    }
}

/// Spell the server's `workspaceRoot` (on the row, and in its receipt)
/// relative to the workspace like every row path; a root outside it stays
/// absolute.
pub(super) fn relative_server_roots(row: &mut Value, paths: &crate::policy::path::PathPolicy) {
    for pointer in ["/workspaceRoot", "/lsp/receipt/workspaceRoot"] {
        if let Some(root) = row.pointer_mut(pointer)
            && let Some(relative) = root
                .as_str()
                .and_then(|root| paths.workspace_relative(root))
        {
            *root = json!(relative);
        }
    }
}

pub(super) fn resolved_server_receipt(
    config: &LspLanguageServerConfig,
    client: &NativeLspClient,
) -> Value {
    const CAPABILITIES: [&str; 10] = [
        "definitionProvider",
        "typeDefinitionProvider",
        "implementationProvider",
        "referencesProvider",
        "hoverProvider",
        "callHierarchyProvider",
        "typeHierarchyProvider",
        "documentSymbolProvider",
        "workspaceSymbolProvider",
        "diagnosticProvider",
    ];
    let capabilities = CAPABILITIES
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                json!(client.has_capability(name.to_owned())),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let workspace = Path::new(&config.workspace_root)
        .canonicalize()
        .unwrap_or_else(|_| Path::new(&config.workspace_root).to_path_buf())
        .to_string_lossy()
        .into_owned();
    let configuration = canonical_json_keep_nulls(json!([
        config
            .initialization_options
            .clone()
            .unwrap_or_else(|| json!({})),
        config.env.clone().unwrap_or_default()
    ]));
    let mut receipt = json!({
        "command": config.command,
        "source": config.source.as_str(),
        "workspaceRoot": config.workspace_root,
        "workspaceFingerprint": octocode_engine::digest::sha256(workspace.as_bytes()),
        "configurationFingerprint": crate::digest::json_sha256(&configuration),
        "capabilities": capabilities
    });
    if let Some(argv) = config.args.as_ref().filter(|argv| !argv.is_empty()) {
        receipt["argv"] = json!(argv);
    }
    if let Some(readiness) = client.readiness() {
        receipt["readiness"] = json!(readiness);
    }
    receipt
}

#[cfg(test)]
mod tests {
    use super::settle_source;
    use serde_json::{Value, json};

    fn settled(lsp: Value, anchored: bool) -> Value {
        let mut lsp = lsp.as_object().cloned().expect("an lsp object");
        settle_source(&mut lsp, anchored);
        Value::Object(lsp)
    }

    /// A native scan that answered for the server keeps its label on every
    /// row, anchored or not; a server answer is implied on anchored rows.
    #[test]
    fn a_degraded_source_survives_the_receipt_stage() {
        for anchored in [true, false] {
            let row = settled(json!({"source": "native-graph-facts"}), anchored);
            assert_eq!(row["source"], "native-graph-facts", "anchored={anchored}");
        }
        assert!(
            settled(json!({"source": "lsp"}), true)
                .get("source")
                .is_none()
        );
        assert!(settled(json!({}), true).get("source").is_none());
        assert_eq!(settled(json!({}), false)["source"], "lsp");
    }
}
