//! Native `lspSearch` using the portable engine language-server client.
use crate::tools::local_fetch::CancellationCheck;
use octocode_engine_core::lsp::client::NativeLspClient;
use octocode_engine_core::lsp::config::default_server_for_file;
use octocode_engine_core::lsp::pool::LspClientPool;
use octocode_engine_core::lsp::resolver::resolve_position;
use octocode_engine_core::lsp::types::JsFuzzyPosition;
use octocode_engine_core::lsp::workspace::resolve_workspace_root_for_file;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspSearchQuery {
    pub operation: String,
    pub uri: Option<String>,
    pub workspace_root: Option<String>,
    pub symbol_name: Option<String>,
    pub line: Option<u32>,
    pub character: Option<u32>,
    pub line_hint: Option<u32>,
    pub order_hint: Option<u32>,
    pub include_declaration: Option<bool>,
    pub group_by_file: Option<bool>,
    pub format: Option<String>,
    pub depth: Option<u32>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub rust_context: Option<Value>,
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

pub async fn execute(
    query: Value,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
) -> Result<Value, String> {
    cancel.check()?;
    let mut query = query;
    if let Some(object) = query.as_object_mut() {
        object.remove("goal");
        object.remove("reasoning");
    }
    let query: LspSearchQuery = serde_json::from_value(query).map_err(|error| error.to_string())?;
    let path = query
        .uri
        .as_deref()
        .map(uri_to_path)
        .or_else(|| query.workspace_root.clone())
        .ok_or_else(|| "lspSearch requires uri or workspaceRoot".to_owned())?;
    let workspace = query
        .workspace_root
        .clone()
        .or_else(|| resolve_workspace_root_for_file(path.clone()).ok())
        .unwrap_or_else(|| {
            Path::new(&path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_else(|| ".".into())
        });
    if let Some(root) = query.workspace_root.as_deref()
        && !Path::new(root).is_dir()
    {
        return Ok(failure(
            &query,
            "lsp.workspaceRootInvalid",
            "workspaceRoot is not a directory.",
            false,
        ));
    }
    let Some(mut config) = default_server_for_file(path.clone(), workspace) else {
        return Ok(failure(
            &query,
            "lsp.serverUnavailable",
            "No language server is configured for this file.",
            false,
        ));
    };
    apply_rust_context(&mut config, &query)?;
    let client = match pool.acquire(config).await {
        Ok(Some(client)) => client,
        Ok(None) => {
            return Ok(failure(
                &query,
                "lsp.serverUnavailable",
                "Language server failed to start.",
                false,
            ));
        }
        Err(error) => {
            return Ok(failure(
                &query,
                "lsp.serverUnavailable",
                &error.to_string(),
                false,
            ));
        }
    };
    if client.readiness().as_deref() == Some("timeout") {
        return Ok(failure(
            &query,
            "lsp.timeout",
            "Timed out waiting for the language server to become ready.",
            false,
        ));
    }
    if Path::new(&path).is_file()
        && let Ok(content) = fs::read_to_string(&path)
    {
        let _ = client.open_document(path.clone(), content).await;
    }
    let (line, character) = match resolve_anchor(&query, &path) {
        Ok(anchor) => anchor,
        Err(error) => {
            return Ok(failure(&query, "lsp.anchorUnresolved", &error, true));
        }
    };
    let result = match query.operation.as_str() {
        "definition" => locations(
            &query,
            "definition",
            "definitionProvider",
            client
                .get_definition(path.clone(), line, character)
                .await
                .map_err(|error| error.to_string())?,
        ),
        "references" => {
            let snippets = client
                .get_references(
                    path.clone(),
                    line,
                    character,
                    Some(query.include_declaration.unwrap_or(true)),
                )
                .await
                .map_err(|error| error.to_string())?;
            let recovered =
                recover_aliases(&client, &query, &path, line, character, &snippets).await;
            let mut all = snippets;
            all.extend(recovered);
            locations(&query, "references", "referencesProvider", all)
        }
        "hover" => json!({
            "type": query.operation,
            "uri": query.uri,
            "lsp": { "serverAvailable": true, "source": "native", "provider": "hoverProvider" },
            "payload": { "kind": "hover", "hover": client.get_hover(path.clone(), line, character).await.map_err(|error| error.to_string())? }
        }),
        "typeDefinition" => locations(
            &query,
            "typeDefinition",
            "typeDefinitionProvider",
            client
                .get_type_definition(path.clone(), line, character)
                .await
                .map_err(|error| error.to_string())?,
        ),
        "implementation" => locations(
            &query,
            "implementation",
            "implementationProvider",
            client
                .get_implementation(path.clone(), line, character)
                .await
                .map_err(|error| error.to_string())?,
        ),
        "documentSymbols" => items_payload(
            &query,
            "symbols",
            client
                .get_document_symbols(path.clone())
                .await
                .map_err(|error| error.to_string())?,
        ),
        "workspaceSymbol" => {
            let name = query
                .symbol_name
                .clone()
                .ok_or_else(|| "workspaceSymbol requires symbolName".to_owned())?;
            items_payload(
                &query,
                "symbols",
                client
                    .workspace_symbol(name)
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
        "diagnostic" => items_payload(
            &query,
            "diagnostics",
            client
                .get_diagnostics(path.clone())
                .await
                .map_err(|error| error.to_string())?,
        ),
        "callers" | "callees" | "callHierarchy" => {
            hierarchy(&client, &query, &path, line, character).await?
        }
        "supertypes" | "subtypes" => types(&client, &query, &path, line, character).await?,
        other => empty(
            &query,
            "unsupportedOperation",
            &format!("Unsupported lspSearch operation: {other}"),
            true,
        ),
    };
    Ok(with_next(&query, result))
}

fn apply_rust_context(
    config: &mut octocode_engine_core::lsp::types::JsLanguageServerConfig,
    query: &LspSearchQuery,
) -> Result<(), String> {
    let Some(value) = &query.rust_context else {
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
    let mut options = config
        .initialization_options
        .clone()
        .unwrap_or_else(|| json!({}));
    let mut cargo = options.get("cargo").cloned().unwrap_or_else(|| json!({}));
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

fn resolve_anchor(query: &LspSearchQuery, path: &str) -> Result<(u32, u32), String> {
    if let Some(line) = query.line {
        return Ok((line, query.character.unwrap_or(0)));
    }
    if let Some(name) = query.symbol_name.as_deref() {
        let resolved = resolve_position(
            path.to_owned(),
            JsFuzzyPosition {
                symbol_name: name.to_owned(),
                line_hint: query.line_hint,
                order_hint: query.order_hint,
            },
        )
        .map_err(|error| error.to_string())?;
        return Ok((resolved.position.line, resolved.position.character));
    }
    Ok((query.line_hint.unwrap_or(0), query.character.unwrap_or(0)))
}

fn with_next(query: &LspSearchQuery, mut value: Value) -> Value {
    if value
        .pointer("/pagination/hasMore")
        .and_then(Value::as_bool)
        == Some(true)
    {
        let mut next_query = serde_json::to_value(query).unwrap_or(json!({}));
        if let Some(object) = next_query.as_object_mut() {
            object.insert(
                "page".into(),
                json!(
                    value
                        .pointer("/pagination/nextPage")
                        .and_then(Value::as_u64)
                        .unwrap_or(2)
                ),
            );
        }
        value["next"]["nextPage"] = json!({
            "tool": "lspSearch",
            "query": next_query,
            "confidence": "exact"
        });
    } else if value.pointer("/payload/kind").and_then(Value::as_str) == Some("empty") {
        value["status"] = json!("empty");
        attach_recovery_next(&mut value, query);
    }
    if let Some(context) = &query.rust_context {
        value["rustContext"] = json!({
            "context": context,
            "fingerprint": rust_fingerprint(context)
        });
    }
    value
}

fn rust_fingerprint(context: &Value) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_vec(context).unwrap_or_default();
    format!("rust-v1:{:x}", Sha256::digest(canonical))
}

async fn hierarchy(
    client: &NativeLspClient,
    query: &LspSearchQuery,
    path: &str,
    line: u32,
    character: u32,
) -> Result<Value, String> {
    let prepared = client
        .prepare_call_hierarchy(path.to_owned(), line, character)
        .await
        .map_err(|error| error.to_string())?;
    let roots = as_array(&prepared);
    let depth = query.depth.unwrap_or(1).max(1);
    let mut incoming = Vec::new();
    let mut outgoing = Vec::new();
    for item in &roots {
        if matches!(query.operation.as_str(), "callers" | "callHierarchy") {
            incoming.extend(walk_calls(client, item.clone(), true, depth).await);
        }
        if matches!(query.operation.as_str(), "callees" | "callHierarchy") {
            outgoing.extend(walk_calls(client, item.clone(), false, depth).await);
        }
    }
    let items = match query.operation.as_str() {
        "callers" => incoming,
        "callees" => outgoing,
        _ => {
            let mut both = incoming;
            both.extend(outgoing);
            both
        }
    };
    Ok(items_payload(query, query.operation.as_str(), json!(items)))
}

async fn walk_calls(
    client: &NativeLspClient,
    root: Value,
    incoming: bool,
    depth: u32,
) -> Vec<Value> {
    let mut items = Vec::new();
    let mut frontier = vec![(root, 1u32)];
    let mut seen = std::collections::HashSet::new();
    while let Some((item, level)) = frontier.pop() {
        let key = item.to_string();
        if !seen.insert(key) {
            continue;
        }
        let next = if incoming {
            client.incoming_calls(item.clone()).await
        } else {
            client.outgoing_calls(item.clone()).await
        }
        .unwrap_or_else(|_| Value::Array(vec![]));
        for call in as_array(&next) {
            items.push(call.clone());
            if level < depth
                && let Some(from) = call
                    .get("from")
                    .cloned()
                    .or_else(|| call.get("to").cloned())
            {
                frontier.push((from, level + 1));
            }
        }
    }
    items
}

async fn types(
    client: &NativeLspClient,
    query: &LspSearchQuery,
    path: &str,
    line: u32,
    character: u32,
) -> Result<Value, String> {
    let prepared = client
        .prepare_type_hierarchy(path.to_owned(), line, character)
        .await
        .map_err(|error| error.to_string())?;
    let roots = as_array(&prepared);
    let mut items = Vec::new();
    for item in roots {
        let next = if query.operation == "supertypes" {
            client.type_hierarchy_supertypes(item).await
        } else {
            client.type_hierarchy_subtypes(item).await
        }
        .unwrap_or_else(|_| Value::Array(vec![]));
        items.extend(as_array(&next));
    }
    Ok(items_payload(query, query.operation.as_str(), json!(items)))
}

fn locations(
    query: &LspSearchQuery,
    kind: &str,
    provider: &str,
    snippets: Vec<impl serde::Serialize>,
) -> Value {
    let mut locations = snippets
        .into_iter()
        .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
        .collect::<Vec<_>>();
    if query.format.as_deref() == Some("compact") {
        locations = locations.into_iter().map(compact_location).collect();
    }
    if locations.is_empty() {
        return empty(
            query,
            "noLocations",
            &format!("{provider} returned no locations"),
            true,
        );
    }
    let (page, pagination) = paginate(
        &locations,
        query.page.unwrap_or(1),
        query.page_size.unwrap_or(40),
    );
    let mut payload = json!({ "kind": kind, "locations": page });
    if query.group_by_file == Some(true) {
        payload["byFile"] = group_by_file(&locations);
    }
    json!({
        "type": query.operation,
        "uri": query.uri,
        "lsp": { "serverAvailable": true, "source": "native", "provider": provider },
        "payload": payload,
        "pagination": pagination
    })
}

fn compact_location(value: Value) -> Value {
    json!({
        "uri": value.get("uri"),
        "path": value.get("uri").and_then(Value::as_str).map(uri_to_path),
        "range": value.get("range"),
        "line": value.pointer("/range/start/line"),
        "character": value.pointer("/range/start/character")
    })
}

fn group_by_file(locations: &[Value]) -> Value {
    let mut files = serde_json::Map::new();
    for location in locations {
        let key = location
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        files
            .entry(key)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .into_iter()
            .for_each(|rows| rows.push(location.clone()));
    }
    Value::Object(files)
}

async fn recover_aliases(
    client: &NativeLspClient,
    query: &LspSearchQuery,
    path: &str,
    line: u32,
    character: u32,
    provider: &[octocode_engine_core::lsp::types::JsCodeSnippet],
) -> Vec<octocode_engine_core::lsp::types::JsCodeSnippet> {
    let Some(symbol) = query.symbol_name.as_deref() else {
        return Vec::new();
    };
    let Ok(definitions) = client
        .get_definition(path.to_owned(), line, character)
        .await
    else {
        return Vec::new();
    };
    let definition_ids: std::collections::HashSet<_> =
        definitions.iter().map(snippet_identity).collect();
    if definition_ids.is_empty() {
        return Vec::new();
    }
    let provider_ids: std::collections::HashSet<_> =
        provider.iter().map(snippet_identity).collect();
    let mut files: Vec<String> = provider
        .iter()
        .map(|snippet| uri_to_path(&snippet.uri))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .take(100)
        .collect();
    files.sort();
    let mut recovered = Vec::new();
    let mut inspected = 0usize;
    for file in files {
        if inspected >= 32 {
            break;
        }
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };
        let Some(facts) = octocode_engine_core::portable::extract_graph_facts(&source, &file)
        else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&facts) else {
            continue;
        };
        for import in parsed
            .get("imports")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let imported = import.get("importedName").and_then(Value::as_str);
            let local = import.get("localName").and_then(Value::as_str);
            if imported != Some(symbol) || local.is_none() || local == imported {
                continue;
            }
            let Some(imported_range) = import.get("importedRange") else {
                continue;
            };
            let Some(local_range) = import.get("localRange") else {
                continue;
            };
            let imported_id = format!(
                "{}:{}:{}",
                file,
                imported_range
                    .pointer("/start/line")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                imported_range
                    .pointer("/start/character")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            );
            if !provider.iter().any(|snippet| {
                uri_to_path(&snippet.uri) == file
                    && snippet_identity(snippet).ends_with(&imported_id[file.len()..])
            }) && !provider_ids.iter().any(|id| id.contains(&imported_id))
            {
                // Keep going; identity formats differ by uri vs path.
            }
            let local_line = local_range
                .pointer("/start/line")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let local_character = local_range
                .pointer("/start/character")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            inspected += 1;
            let Ok(targets) = client
                .get_definition(file.clone(), local_line, local_character)
                .await
            else {
                continue;
            };
            let target_ids: std::collections::HashSet<_> =
                targets.iter().map(snippet_identity).collect();
            if target_ids != definition_ids {
                continue;
            }
            if let Ok(extra) = client
                .get_references(
                    file.clone(),
                    local_line,
                    local_character,
                    Some(query.include_declaration.unwrap_or(true)),
                )
                .await
            {
                for snippet in extra {
                    if provider_ids.contains(&snippet_identity(&snippet)) {
                        continue;
                    }
                    recovered.push(snippet);
                }
            }
        }
    }
    recovered
}

fn snippet_identity(snippet: &octocode_engine_core::lsp::types::JsCodeSnippet) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        uri_to_path(&snippet.uri),
        snippet.range.start.line,
        snippet.range.start.character,
        snippet.range.end.line,
        snippet.range.end.character
    )
}

fn items_payload(query: &LspSearchQuery, kind: &str, value: Value) -> Value {
    let items = as_array(&value);
    if items.is_empty() {
        return empty(
            query,
            "noLocations",
            &format!("{kind} returned no results"),
            true,
        );
    }
    let (page, pagination) = paginate(
        &items,
        query.page.unwrap_or(1),
        query.page_size.unwrap_or(40),
    );
    json!({
        "type": query.operation,
        "uri": query.uri,
        "lsp": { "serverAvailable": true, "source": "native" },
        "payload": { "kind": kind, "items": page },
        "pagination": pagination
    })
}

fn recovery_next(query: &LspSearchQuery) -> Value {
    let path = query
        .uri
        .as_deref()
        .map(uri_to_path)
        .or_else(|| query.workspace_root.clone())
        .unwrap_or_default();
    let symbol = query.symbol_name.clone().unwrap_or_default();
    json!({
        "searchText": {
            "tool": "localSearch",
            "query": { "path": path, "searchText": symbol },
            "confidence": "medium"
        },
        "syntax": {
            "tool": "astSearch",
            "query": { "operation": "match", "path": path, "pattern": symbol },
            "confidence": "medium"
        }
    })
}

fn attach_recovery_next(value: &mut Value, query: &LspSearchQuery) {
    value["next"] = recovery_next(query);
}

fn failure(query: &LspSearchQuery, code: &str, message: &str, server_available: bool) -> Value {
    let mut value = json!({
        "status": "error",
        "errorCode": code,
        "error": message,
        "type": query.operation,
        "uri": query.uri,
        "lsp": { "serverAvailable": server_available, "source": "native" },
        "hints": [
            "Use localSearch for text or astSearch operation:\"match\" for syntax, then localFetch for surrounding code."
        ]
    });
    attach_recovery_next(&mut value, query);
    value
}

fn empty(query: &LspSearchQuery, category: &str, reason: &str, server_available: bool) -> Value {
    json!({
        "status": "empty",
        "type": query.operation,
        "uri": query.uri,
        "lsp": { "serverAvailable": server_available, "source": "native" },
        "payload": { "kind": "empty", "category": category, "reason": reason },
        "hints": [
            "Use localSearch for text or astSearch operation:\"match\" for syntax, then localFetch for surrounding code."
        ]
    })
}

fn paginate(items: &[Value], page: u32, page_size: u32) -> (Vec<Value>, Value) {
    let page_size = page_size.max(1);
    let total = items.len() as u32;
    let total_pages = total.div_ceil(page_size).max(1);
    let current = page.clamp(1, total_pages);
    let start = ((current - 1) * page_size) as usize;
    let page_items = items
        .iter()
        .skip(start)
        .take(page_size as usize)
        .cloned()
        .collect::<Vec<_>>();
    let has_more = current < total_pages;
    (
        page_items,
        json!({
            "currentPage": current,
            "totalPages": total_pages,
            "totalResults": total,
            "hasMore": has_more,
            "pageSize": page_size,
            "nextPage": has_more.then_some(current + 1)
        }),
    )
}

fn as_array(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values.clone(),
        Value::Null => vec![],
        other => vec![other.clone()],
    }
}

fn uri_to_path(uri: &str) -> String {
    uri.strip_prefix("file://")
        .map(percent_decode)
        .unwrap_or_else(|| uri.to_owned())
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            )
        {
            out.push(byte);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_and_unavailable_rows_expose_status_and_recovery_next() {
        let query = super::LspSearchQuery {
            operation: "definition".into(),
            uri: Some("/repo/src/lib.rs".into()),
            workspace_root: None,
            symbol_name: Some("execute".into()),
            line: None,
            character: None,
            line_hint: Some(10),
            order_hint: None,
            include_declaration: None,
            group_by_file: None,
            format: None,
            depth: None,
            page: None,
            page_size: None,
            rust_context: None,
        };
        let empty = super::with_next(&query, super::empty(&query, "noLocations", "none", true));
        assert_eq!(empty["status"], "empty");
        assert_eq!(empty["next"]["searchText"]["confidence"], "medium");
        let down = super::failure(&query, "lsp.serverUnavailable", "missing", false);
        assert_eq!(down["status"], "error");
        assert_eq!(down["errorCode"], "lsp.serverUnavailable");
        assert_eq!(down["next"]["syntax"]["tool"], "astSearch");
    }
}
