//! Management uses generated field metadata and never returns stored secret values.
use super::resolver::{parse_candidate, workspace_file_allowed, workspace_setting_allowed};
use super::validation::{get_path, validate_field};
use super::{
    CONFIG_FIELDS, ConfigFieldKind, ConfigFieldSpec, ConfigOutput, SettingSource, config_revision,
    edit_config_json, get_config_value, parse_config_json, parse_env, read_private_config,
    removed_reason, replace_private_config,
};
use serde_json::{Value, json};
use std::path::Path;
fn field_value(field: &ConfigFieldSpec, value: &Value) -> Result<(), String> {
    let mut errors = Vec::new();
    validate_field(field, Some(value), &mut errors);
    if !errors.is_empty() {
        return Err(format!(
            "Invalid value for {}; check its type and allowed values.",
            field.path
        ));
    }
    Ok(())
}
/// Why `name` cannot be saved: the contract removed it. Removing it stays
/// allowed, so a stale line can be deleted.
pub(super) fn reject_removed(name: &str) -> Result<(), String> {
    removed_reason(name).map_or(Ok(()), |reason| {
        Err(format!("{name} was removed and cannot be set: {reason}"))
    })
}
/// Whether `value` is one a setting's variable may be saved with: one the
/// resolver accepts. Undeclared keys are free-form.
pub(crate) fn validate_env_value(key: &str, value: &str) -> Result<(), String> {
    let Some((field, binding)) = CONFIG_FIELDS.iter().find_map(|field| {
        field
            .env
            .filter(|binding| binding.name == key)
            .map(|binding| (field, binding))
    }) else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Ok(());
    }
    if parse_candidate(
        field,
        &Value::String(value.to_owned()),
        true,
        binding.normalize,
    )
    .is_none()
    {
        return Err(format!(
            "Invalid value for {key}; check its type and allowed values."
        ));
    }
    Ok(())
}
/// Set or remove one `.octocoderc` setting under the inspected `revision`.
/// Errors are `InvalidInput` for a rejected key or value, `WouldBlock` when
/// the file changed or is locked since it was inspected, and `Other` when it
/// cannot be read or written safely.
pub fn edit_setting(
    path: &Path,
    key: &str,
    value: Option<&Value>,
    revision: &str,
    workspace: bool,
) -> std::io::Result<bool> {
    let invalid = |message: String| std::io::Error::new(std::io::ErrorKind::InvalidInput, message);
    let field = CONFIG_FIELDS.iter().find(|f| f.file && f.path == key);
    if field.is_none() && removed_reason(key).is_none() {
        return Err(invalid("Unknown configuration setting.".into()));
    }
    if let Some(value) = value {
        reject_removed(key).map_err(invalid)?;
        let field = field.ok_or_else(|| invalid("Unknown configuration setting.".into()))?;
        field_value(field, value).map_err(invalid)?;
        if workspace && !workspace_setting_allowed(field, value) {
            return Err(invalid(
                "This setting may only be configured in home scope.".into(),
            ));
        }
    }
    let original = read_private_config(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::InvalidInput {
            error
        } else {
            std::io::Error::other("Cannot safely read configuration.")
        }
    })?;
    let parts = key.split('.').collect::<Vec<_>>();
    let mut body =
        edit_config_json(original.as_deref().unwrap_or("{}"), &parts, value).map_err(invalid)?;
    if value.is_none() {
        // A section the removal leaves empty goes with it.
        for depth in (1..parts.len()).rev() {
            let parsed = parse_config_json(&body).map_err(invalid)?;
            if !get_path(&parsed, &parts[..depth].join("."))
                .and_then(Value::as_object)
                .is_some_and(serde_json::Map::is_empty)
            {
                break;
            }
            body = edit_config_json(&body, &parts[..depth], None).map_err(invalid)?;
        }
    }
    replace_private_config(path, revision, &body, false)
        .map(|(changed, _)| changed)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::InvalidInput => e,
            _ => std::io::Error::other("Cannot safely write configuration."),
        })
}
/// A URL setting without the parts that carry credentials (userinfo, query
/// values, fragment), so a token embedded in it is never shown. Query keys
/// stay: they name what is set without revealing it.
fn shown(field: &ConfigFieldSpec, value: Option<Value>) -> Option<Value> {
    if field.kind != ConfigFieldKind::Url {
        return value;
    }
    let Some(mut url) = value
        .as_ref()
        .and_then(Value::as_str)
        .and_then(|v| url::Url::parse(v).ok())
    else {
        return value;
    };
    let sensitive = !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some_and(|query| !query.is_empty())
        || url.fragment().is_some();
    if !sensitive {
        return value;
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let keys: Vec<String> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
    if keys.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut()
            .clear()
            .extend_pairs(keys.iter().map(|key| (key.as_str(), "[REDACTED]")));
    }
    url.set_fragment(None);
    Some(Value::String(url.into()))
}
/// Each file degrades on its own: one that cannot be read safely (a symlink,
/// a device, too large) is reported read-only and never read through.
pub fn inspect_management(config: &ConfigOutput, workspace: &Path) -> Value {
    let paths = [
        ("homeEnv", config.home.join(".env")),
        ("workspaceEnv", workspace.join(".env")),
        ("homeSettings", config.home.join(".octocoderc")),
        ("workspaceSettings", workspace.join(".octocoderc")),
    ];
    let mut files = serde_json::Map::new();
    let mut envs = vec![];
    let mut saved = vec![];
    let mut warnings = config.diagnostics.iter().map(|diagnostic|json!({
        "code":diagnostic.code,"severity":diagnostic.severity,"key":diagnostic.field_path,"source":diagnostic.source_path,
        "message":"Configuration resolver reported an invalid, unsupported, or protected value; values are omitted."
    })).collect::<Vec<_>>();
    for (name, path) in &paths {
        let text = match read_private_config(path) {
            Ok(text) => text,
            Err(_) => {
                let reason =
                    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
                        "symlink"
                    } else {
                        "unreadable"
                    };
                warnings.push(json!({"file":name,"message":"File cannot be read safely; it is shown read-only."}));
                files.insert(
                    (*name).into(),
                    json!({"path":path,"exists":true,"writable":false,"reason":reason}),
                );
                if name.ends_with("Env") {
                    envs.push(Default::default());
                } else {
                    saved.push(None);
                }
                continue;
            }
        };
        let mut status = json!({"path":path,"revision":config_revision(text.as_deref()),"exists":text.is_some(),"writable":true});
        if name.ends_with("Env") {
            envs.push(parse_env(text.as_deref()));
        } else {
            let parsed = match text.as_deref() {
                None => Some(json!({})),
                Some(t) => match parse_config_json(t) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        status["valid"] = json!(false);
                        status["writable"] = json!(false);
                        warnings.push(json!({"file":name,"message":"Invalid or ambiguous settings file; repair it before editing."}));
                        None
                    }
                },
            };
            saved.push(parsed);
        }
        files.insert((*name).into(), status);
    }
    let mut keys = vec![];
    for (index, env) in envs.iter().enumerate() {
        for (key, value) in env {
            keys.push(json!({"key":key,"set":!value.trim().is_empty(),"scope":if index==0{"home"}else{"workspace"},"source":paths[index].1,"secret":true}));
        }
    }
    // Include credential presence saved in settings without disclosing its value.
    for (index, file) in saved.iter().enumerate() {
        if let Some(file) = file {
            for field in CONFIG_FIELDS.iter().filter(|f| f.file && f.credential) {
                if let Some(value) = get_path(file, field.path) {
                    keys.push(json!({"key":field.env_name().unwrap_or(field.path),"setting":field.path,"set":value.as_str().is_some_and(|v|!v.trim().is_empty()),"scope":if index==0{"home"}else{"workspace"},"source":paths[index+2].1,"secret":true}));
                }
            }
        }
    }
    let settings = CONFIG_FIELDS.iter().filter(|f| f.file && !f.credential).map(|field| {
        let home_saved = saved[0].as_ref().and_then(|v| get_path(v, field.path));
        let workspace_saved = saved[1].as_ref().and_then(|v| get_path(v, field.path));
        let home = home_saved.filter(|v| field_value(field, v).is_ok());
        let project = workspace_saved.filter(|v| field_value(field, v).is_ok());
        for (name, value) in [("homeSettings", home_saved), ("workspaceSettings", workspace_saved)] {
            if value.is_some_and(|v| field_value(field,v).is_err()) {
                warnings.push(json!({"file":name,"key":field.path,"message":"Invalid saved setting; its value is omitted."}));
            }
        }
        let source = match config.setting_source(field.path) {
            SettingSource::Process => "process",
            SettingSource::HomeEnv => "homeEnv",
            SettingSource::WorkspaceEnv => "workspaceEnv",
            SettingSource::WorkspaceFile => "workspaceSettings",
            SettingSource::HomeFile => "homeSettings",
            SettingSource::Default => "default",
        };
        let env_key = matches!(source, "process" | "homeEnv" | "workspaceEnv").then(|| field.env_name()).flatten();
        let workspace_allowed=workspace_file_allowed(field);
        let workspace_narrow_values=field.values.iter().filter(|value| !workspace_allowed && workspace_setting_allowed(field,&json!(value))).copied().collect::<Vec<_>>();
        json!({"key":field.path,"value":shown(field,get_config_value(&config.resolved,field.path)),"homeValue":shown(field,home.cloned()),"workspaceValue":shown(field,project.cloned()),"homeSet":home_saved.is_some(),"workspaceSet":workspace_saved.is_some(),"source":source,"envKey":env_key,"workspaceAllowed":workspace_allowed,"workspaceNarrowValues":workspace_narrow_values})
    }).collect::<Vec<_>>();
    let credential_settings = CONFIG_FIELDS
        .iter()
        .filter(|f| f.file && f.credential)
        .map(|f| f.path)
        .collect::<Vec<_>>()
        .join(", ");
    json!({"settings":settings,"keys":keys,"files":files,"warnings":warnings,"storage":{"kind":"dotenv","encrypted":false,"message":format!("Environment keys are saved in .env files and credential settings ({credential_settings}) in .octocoderc, unencrypted with owner-only permissions; the GitHub login store is separate.")}})
}
#[cfg(test)]
mod tests {
    use super::*;
    /// A URL setting shown by `config` hides every credential it embeds:
    /// userinfo, query values and the fragment (tokens often ride there).
    #[test]
    fn shown_url_hides_query_and_fragment_credentials() {
        let field = CONFIG_FIELDS
            .iter()
            .find(|field| field.kind == ConfigFieldKind::Url)
            .expect("a URL setting");
        let secret = "s3cr3tvalue";
        let shown_value = shown(
            field,
            Some(json!(format!(
                "https://user:{secret}@api.example/v1?access_token={secret}&region=eu#key={secret}"
            ))),
        )
        .expect("value");
        let text = shown_value.as_str().expect("string");
        assert!(!text.contains(secret), "{text}");
        assert!(text.starts_with("https://api.example/v1?"), "{text}");
        assert!(text.contains("access_token="), "{text}");
        let plain = json!("https://api.example/v1");
        assert_eq!(shown(field, Some(plain.clone())), Some(plain));
    }
    #[test]
    fn strict_environment_types() {
        assert!(validate_env_value("OCTOCODE_LSP_PREWARM", "sometimes").is_err());
        assert!(validate_env_value("OCTOCODE_LSP_PREWARM", "OFF").is_ok());
        assert!(validate_env_value("OCTOCODE_ENABLE_LOCAL", "perhaps").is_err());
        assert!(validate_env_value("CUSTOM_PROVIDER_KEY", "anything").is_ok());
    }
    /// A removed name cannot be saved, in either file, and names its reason;
    /// removing it stays allowed so a stale line can be deleted.
    #[test]
    fn removed_names_cannot_be_saved_but_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let env = root.join(".env");
        let error =
            super::super::edit_scoped_env(&env, "REQUEST_TIMEOUT", Some("5000"), false, None)
                .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("removed"), "{error}");
        assert!(!env.exists());
        let rc = root.join(".octocoderc");
        let text =
            r#"{"network":{"timeout":5000},"output":{"pagination":{"defaultCharLength":4000}}}"#;
        std::fs::write(&rc, text).unwrap();
        for key in ["network.timeout", "output.pagination.defaultCharLength"] {
            let error = edit_setting(
                &rc,
                key,
                Some(&json!(6000)),
                &config_revision(Some(text)),
                false,
            )
            .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{key}");
            assert!(error.to_string().contains("removed"), "{key}: {error}");
        }
        for key in ["network.timeout", "output.pagination"] {
            let current = std::fs::read_to_string(&rc).unwrap();
            assert!(edit_setting(&rc, key, None, &config_revision(Some(&current)), false).unwrap());
        }
        assert_eq!(
            parse_config_json(&std::fs::read_to_string(&rc).unwrap()).unwrap(),
            json!({})
        );
    }
    #[test]
    fn inspect_never_discloses_unknown_or_invalid_values() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(cwd.join(".octocode")).unwrap();
        std::fs::write(
            home.join(".env"),
            "CUSTOM_KEY=private-marker\nOCTOCODE_LSP_PREWARM=off\n",
        )
        .unwrap();
        std::fs::write(home.join(".octocoderc"),r#"{"classification":{"api":"credential-marker"},"lsp":{"prewarm":{"hidden":"invalid-marker"}}}"#).unwrap();
        let input = super::super::acquire_config_input(
            std::collections::BTreeMap::from([(
                "OCTOCODE_HOME".into(),
                home.to_string_lossy().into_owned(),
            )]),
            cwd.clone(),
            root,
            super::super::RuntimeSurface::Cli,
        );
        let config = super::super::resolve_config(&input);
        let output = inspect_management(&config, &cwd.join(".octocode"));
        let serialized = serde_json::to_string(&output).unwrap();
        for marker in ["private-marker", "credential-marker", "invalid-marker"] {
            assert!(!serialized.contains(marker));
        }
        assert!(
            output["keys"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["key"] == "CUSTOM_KEY" && v["scope"] == "home")
        );
    }
    #[test]
    fn inspection_source_tracks_skipped_environment_and_process_override() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(cwd.join(".octocode")).unwrap();
        std::fs::write(home.join(".env"), "OCTOCODE_ENABLE_LOCAL=true\n").unwrap();
        std::fs::write(
            home.join(".octocoderc"),
            r#"{"local":{"enabled":false},"lsp":{"prewarm":"off"}}"#,
        )
        .unwrap();
        for (raw, source) in [("nope", "homeSettings"), ("true", "process")] {
            let input = super::super::acquire_config_input(
                std::collections::BTreeMap::from([
                    ("OCTOCODE_HOME".into(), home.to_string_lossy().into_owned()),
                    ("OCTOCODE_ENABLE_LOCAL".into(), raw.into()),
                ]),
                cwd.clone(),
                root.clone(),
                super::super::RuntimeSurface::Cli,
            );
            let config = super::super::resolve_config(&input);
            let output = inspect_management(&config, &cwd.join(".octocode"));
            let settings = output["settings"].as_array().unwrap();
            assert_eq!(
                settings
                    .iter()
                    .find(|s| s["key"] == "local.enabled")
                    .unwrap()["source"],
                source
            );
            assert_eq!(
                settings.iter().find(|s| s["key"] == "lsp.prewarm").unwrap()["source"],
                "homeSettings"
            );
        }
    }
    fn inspect_home(home_env: Option<&str>, home_rc: &str) -> (tempfile::TempDir, Value) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let home = root.join("home");
        let cwd = root.join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(cwd.join(".octocode")).unwrap();
        std::fs::write(home.join(".octocoderc"), home_rc).unwrap();
        if let Some(text) = home_env {
            let target = root.join("elsewhere.env");
            std::fs::write(&target, text).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, home.join(".env")).unwrap();
        }
        let input = super::super::acquire_config_input(
            std::collections::BTreeMap::from([(
                "OCTOCODE_HOME".into(),
                home.to_string_lossy().into_owned(),
            )]),
            cwd.clone(),
            root,
            super::super::RuntimeSurface::Cli,
        );
        let output = inspect_management(
            &super::super::resolve_config(&input),
            &cwd.join(".octocode"),
        );
        (dir, output)
    }
    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_is_read_only_and_never_read_through() {
        let (_dir, output) = inspect_home(Some("LINKED_SECRET_KEY=linked-marker\n"), "{}");
        assert_eq!(output["files"]["homeEnv"]["writable"], false);
        assert_eq!(output["files"]["homeEnv"]["reason"], "symlink");
        assert!(output["files"]["homeEnv"].get("revision").is_none());
        assert_eq!(output["files"]["homeSettings"]["writable"], true);
        assert!(!output.to_string().contains("LINKED_SECRET_KEY"));
    }
    #[test]
    fn url_settings_never_show_embedded_credentials() {
        let (_dir, output) = inspect_home(
            None,
            r#"{"github":{"apiUrl":"https://user:url-secret@ghe.example/api/v3"}}"#,
        );
        assert!(!output.to_string().contains("url-secret"));
        let api = output["settings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == "github.apiUrl")
            .unwrap();
        assert_eq!(api["homeValue"], "https://ghe.example/api/v3");
    }
    #[test]
    fn storage_message_names_where_credential_settings_live() {
        let (_dir, output) = inspect_home(None, "{}");
        let message = output["storage"]["message"].as_str().unwrap();
        assert!(message.contains(".octocoderc") && message.contains("classification.api"));
    }
    #[test]
    fn removing_the_last_key_prunes_its_empty_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(dir.path())
            .unwrap()
            .join(".octocoderc");
        let text =
            r#"{"mcp":{"deferred":["astSearch"]},"lsp":{"prewarm":"off","autoInstall":"auto"}}"#;
        std::fs::write(&path, text).unwrap();
        edit_setting(
            &path,
            "mcp.deferred",
            None,
            &config_revision(Some(text)),
            false,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        edit_setting(
            &path,
            "lsp.prewarm",
            None,
            &config_revision(Some(&text)),
            false,
        )
        .unwrap();
        let saved = parse_config_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved, json!({"lsp":{"autoInstall":"auto"}}));
    }
    #[test]
    fn stale_revision_cannot_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(dir.path())
            .unwrap()
            .join(".octocoderc");
        std::fs::write(&path, "{/*keep*/\"lsp\":{\"prewarm\":\"off\"}}").unwrap();
        assert!(edit_setting(&path, "lsp.prewarm", Some(&json!("all")), "missing", false).is_err());
        let revision = config_revision(read_private_config(&path).unwrap().as_deref());
        assert!(edit_setting(&path, "lsp.prewarm", Some(&json!("all")), &revision, false).unwrap());
        assert!(std::fs::read_to_string(path).unwrap().contains("/*keep*/"));
    }
}
