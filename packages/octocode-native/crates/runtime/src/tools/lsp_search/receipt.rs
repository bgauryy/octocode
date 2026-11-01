//! Server selection metadata: operation → provider capability, the Rust
//! build-context overlay, the provider context of an answered row, and the
//! server receipt of a row where the server's state decides the next step.

use super::anchor::is_anchored;
use super::{Exit, Op};
use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::types::LspLanguageServerConfig;
use serde_json::{Value, json};

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
    // for capability admission and for the returned `provider`.
    (operation != "diagnostic")
        .then(|| provider_for_operation(operation))
        .flatten()
}

/// Whether a server advertising `advertises` can run `operation`: it needs
/// no capability, advertises the one it needs, or advertises the provider
/// its fallback uses.
pub(super) fn admits(operation: &str, advertises: &dyn Fn(&str) -> bool) -> bool {
    required_capability(operation).is_none_or(advertises)
        || super::heritage::fallback_provider(operation).is_some_and(advertises)
}

/// The operations [`admits`] lets through, spelled as the input spells
/// them, so a capability failure's row names what to retry with.
pub(super) fn advertised_operations(advertises: &dyn Fn(&str) -> bool) -> Vec<&'static str> {
    Op::ALL
        .into_iter()
        .map(Op::name)
        .filter(|operation| admits(operation, advertises))
        .collect()
}

/// The server a failed row names: its command, the argv when `argv` asks
/// for it (what a start failure needs to configure) and it has one, and
/// where discovery found it.
pub(super) fn server_receipt(config: &LspLanguageServerConfig, argv: bool) -> Value {
    let mut receipt = json!({"command": config.command});
    if argv && let Some(args) = config.args.as_ref().filter(|args| !args.is_empty()) {
        receipt["argv"] = json!(args);
    }
    receipt["source"] = json!(config.source.as_str());
    receipt
}

/// `exit` with `receipt` at `lsp.receipt` when it is an error row.
pub(super) fn naming(exit: Exit, receipt: Value) -> Exit {
    let Exit::Row(mut row) = exit else {
        return exit;
    };
    if let Some(row_fields) = row.as_object_mut()
        && let Some(lsp) = row_fields
            .entry("lsp")
            .or_insert_with(|| json!({}))
            .as_object_mut()
    {
        lsp.insert("receipt".into(), receipt);
    }
    Exit::Row(row)
}

/// rust-analyzer's own defaults for the `rustContext` keys the headless
/// defaults do not set: an unset key and an explicit default are the same
/// analysis, so neither changes the server configuration.
const RUST_ANALYZER_DEFAULT_CFGS: [&str; 2] = ["debug_assertions", "miri"];

#[derive(Clone, Copy)]
pub(super) struct RustAnalysis {
    pub(super) build_scripts: bool,
    pub(super) all_features: bool,
    proc_macros: bool,
}

impl RustAnalysis {
    pub(super) fn of(config: &LspLanguageServerConfig) -> Self {
        Self::from_options(config.initialization_options.as_ref())
    }

    fn from_options(options: Option<&Value>) -> Self {
        Self {
            build_scripts: options
                .and_then(|value| value.pointer("/cargo/buildScripts/enable"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            all_features: options
                .and_then(|value| value.pointer("/cargo/features"))
                .and_then(Value::as_str)
                == Some("all"),
            proc_macros: options
                .and_then(|value| value.pointer("/procMacro/enable"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    fn validate(self) -> Result<(), String> {
        if self.proc_macros && !self.build_scripts {
            return Err(
                "Rust procMacros requires buildScripts:true; rust-analyzer builds procedural macros through Cargo."
                    .into(),
            );
        }
        Ok(())
    }
}

/// Overlay an explicit `rustContext` onto the resolved rust-analyzer options.
/// The engine already merged its headless defaults (`cargo.buildScripts`,
/// `procMacro`, `checkOnSave`, `cargo.targetDir`) into the
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
    let Some(context) = lsp_search_query::rust_context(query) else {
        if config.language_id.as_deref() == Some("rust") {
            RustAnalysis::of(config).validate()?;
        }
        return Ok(());
    };
    if config.language_id.as_deref() != Some("rust") {
        return Err("rustContext requires a Rust language server.".into());
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
        set("/cargo/features", json!(features), json!([]));
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
    RustAnalysis::from_options(Some(&options)).validate()?;
    if !context.cfgs.is_empty() {
        let cfgs = context
            .cfgs
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
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
        for cfg in &cfgs {
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

pub(super) fn rust_fingerprint(context: &octocode_contract_types::LspRustContext) -> String {
    format!("rust-v1:{}", crate::digest::json_sha256(context))
}

pub(super) fn attach_provider_context(
    value: &mut Value,
    query: &LspSearchQuery,
    canonical_uri: &str,
    resolved_symbol: Option<Value>,
    config: &LspLanguageServerConfig,
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
    }
    if anchored {
        let mut ordered = serde_json::Map::new();
        for key in ["path", "resolvedSymbol", "lsp"] {
            if let Some(value) = envelope.shift_remove(key) {
                ordered.insert(key.into(), value);
            }
        }
        ordered.append(envelope);
        *envelope = ordered;
    }
    // Every row names the root its server searched: a workspace-wide
    // answer's scope, and an anchored answer's project.
    envelope.insert("workspaceRoot".into(), json!(config.workspace_root));
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
                .any(|key| !matches!(key.as_str(), "foundAtCharacter" | "occurrence"))
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

#[cfg(test)]
mod tests;
