//! Server selection metadata: operation → provider capability, what a Rust
//! server's options make it analyze, the provider context of an answered
//! row, and the server receipt of a row where the server's state decides the
//! next step.

use super::anchor::is_anchored;
use super::{Exit, Op};
use crate::lsp_search::lsp_search_query;
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

/// What a Rust server's options make it run.
#[derive(Clone, Copy)]
pub(super) struct RustAnalysis {
    pub(super) build_scripts: bool,
    pub(super) all_features: bool,
    proc_macros: bool,
    /// The target it analyzes for (`cargo.target`, else the host).
    pub(super) target: super::server_coverage::AnalysisTarget,
}

impl RustAnalysis {
    pub(super) fn of(config: &LspLanguageServerConfig) -> Self {
        Self::from_options(config.initialization_options.as_ref())
    }

    /// Reads each setting as the server reads it (flat dotted key, then
    /// nested), so the checks see what rust-analyzer will run.
    fn from_options(options: Option<&Value>) -> Self {
        let setting = |section: &str| {
            options.and_then(|value| octocode_engine::lsp::lookup_section(value, section))
        };
        Self {
            build_scripts: setting("cargo.buildScripts.enable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            all_features: setting("cargo.features").and_then(Value::as_str) == Some("all"),
            proc_macros: setting("procMacro.enable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            target: super::server_coverage::AnalysisTarget::of(
                setting("cargo.target").and_then(Value::as_str),
            ),
        }
    }

    fn validate(self) -> Result<(), String> {
        if self.proc_macros && !self.build_scripts {
            return Err(
                "rust-analyzer's procMacro.enable requires cargo.buildScripts.enable: it builds procedural macros through Cargo."
                    .into(),
            );
        }
        Ok(())
    }
}

/// Refuse a Rust server whose configured options cannot run together
/// ([`RustAnalysis::validate`]); any other server passes.
pub(super) fn validate_rust_options(config: &LspLanguageServerConfig) -> Result<(), String> {
    if config.language_id.as_deref() == Some("rust") {
        RustAnalysis::of(config).validate()?;
    }
    Ok(())
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
/// A server answer carries no `source` (`lsp.level` names the engine); a
/// degraded source another stage set (a native scan that stood in for the
/// server) is kept: it is what the row's evidence rests on.
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
mod option_spelling_tests;
