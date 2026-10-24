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
#[test]
fn unknown_settings_cannot_mutate_the_configuration_file() {
    let dir = tempfile::tempdir().unwrap();
    let rc = std::fs::canonicalize(dir.path())
        .unwrap()
        .join(".octocoderc");
    let text = r#"{"lsp":{"prewarm":"off"},"unrecognized":"private-marker"}"#;
    std::fs::write(&rc, text).unwrap();
    for value in [Some(json!("replacement")), None] {
        let error = edit_setting(
            &rc,
            "unrecognized",
            value.as_ref(),
            &config_revision(Some(text)),
            false,
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(!error.to_string().contains("private-marker"));
        assert_eq!(std::fs::read_to_string(&rc).unwrap(), text);
    }
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
    let text = r#"{"local":{"enabled":true},"lsp":{"prewarm":"off","autoInstall":"auto"}}"#;
    std::fs::write(&path, text).unwrap();
    edit_setting(
        &path,
        "local.enabled",
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
