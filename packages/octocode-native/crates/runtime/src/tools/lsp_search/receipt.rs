//! Server selection metadata: operation → provider capability, the Rust
//! build-context overlay, and the provider receipt attached to every row.

use super::LspSearchQuery;
use super::anchor::is_anchored;
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::pool::canonical_json;
use octocode_engine::lsp::types::JsLanguageServerConfig;
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
        "callers" | "callees" | "callHierarchy" => Some("callHierarchyProvider"),
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

/// Overlay an explicit `rustContext` onto the resolved rust-analyzer options.
/// The engine already merged its headless defaults (`cargo.buildScripts`,
/// `procMacro`, `checkOnSave`, `cachePriming`, `cargo.targetDir`) into the
/// config; this sets the same keys, so code execution stays off unless the
/// caller opted in with `buildScripts`/`procMacros`, and every other default
/// (for example `cachePriming`) is kept.
pub(super) fn apply_rust_context(
    config: &mut JsLanguageServerConfig,
    query: &LspSearchQuery,
) -> Result<(), String> {
    let Some(value) = &query.rust_context() else {
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
    let mut cargo = match options.get("cargo") {
        Some(cargo @ Value::Object(_)) => cargo.clone(),
        _ => json!({}),
    };
    let features = match &context.features {
        Some(Value::String(value)) if value == "all" => json!("all"),
        Some(Value::Array(items)) => json!(items),
        _ => json!([]),
    };
    cargo["features"] = features;
    cargo["noDefaultFeatures"] = json!(context.no_default_features.unwrap_or(false));
    cargo["target"] = json!(context.target);
    cargo["cfgs"] = json!(context.cfgs.clone().unwrap_or_default());
    cargo["buildScripts"] = json!({ "enable": context.build_scripts.unwrap_or(false) });
    cargo["targetDir"] = json!(true);
    options["cargo"] = cargo;
    options["procMacro"] = json!({ "enable": context.proc_macros.unwrap_or(false) });
    options["cfg"] = json!({ "setTest": false });
    options["checkOnSave"] = json!(false);
    config.initialization_options = Some(options);
    Ok(())
}

pub(super) fn rust_fingerprint(context: &Value) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_vec(context).unwrap_or_default();
    format!("rust-v1:{}", hex::encode(Sha256::digest(canonical)))
}

pub(super) fn attach_provider_context(
    value: &mut Value,
    query: &LspSearchQuery,
    canonical_uri: &str,
    resolved_symbol: Option<Value>,
    config: &JsLanguageServerConfig,
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
    let anchored = is_anchored(&query.operation());
    // The row's `path` names the anchor file once; the anchor receipt and a
    // single-file payload restate it only when they point elsewhere.
    if let Some(resolved_symbol) = public_resolved_symbol(query, resolved_symbol, canonical_uri) {
        envelope.insert("resolvedSymbol".into(), resolved_symbol);
    }
    if let Some(payload) = envelope.get_mut("payload") {
        drop_same_uri(payload, canonical_uri);
    }
    let lsp = envelope
        .entry("lsp")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(lsp) = lsp {
        lsp.insert("serverAvailable".into(), json!(true));
        if let Some(provider) = provider_for_operation(&query.operation()) {
            lsp.insert("provider".into(), json!(provider));
        }
        if anchored {
            lsp.remove("source");
        } else {
            lsp.insert("source".into(), json!("lsp"));
        }
        // `lsp.receipt` is verbose (core field class): the verbose stage
        // drops it unless the row asked for `debug: true`.
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
/// symbol moved to off its `lineHint`. A `position` anchor, or a symbol found
/// on its `lineHint` (only its column is new), restates the request: no
/// receipt.
pub(super) fn public_resolved_symbol(
    query: &LspSearchQuery,
    resolved_symbol: Option<Value>,
    canonical_uri: &str,
) -> Option<Value> {
    let mut resolved_symbol = resolved_symbol?;
    drop_same_uri(&mut resolved_symbol, canonical_uri);
    let moved = query.symbol_name().is_some()
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

pub(super) fn resolved_server_receipt(
    config: &JsLanguageServerConfig,
    client: &NativeLspClient,
) -> Value {
    use sha2::{Digest, Sha256};

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
    let configuration = canonical_json(json!([
        config
            .initialization_options
            .clone()
            .unwrap_or_else(|| json!({})),
        config.env.clone().unwrap_or_default()
    ]));
    let source = if config
        .args
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|argument| argument.contains("node_modules"))
    {
        "bundled"
    } else {
        "path"
    };
    let mut receipt = json!({
        "command": config.command,
        "source": source,
        "workspaceRoot": config.workspace_root,
        "workspaceFingerprint": hex::encode(Sha256::digest(workspace.as_bytes())),
        "configurationFingerprint": hex::encode(Sha256::digest(
            serde_json::to_vec(&configuration).unwrap_or_default()
        )),
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
