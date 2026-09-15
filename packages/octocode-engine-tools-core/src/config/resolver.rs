use super::dotenv::{
    apply_env, merged_env, parse_boolean_env, parse_int_env, parse_string_array_env,
};
use super::loader::load_config;
use super::types::*;
use super::validation::validate_config;
use serde_json::{Value, json};
use std::collections::BTreeMap;
const SOURCE_KEYS: [&str; 17] = [
    "GITHUB_API_URL",
    "OCTOCODE_GITHUB_GRAPHQL",
    "ENABLE_LOCAL",
    "ENABLE_CLONE",
    "ENABLE_AST_REWRITE_APPLY",
    "ALLOWED_PATHS",
    "WORKSPACE_ROOT",
    "TOOLS_TO_RUN",
    "DISABLE_TOOLS",
    "REQUEST_TIMEOUT",
    "MAX_RETRIES",
    "OCTOCODE_LSP_CONFIG",
    "OCTOCODE_OUTPUT_FORMAT",
    "OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH",
    "OCTOCODE_ENABLE_STATS",
    "OCTOCODE_STORAGE_MODE",
    "OCTOCODE_EXTENSION_STORAGE_MODE",
];
fn object<'a>(root: Option<&'a Value>, key: &str) -> Option<&'a serde_json::Map<String, Value>> {
    root?.get(key)?.as_object()
}
fn str_field(o: Option<&serde_json::Map<String, Value>>, k: &str) -> Option<String> {
    o?.get(k)?.as_str().map(str::to_owned)
}
fn bool_field(o: Option<&serde_json::Map<String, Value>>, k: &str) -> Option<bool> {
    o?.get(k)?.as_bool()
}
fn num_field(o: Option<&serde_json::Map<String, Value>>, k: &str) -> Option<f64> {
    o?.get(k)?.as_f64()
}
fn arr_field(o: Option<&serde_json::Map<String, Value>>, k: &str) -> Option<Vec<String>> {
    o?.get(k)?.as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    })
}
fn env<'a>(e: &'a BTreeMap<String, String>, k: &str) -> Option<&'a str> {
    e.get(k).map(String::as_str)
}
fn clamp(n: f64, min: f64, max: f64) -> f64 {
    n.max(min).min(max)
}
pub fn resolve_sections(file: Option<&Value>, e: &BTreeMap<String, String>) -> ResolvedConfig {
    let github = object(file, "github");
    let local = object(file, "local");
    let tools = object(file, "tools");
    let network = object(file, "network");
    let lsp = object(file, "lsp");
    let output = object(file, "output");
    let pagination = output.and_then(|o| o.get("pagination")?.as_object());
    let storage = object(file, "storage");
    let extension = object(file, "extension");
    let ext_storage = extension.and_then(|o| o.get("storage")?.as_object());
    let storage_mode = match env(e, "OCTOCODE_STORAGE_MODE").map(|s| s.trim().to_ascii_lowercase())
    {
        Some(x) if x == "memory" || x == "persistent" => x,
        _ => str_field(storage, "mode").unwrap_or_else(|| "persistent".into()),
    };
    let extension_mode =
        match env(e, "OCTOCODE_EXTENSION_STORAGE_MODE").map(|s| s.trim().to_ascii_lowercase()) {
            Some(x) if x == "memory" || x == "persistent" => x,
            _ => str_field(ext_storage, "mode").unwrap_or_else(|| storage_mode.clone()),
        };
    ResolvedConfig {
        version: file
            .and_then(|v| v.get("version"))
            .cloned()
            .unwrap_or(json!(1)),
        github: GitHubConfig {
            api_url: env(e, "GITHUB_API_URL")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| str_field(github, "apiUrl"))
                .unwrap_or_else(|| "https://api.github.com".into()),
            graphql_enabled: parse_boolean_env(env(e, "OCTOCODE_GITHUB_GRAPHQL"))
                .or_else(|| bool_field(github, "graphqlEnabled"))
                .unwrap_or(true),
        },
        local: LocalConfig {
            enabled: parse_boolean_env(env(e, "ENABLE_LOCAL"))
                .or_else(|| bool_field(local, "enabled"))
                .unwrap_or(true),
            enable_clone: parse_boolean_env(env(e, "ENABLE_CLONE"))
                .or_else(|| bool_field(local, "enableClone"))
                .unwrap_or(false),
            enable_ast_rewrite_apply: parse_boolean_env(env(e, "ENABLE_AST_REWRITE_APPLY"))
                .or_else(|| bool_field(local, "enableAstRewriteApply"))
                .unwrap_or(false),
            allowed_paths: parse_string_array_env(env(e, "ALLOWED_PATHS"))
                .or_else(|| arr_field(local, "allowedPaths"))
                .unwrap_or_default(),
            workspace_root: env(e, "WORKSPACE_ROOT")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| str_field(local, "workspaceRoot")),
        },
        tools: ToolsConfig {
            enabled: parse_string_array_env(env(e, "TOOLS_TO_RUN"))
                .or_else(|| arr_field(tools, "enabled")),
            disabled: parse_string_array_env(env(e, "DISABLE_TOOLS"))
                .or_else(|| arr_field(tools, "disabled")),
        },
        network: NetworkConfig {
            timeout: clamp(
                parse_int_env(env(e, "REQUEST_TIMEOUT"))
                    .map(|x| x as f64)
                    .or_else(|| num_field(network, "timeout"))
                    .unwrap_or(30000.),
                5000.,
                300000.,
            ),
            max_retries: clamp(
                parse_int_env(env(e, "MAX_RETRIES"))
                    .map(|x| x as f64)
                    .or_else(|| num_field(network, "maxRetries"))
                    .unwrap_or(3.),
                0.,
                10.,
            ),
        },
        lsp: LspConfig {
            config_path: env(e, "OCTOCODE_LSP_CONFIG")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| str_field(lsp, "configPath")),
        },
        output: OutputConfig {
            format: {
                let x = env(e, "OCTOCODE_OUTPUT_FORMAT")
                    .map(|s| s.trim().to_ascii_lowercase())
                    .or_else(|| str_field(output, "format"))
                    .unwrap_or_else(|| "yaml".into());
                if x == "json" || x == "yaml" {
                    x
                } else {
                    "yaml".into()
                }
            },
            pagination: PaginationConfig {
                default_char_length: clamp(
                    parse_int_env(env(e, "OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH"))
                        .map(|x| x as f64)
                        .or_else(|| num_field(pagination, "defaultCharLength"))
                        .unwrap_or(20000.),
                    1000.,
                    50000.,
                ),
            },
        },
        session: SessionConfig {
            enable_stats: parse_boolean_env(env(e, "OCTOCODE_ENABLE_STATS")).unwrap_or(false),
        },
        storage: StorageConfig { mode: storage_mode },
        extension: ExtensionConfig {
            storage: StorageConfig {
                mode: extension_mode,
            },
        },
    }
}
pub fn resolve_env_token(e: &BTreeMap<String, String>) -> Option<PrivateTokenSelection> {
    for k in ENV_TOKEN_VARS {
        if let Some(v) = e.get(k).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            return Some(PrivateTokenSelection::new(v.into(), format!("env:{k}")));
        }
    }
    None
}
pub fn resolve_config(input: &ConfigInput) -> ConfigOutput {
    let gt = match &input.global_env {
        FileInput::Read { text, .. } => Some(text.as_str()),
        _ => None,
    };
    let pt = match &input.project_env {
        FileInput::Read { text, .. } => Some(text.as_str()),
        _ => None,
    };
    let (map, sources) = merged_env(gt, pt, input.trusted_project);
    let mut effective = input.env.clone();
    let dotenv = apply_env(&map, sources, &mut effective);
    let load = load_config(&input.config_file);
    let mut diagnostics = vec![];
    let (file, state) = if load.success {
        let value = load.config.unwrap_or_else(|| json!({}));
        let v = validate_config(&value);
        for m in &v.warnings {
            diagnostics.push(ConfigDiagnostic {
                severity: Severity::Warning,
                code: "unknown_or_future_config".into(),
                field_path: None,
                message: m.clone(),
                source_path: Some(load.path.clone()),
            })
        }
        if v.valid {
            (Some(value), "valid")
        } else {
            for m in v.errors {
                diagnostics.push(ConfigDiagnostic {
                    severity: Severity::Error,
                    code: "invalid_config".into(),
                    field_path: None,
                    message: m,
                    source_path: Some(load.path.clone()),
                })
            }
            (None, "invalid")
        }
    } else if load.error.as_deref() == Some("Config file does not exist") {
        (None, "absent")
    } else {
        diagnostics.push(ConfigDiagnostic {
            severity: Severity::Error,
            code: "config_load_error".into(),
            field_path: None,
            message: load.error.unwrap_or_else(|| "Invalid configuration".into()),
            source_path: Some(load.path.clone()),
        });
        (None, "invalid")
    };
    let has_env = SOURCE_KEYS.iter().any(|k| effective.contains_key(*k));
    let source = match state {
        "invalid" => ConfigSource::Invalid,
        "valid" if has_env => ConfigSource::Mixed,
        "valid" => ConfigSource::File,
        "absent" if has_env => ConfigSource::Env,
        _ => ConfigSource::Defaults,
    };
    let resolved = resolve_sections(file.as_ref(), &effective);
    let token = resolve_env_token(&effective);
    let config_path = (state != "absent").then(|| load.path.clone());
    let child_env = ChildEnvPlan {
        set: effective.clone(),
    };
    ConfigOutput {
        resolved,
        effective_env: effective,
        dotenv,
        diagnostics,
        token,
        child_env,
        source,
        config_path,
        revision: input.revision,
    }
}
pub fn get_config_value(resolved: &ResolvedConfig, path: &str) -> Option<Value> {
    let value = serde_json::to_value(resolved).ok()?;
    if path.is_empty() {
        return None;
    }
    let mut cur = &value;
    for p in path.split('.') {
        cur = cur.as_object()?.get(p)?;
    }
    Some(cur.clone())
}
pub fn is_stats_enabled(resolved: &ResolvedConfig) -> bool {
    resolved.storage.mode == "persistent" && resolved.session.enable_stats
}
pub fn is_persistent_storage_enabled(resolved: &ResolvedConfig) -> bool {
    resolved.storage.mode == "persistent"
}
pub fn is_persistent_storage_enabled_for_extension(resolved: &ResolvedConfig) -> bool {
    resolved.extension.storage.mode == "persistent"
}
pub fn inspector_data(input: &ConfigInput, output: &ConfigOutput) -> ConfigInspectorData {
    let home = input
        .config_file
        .path()
        .parent()
        .unwrap_or(&input.os_home)
        .to_path_buf();
    let load = load_config(&input.config_file);
    let mut config_keys: Vec<String> = load
        .config
        .and_then(|v| v.as_object().cloned())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    config_keys.sort();
    ConfigInspectorData {
        home,
        global_env_path: input.global_env.path().clone(),
        project_env_path: input.project_env.path().clone(),
        loaded_keys: output.dotenv.keys.clone(),
        global_key_count: output
            .dotenv
            .sources
            .values()
            .filter(|s| s.as_str() == "global")
            .count(),
        project_key_count: output
            .dotenv
            .sources
            .values()
            .filter(|s| s.as_str() == "project")
            .count(),
        storage_mode: output.resolved.storage.mode.clone(),
        config_keys,
        source: output.source.clone(),
        config_path: output.config_path.clone(),
        diagnostics: output.diagnostics.clone(),
        revision: output.revision,
    }
}
