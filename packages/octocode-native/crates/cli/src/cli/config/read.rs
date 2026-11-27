//! Reading configuration: `config list`, `get` and `check`.
use super::write_json;
use octocode_native::runtime::ToolRuntime;
use octocode_runtime_base::security::scrub_error_text;
use serde_json::json;

/// The setting `key` names, by its path or its variable.
fn setting(key: &str) -> Option<&'static octocode_runtime_base::config::ConfigFieldSpec> {
    octocode_runtime_base::config::CONFIG_FIELDS
        .iter()
        .find(|field| field.path == key || field.env_name() == Some(key))
}

/// A setting's value as one output token: a string raw, anything else as JSON.
fn plain(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// `config list`: every setting, its effective value, source and default.
pub(crate) fn list(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let reports = octocode_runtime_base::config::setting_reports(runtime.config());
    if json_out {
        return write_json(&json!({"settings": reports}));
    }
    let row = |report: &octocode_runtime_base::config::SettingReport| {
        [
            report.setting.or(report.key).unwrap_or_default().to_owned(),
            report.value.as_ref().map_or_else(|| "unset".into(), plain),
            report.source.to_owned(),
            plain(&report.default),
        ]
    };
    let rows: Vec<[String; 4]> = reports.iter().map(row).collect();
    let width = |column: usize| {
        rows.iter()
            .map(|row| row[column].chars().count())
            .max()
            .unwrap_or(0)
    };
    let (key, value, source) = (width(0), width(1), width(2));
    for [k, v, s, d] in &rows {
        println!("{k:<key$}  {v:<value$}  {s:<source$}  default: {d}");
    }
    0
}

/// `config get` for a declared setting: the value that applies, from whichever
/// layer holds it or the default, so a file-set value is never reported unset.
fn write_report(report: &octocode_runtime_base::config::SettingReport, json_out: bool) -> u8 {
    let found = report.value.is_some();
    if json_out {
        let code = write_json(&json!(report));
        if code != 0 {
            return code;
        }
    } else if let Some(value) = &report.value {
        match (report.credential, &report.path) {
            (false, _) => println!("{}", plain(value)),
            (true, Some(path)) => {
                println!("{} ({}: {})", plain(value), report.source, path.display());
            }
            (true, None) => println!("{} (process environment)", plain(value)),
        }
    } else {
        eprintln!(
            "{}: unset (default: {})",
            report.setting.or(report.key).unwrap_or_default(),
            plain(&report.default)
        );
    }
    if found { 0 } else { 1 }
}

/// `config get KEY`: the value on stdout (nothing else, so
/// `$(octocode config get KEY)` captures it); exit 1 when unset. A secret is
/// masked, with the layer and file that hold it, so a person or agent can
/// read it there.
pub(crate) fn get(runtime: &ToolRuntime, key: &str, json_out: bool) -> u8 {
    if let Some(field) = setting(key) {
        let report = octocode_runtime_base::config::setting_report(runtime.config(), field);
        return write_report(&report, json_out);
    }
    if let Some(known) = octocode_runtime_base::config::KNOWN_ENV
        .iter()
        .find(|known| known.name == key)
    {
        let report = octocode_runtime_base::config::known_env_report(runtime.config(), known);
        return write_report(&report, json_out);
    }
    let config = runtime.config();
    let value = config.env_value(key).filter(|value| !value.is_empty());
    let origin = value.and(config.env_origins.get(key));
    let source = value.map(|_| origin.map_or("env", |origin| origin.scope.as_str()));
    let path = origin.map(|origin| &origin.path);
    let masked = octocode_runtime_base::config::is_secret_key(key);
    let shown_key = scrub_error_text(key);
    let shown = value.map(|value| {
        if masked {
            octocode_engine::security::redaction_placeholder(&shown_key)
        } else {
            scrub_error_text(value)
        }
    });
    if json_out {
        let code = write_json(
            &json!({"key": shown_key, "set": value.is_some(), "value": shown, "source": source, "path": path}),
        );
        if code != 0 {
            return code;
        }
    } else if let Some(shown) = shown {
        match (masked, path) {
            (false, _) => println!("{shown}"),
            (true, Some(path)) => {
                println!("{shown} ({}: {})", source.unwrap_or("env"), path.display());
            }
            (true, None) => println!("{shown} (process environment)"),
        }
    } else {
        eprintln!("{shown_key}: unset");
    }
    if value.is_some() { 0 } else { 1 }
}

/// `config check KEY`: exit 0 when set, 1 when unset. A GitHub token key that
/// is unset names the credential GitHub calls use instead.
pub(crate) async fn check(runtime: &ToolRuntime, key: &str, json_out: bool) -> u8 {
    let field = setting(key);
    let key = field
        .and_then(octocode_runtime_base::config::ConfigFieldSpec::env_name)
        .unwrap_or(key);
    let set = field.map_or_else(
        || {
            runtime
                .config()
                .env_value(key)
                .is_some_and(|value| !value.is_empty())
        },
        |field| octocode_runtime_base::config::setting_report(runtime.config(), field).set,
    );
    let github_auth = if !set && octocode_runtime_base::config::ENV_TOKEN_VARS.contains(&key) {
        runtime.active_token_source().await
    } else {
        None
    };
    let shown_key = scrub_error_text(key);
    if json_out {
        let mut value = json!({"key": shown_key, "set": set});
        if let Some(source) = &github_auth {
            value["githubTokenSource"] = json!(source);
        }
        let code = write_json(&value);
        if code != 0 {
            return code;
        }
    } else {
        match &github_auth {
            Some(source) => {
                println!("{shown_key}: unset (GitHub calls use the token from {source})");
            }
            None => println!("{shown_key}: {}", if set { "set" } else { "unset" }),
        }
    }
    if set { 0 } else { 1 }
}
