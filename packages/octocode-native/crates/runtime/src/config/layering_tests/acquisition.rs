use super::*;

// ─── acquisition (real filesystem) ───────────────────────────────────────────

fn acquire(env: &[(&str, String)], cwd: PathBuf, os_home: PathBuf) -> ConfigInput {
    acquire_config_input(
        env.iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        cwd,
        os_home,
        RuntimeSurface::Cli,
    )
}

#[test]
fn acquisition_reads_workspace_file_from_cwd_dot_octocode() {
    let root = tempfile::tempdir().expect("create temp dir");
    let (os_home, cwd) = (root.path().join("user"), root.path().join("repo"));
    fs::create_dir_all(os_home.join(".octocode")).expect("home");
    fs::create_dir_all(cwd.join(".octocode")).expect("workspace");
    fs::write(
        os_home.join(".octocode/.octocoderc"),
        r#"{"tools":{"enabled":["localSearch"],"disabled":["astSearch"]}}"#,
    )
    .expect("global rc");
    fs::write(
        cwd.join(".octocode/.octocoderc"),
        "{\n  // workspace override\n  \"tools\": { \"enabled\": [\"localFetch\"], },\n}\n",
    )
    .expect("workspace rc");
    let i = acquire(&[], cwd.clone(), os_home);
    assert_eq!(
        i.project_config_file.path(),
        &cwd.join(".octocode/.octocoderc")
    );
    let out = resolve_config(&i);
    assert_eq!(
        out.resolved.tools.enabled,
        Some(vec!["localFetch".to_owned()])
    );
    assert_eq!(
        out.resolved.tools.disabled,
        Some(vec!["astSearch".to_owned()])
    );
}

#[test]
fn acquisition_never_reads_the_global_file_twice_when_cwd_is_home() {
    let root = tempfile::tempdir().expect("create temp dir");
    let os_home = root.path().join("user");
    fs::create_dir_all(os_home.join(".octocode")).expect("home");
    fs::write(os_home.join(".octocode/.octocoderc"), "{broken").expect("rc");
    // Default home: cwd = OS home makes <cwd>/.octocode the Octocode home.
    let i = acquire(&[], os_home.clone(), os_home.clone());
    assert!(matches!(i.project_config_file, FileInput::Missing { .. }));
    let out = resolve_config(&i);
    assert_eq!(
        out.diagnostics
            .iter()
            .filter(|d| d.code == "config_load_error")
            .count(),
        1,
        "one broken file must yield one error: {:?}",
        out.diagnostics
    );
    // OCTOCODE_HOME spelled differently but pointing at the same directory.
    let repo = root.path().join("repo");
    fs::create_dir_all(repo.join(".octocode")).expect("workspace");
    let i = acquire(
        &[(
            "OCTOCODE_HOME",
            repo.join("sub/../.octocode").to_string_lossy().into_owned(),
        )],
        repo,
        os_home,
    );
    assert!(matches!(i.project_config_file, FileInput::Missing { .. }));
}

/// `~` and `~/x` expand once, at resolution, against the OS home the path
/// policy uses — from the environment and from a file.
#[test]
fn home_relative_paths_expand_at_resolution() {
    use serde_json::json;
    let env = resolve(Layers {
        global_env: Some(
            "ALLOWED_PATHS=~/allowed,~\nWORKSPACE_ROOT=~/work\nOCTOCODE_LSP_CACHE_DIR=~/lsp\nOCTOCODE_LSP_CONFIG=~/servers.json",
        ),
        ..NONE
    });
    let file = resolve(Layers {
        global_rc: Some(
            r#"{"local":{"allowedPaths":["~/allowed","~"],"workspaceRoot":"~/work"},"lsp":{"cacheDir":"~/lsp","configPath":"~/servers.json"}}"#,
        ),
        ..NONE
    });
    for out in [&env, &file] {
        assert_eq!(
            value_of(out, "local.allowedPaths"),
            json!(["/synthetic/allowed", "/synthetic"])
        );
        assert_eq!(
            value_of(out, "local.workspaceRoot"),
            json!("/synthetic/work")
        );
        assert_eq!(value_of(out, "lsp.cacheDir"), json!("/synthetic/lsp"));
        assert_eq!(
            value_of(out, "lsp.configPath"),
            json!("/synthetic/servers.json")
        );
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    }
}

/// `lsp.configPath` follows the path rules: a relative value is reported and
/// ignored instead of resolving against whatever the process cwd is.
#[test]
fn a_relative_lsp_config_path_is_rejected_with_a_diagnostic() {
    let env = resolve(Layers {
        global_env: Some("OCTOCODE_LSP_CONFIG=servers.json"),
        ..NONE
    });
    assert_eq!(value_of(&env, "lsp.configPath"), serde_json::Value::Null);
    assert!(has_diagnostic(&env, "invalid_env_value", "lsp.configPath"));
    let file = resolve(Layers {
        global_rc: Some(r#"{"lsp":{"configPath":"servers.json"}}"#),
        ..NONE
    });
    assert_eq!(value_of(&file, "lsp.configPath"), serde_json::Value::Null);
    assert!(has_diagnostic(&file, "invalid_config", "lsp.configPath"));
}

/// When cwd is the OS home, `<cwd>/.octocode` is the Octocode home: its
/// `.env` is the home layer only, never re-read as a workspace layer.
#[test]
fn acquisition_never_reads_the_home_dotenv_as_a_workspace_layer() {
    let root = tempfile::tempdir().expect("create temp dir");
    let os_home = root.path().join("user");
    fs::create_dir_all(os_home.join(".octocode")).expect("home");
    fs::write(
        os_home.join(".octocode/.env"),
        "GH_TOKEN=synthetic-home-token\nOCTOCODE_LSP_CACHE_DIR=/home-lsp\n",
    )
    .expect("env");
    let i = acquire(&[], os_home.clone(), os_home);
    assert!(matches!(i.project_env, FileInput::Missing { .. }));
    let out = resolve_config(&i);
    assert_eq!(
        out.dotenv.sources.get("GH_TOKEN").map(String::as_str),
        Some("global")
    );
    assert_eq!(out.env_value("OCTOCODE_LSP_CACHE_DIR"), Some("/home-lsp"));
    assert!(out.credential_env().contains_key("GH_TOKEN"));
    assert!(!inspector_data(&i, &out).project_env_exists);
}

/// The OAuth client a login or refresh uses is home-trusted: a checked-out
/// repository cannot choose it.
#[test]
fn a_workspace_dotenv_cannot_choose_the_github_oauth_client() {
    const KEY: &str = "OCTOCODE_GITHUB_CLIENT_ID";
    let workspace = resolve(Layers {
        project_env: Some("OCTOCODE_GITHUB_CLIENT_ID=Iv1.workspace"),
        ..NONE
    });
    assert!(workspace.dotenv.skipped_protected.iter().any(|k| k == KEY));
    assert_eq!(workspace.env_value(KEY), None);
    let home = resolve(Layers {
        global_env: Some("OCTOCODE_GITHUB_CLIENT_ID=Iv1.home"),
        project_env: Some("OCTOCODE_GITHUB_CLIENT_ID=Iv1.workspace"),
        ..NONE
    });
    assert_eq!(home.env_value(KEY), Some("Iv1.home"));
    assert_eq!(home.dotenv.skipped_workspace_protected, vec![KEY]);
}
