use super::types::{CONFIG_SCHEMA_VERSION, ValidationResult};
use serde_json::Value;
use std::path::Path;

fn object<'a>(
    v: Option<&'a Value>,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<&'a serde_json::Map<String, Value>> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::Object(o)) => Some(o),
        Some(_) => {
            errors.push(format!("{name}: Must be an object"));
            None
        }
    }
}
fn number(v: Option<&Value>, field: &str, min: f64, max: f64, errors: &mut Vec<String>) {
    if let Some(v) = v.filter(|v| !v.is_null()) {
        match v.as_f64() {
            None => errors.push(format!("{field}: Must be a number")),
            Some(n) if n < min || n > max => {
                errors.push(format!("{field}: Must be between {min:.0} and {max:.0}"))
            }
            _ => {}
        }
    }
}
fn boolean(v: Option<&Value>, field: &str, errors: &mut Vec<String>) {
    if v.is_some_and(|v| !v.is_null() && !v.is_boolean()) {
        errors.push(format!("{field}: Must be a boolean"))
    }
}
fn string(v: Option<&Value>, field: &str, errors: &mut Vec<String>) {
    if v.is_some_and(|v| !v.is_null() && !v.is_string()) {
        errors.push(format!("{field}: Must be a string"))
    }
}
fn array(v: Option<&Value>, field: &str, errors: &mut Vec<String>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::Array(a)) => {
            for (i, v) in a.iter().enumerate() {
                if !v.is_string() {
                    errors.push(format!("{field}[{i}]: Must be a string"));
                    return false;
                }
            }
            true
        }
        Some(_) => {
            errors.push(format!("{field}: Must be an array"));
            false
        }
    }
}
fn warn_unknown(
    o: Option<&serde_json::Map<String, Value>>,
    prefix: &str,
    known: &[&str],
    warnings: &mut Vec<String>,
) {
    if let Some(o) = o {
        for k in o.keys() {
            if !known.contains(&k.as_str()) {
                warnings.push(if prefix.is_empty() {
                    format!("Unknown configuration key: {k}")
                } else {
                    format!("Unknown configuration key: {prefix}.{k}")
                })
            }
        }
    }
}
fn valid_path(s: &str) -> bool {
    let abs = s.starts_with('~')
        || Path::new(s).is_absolute()
        || (s.len() > 2
            && s.as_bytes()[1] == b':'
            && (s.as_bytes()[2] == b'/' || s.as_bytes()[2] == b'\\'));
    !s.trim().is_empty() && abs && !s.split(['/', '\\']).any(|x| x == "..")
}

#[allow(clippy::collapsible_if)]
pub fn validate_config(config: &Value) -> ValidationResult {
    let Some(root) = config.as_object() else {
        return ValidationResult {
            valid: false,
            errors: vec!["Configuration must be a JSON object".into()],
            warnings: vec![],
            config: None,
        };
    };
    let mut e = vec![];
    let mut w = vec![];
    if let Some(v) = root.get("version") {
        if !v.is_i64() && !v.is_u64() {
            e.push("version: Must be an integer".into())
        } else if v.as_i64().unwrap_or(i64::MAX) > CONFIG_SCHEMA_VERSION {
            w.push(format!("version: Config version {} is newer than supported version {CONFIG_SCHEMA_VERSION}",v))
        }
    }
    let gh = object(root.get("github"), "github", &mut e);
    if let Some(o) = gh {
        if let Some(v) = o.get("apiUrl").filter(|v| !v.is_null()) {
            if let Some(s) = v.as_str() {
                match url::Url::parse(s) {
                    Ok(u) if matches!(u.scheme(), "http" | "https") => {}
                    Ok(_) => e.push("github.apiUrl: Only http/https URLs allowed".into()),
                    Err(_) => e.push("github.apiUrl: Invalid URL format".into()),
                }
            } else {
                e.push("github.apiUrl: Must be a string".into())
            }
        }
        boolean(o.get("graphqlEnabled"), "github.graphqlEnabled", &mut e);
    }
    warn_unknown(gh, "github", &["apiUrl", "graphqlEnabled"], &mut w);
    let local = object(root.get("local"), "local", &mut e);
    if let Some(o) = local {
        boolean(o.get("enabled"), "local.enabled", &mut e);
        boolean(o.get("enableClone"), "local.enableClone", &mut e);
        boolean(
            o.get("enableAstRewriteApply"),
            "local.enableAstRewriteApply",
            &mut e,
        );
        if array(o.get("allowedPaths"), "local.allowedPaths", &mut e) {
            if let Some(Value::Array(a)) = o.get("allowedPaths") {
                for (i, v) in a.iter().enumerate() {
                    if let Some(s) = v.as_str() {
                        if !valid_path(s) {
                            let msg = if s.trim().is_empty() {
                                format!("local.allowedPaths[{i}]: empty or whitespace-only path")
                            } else if s.split(['/', '\\']).any(|x| x == "..") {
                                format!(
                                    "local.allowedPaths[{i}]: path traversal (..) not allowed (got \"{s}\")"
                                )
                            } else {
                                format!(
                                    "local.allowedPaths[{i}]: must be absolute path or start with ~ (got \"{s}\")"
                                )
                            };
                            e.push(msg)
                        }
                    }
                }
            }
        }
        string(o.get("workspaceRoot"), "local.workspaceRoot", &mut e);
        if let Some(s) = o.get("workspaceRoot").and_then(Value::as_str) {
            if !valid_path(s) {
                let absolute = s.starts_with('~')
                    || Path::new(s).is_absolute()
                    || (s.len() > 2
                        && s.as_bytes()[1] == b':'
                        && matches!(s.as_bytes()[2], b'/' | b'\\'));
                e.push(if s.trim().is_empty() {
                    "local.workspaceRoot: empty or whitespace-only path".into()
                } else if !absolute {
                    format!(
                        "local.workspaceRoot: must be absolute path or start with ~ (got \"{s}\")"
                    )
                } else if s.split(['/', '\\']).any(|x| x == "..") {
                    format!("local.workspaceRoot: path traversal (..) not allowed (got \"{s}\")")
                } else {
                    format!(
                        "local.workspaceRoot: must be absolute path or start with ~ (got \"{s}\")"
                    )
                })
            }
        }
    }
    warn_unknown(
        local,
        "local",
        &[
            "enabled",
            "enableClone",
            "enableAstRewriteApply",
            "allowedPaths",
            "workspaceRoot",
        ],
        &mut w,
    );
    let tools = object(root.get("tools"), "tools", &mut e);
    if let Some(o) = tools {
        array(o.get("enabled"), "tools.enabled", &mut e);
        array(o.get("disabled"), "tools.disabled", &mut e);
    }
    warn_unknown(tools, "tools", &["enabled", "disabled"], &mut w);
    let net = object(root.get("network"), "network", &mut e);
    if let Some(o) = net {
        number(o.get("timeout"), "network.timeout", 5000., 300000., &mut e);
        number(o.get("maxRetries"), "network.maxRetries", 0., 10., &mut e)
    }
    warn_unknown(net, "network", &["timeout", "maxRetries"], &mut w);
    let lsp = object(root.get("lsp"), "lsp", &mut e);
    if let Some(o) = lsp {
        string(o.get("configPath"), "lsp.configPath", &mut e)
    }
    warn_unknown(lsp, "lsp", &["configPath"], &mut w);
    let output = object(root.get("output"), "output", &mut e);
    let pag = output.and_then(|o| object(o.get("pagination"), "output.pagination", &mut e));
    if let Some(o) = output {
        if let Some(v) = o.get("format") {
            match v.as_str() {
                Some("yaml" | "json") | None if v.is_null() => {}
                Some(_) => e.push("output.format: Must be one of: yaml, json".into()),
                None => e.push("output.format: Must be a string".into()),
            }
        }
    }
    if let Some(o) = pag {
        number(
            o.get("defaultCharLength"),
            "output.pagination.defaultCharLength",
            1000.,
            50000.,
            &mut e,
        )
    }
    let storage = object(root.get("storage"), "storage", &mut e);
    validate_storage(storage, "storage", &mut e);
    warn_unknown(storage, "storage", &["mode"], &mut w);
    let ext = object(root.get("extension"), "extension", &mut e);
    let ext_storage = ext.and_then(|o| object(o.get("storage"), "storage", &mut e));
    if let Some(last) = e.last_mut() {
        if last == "storage: Must be an object" {
            *last = "extension.storage: Must be an object".into()
        }
    }
    validate_storage(ext_storage, "extension.storage", &mut e);
    warn_unknown(ext, "extension", &["storage"], &mut w);
    warn_unknown(ext_storage, "extension.storage", &["mode"], &mut w);
    warn_unknown(output, "output", &["format", "pagination"], &mut w);
    warn_unknown(pag, "output.pagination", &["defaultCharLength"], &mut w);
    warn_unknown(
        Some(root),
        "",
        &[
            "$schema",
            "version",
            "github",
            "local",
            "tools",
            "network",
            "lsp",
            "output",
            "storage",
            "extension",
        ],
        &mut w,
    );
    let valid = e.is_empty();
    ValidationResult {
        valid,
        errors: e,
        warnings: w,
        config: valid.then(|| config.clone()),
    }
}
#[allow(clippy::collapsible_if)]
fn validate_storage(o: Option<&serde_json::Map<String, Value>>, prefix: &str, e: &mut Vec<String>) {
    if let Some(v) = o.and_then(|o| o.get("mode")) {
        if !v.is_null() && !matches!(v.as_str(), Some("persistent" | "memory")) {
            e.push(format!(
                "{prefix}.mode: Must be \"persistent\" or \"memory\""
            ))
        }
    }
}
