//! What a person sees and changes through `octocode config`: `.octocoderc`
//! layering, `get`, `check`, `list` and `set`. Providers: `provider_host`.
use super::*;
use octocode_native::config::{CONFIG_FIELDS, KNOWN_ENV};
use std::process::Output;

pub(super) const HOST_ENV: &str = "OCTOCODE_CLASSIFICATION_API_HOST";
pub(super) const HOST_PATH: &str = "classification.apiHost";

pub(super) fn home_rc(workspace: &Workspace, text: &str) {
    std::fs::write(workspace.home.join(".octocoderc"), text).unwrap();
}

pub(super) fn workspace_dir(workspace: &Workspace) -> std::path::PathBuf {
    let dir = workspace.workspace.join(".octocode");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(super) fn config(workspace: &Workspace, args: &[&str]) -> Output {
    workspace
        .cli()
        .arg("config")
        .args(args)
        .output()
        .expect("config")
}

pub(super) fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned()
}

pub(super) fn errors(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub(super) fn json_of(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

// ─── `config get` ──────────────────────────────────────────────────────

#[test]
fn get_reads_a_setting_from_the_home_octocoderc_by_path_and_by_variable() {
    let workspace = Workspace::new();
    home_rc(&workspace, r#"{"lsp":{"prewarm":"off"}}"#);
    for key in ["lsp.prewarm", "OCTOCODE_LSP_PREWARM"] {
        let output = config(&workspace, &["get", key]);
        assert_eq!(output.status.code(), Some(0), "{key}: {}", errors(&output));
        assert_eq!(text(&output), "off", "{key}");
    }
    let detail = json_of(&config(&workspace, &["get", "lsp.prewarm", "--json"]));
    assert_eq!(detail["value"], "off", "{detail}");
    assert_eq!(detail["set"], true, "{detail}");
    assert_eq!(detail["source"], "home", "{detail}");
    assert_eq!(
        detail["path"],
        workspace.home.join(".octocoderc").to_str().unwrap(),
        "{detail}"
    );
}

#[test]
fn get_falls_back_to_a_settings_default_and_says_so() {
    let workspace = Workspace::new();
    let output = config(&workspace, &["get", "lsp.prewarm"]);
    assert_eq!(output.status.code(), Some(0), "{}", errors(&output));
    assert_eq!(text(&output), "targeted");
    let detail = json_of(&config(&workspace, &["get", "lsp.prewarm", "--json"]));
    assert_eq!(detail["set"], false, "{detail}");
    assert_eq!(detail["source"], "default", "{detail}");
    assert_eq!(detail["value"], "targeted", "{detail}");
    assert_eq!(detail["default"], "targeted", "{detail}");
}

#[test]
fn get_of_a_setting_nothing_sets_exits_one_and_names_its_default() {
    let workspace = Workspace::new();
    let output = config(&workspace, &["get", HOST_PATH]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output), "");
    assert!(
        errors(&output).contains("https://api.typesafe.ai"),
        "{}",
        errors(&output)
    );
    let detail = json_of(&config(&workspace, &["get", HOST_PATH, "--json"]));
    assert_eq!(detail["set"], false, "{detail}");
    assert_eq!(detail["value"], serde_json::Value::Null, "{detail}");
    assert!(
        detail["default"]
            .as_str()
            .is_some_and(|default| default.contains("https://api.typesafe.ai")),
        "{detail}"
    );
}

#[test]
fn get_shows_the_classification_host_but_never_the_key() {
    let workspace = Workspace::new();
    home_rc(
        &workspace,
        r#"{"classification":{"api":"rc-secret-key","apiHost":"https://proxy.example"}}"#,
    );
    for key in [HOST_PATH, HOST_ENV] {
        let output = config(&workspace, &["get", key]);
        assert_eq!(output.status.code(), Some(0), "{}", errors(&output));
        assert_eq!(text(&output), "https://proxy.example", "{key}");
    }
    for key in ["classification.api", "OCTOCODE_CLASSIFICATION_API"] {
        let output = config(&workspace, &["get", key]);
        assert_eq!(output.status.code(), Some(0), "{}", errors(&output));
        assert!(
            !text(&output).contains("rc-secret-key"),
            "{}",
            text(&output)
        );
        assert!(text(&output).contains("REDACTED"), "{}", text(&output));
        assert!(text(&output).contains(".octocoderc"), "{}", text(&output));
    }
}

/// A hand-edited URL with a secret still applies, so `get` reports it, without
/// the secret. A call then fails closed: see the provider test below.
#[test]
fn get_never_prints_a_secret_embedded_in_a_hand_edited_host() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".env"),
        format!("{HOST_ENV}=https://user:hand-edited-secret@host.example\n"),
    )
    .unwrap();
    let output = config(&workspace, &["get", HOST_PATH]);
    assert_eq!(output.status.code(), Some(0), "{}", errors(&output));
    assert_eq!(text(&output), "https://host.example/");
    let listed = config(&workspace, &["list", "--json"]);
    let all = format!("{}{}{}", errors(&output), text(&listed), errors(&listed));
    assert!(!all.contains("hand-edited-secret"), "{all}");
}

#[test]
fn get_of_an_undeclared_key_keeps_reading_the_environment_layers() {
    let workspace = Workspace::new();
    let unset = config(&workspace, &["get", "SOME_OTHER_NAME"]);
    assert_eq!(unset.status.code(), Some(1));
    std::fs::write(workspace.home.join(".env"), "SOME_OTHER_NAME=plain\n").unwrap();
    assert_eq!(
        text(&config(&workspace, &["get", "SOME_OTHER_NAME"])),
        "plain"
    );
}

// ─── `config check` ────────────────────────────────────────────────────

#[test]
fn check_counts_a_setting_held_in_an_octocoderc() {
    let workspace = Workspace::new();
    for key in ["local.beta", "OCTOCODE_BETA"] {
        assert_eq!(
            config(&workspace, &["check", key]).status.code(),
            Some(1),
            "{key}"
        );
    }
    home_rc(&workspace, r#"{"local":{"beta":true}}"#);
    for key in ["local.beta", "OCTOCODE_BETA"] {
        let output = config(&workspace, &["check", key]);
        assert_eq!(output.status.code(), Some(0), "{key}: {}", text(&output));
        assert!(text(&output).contains(": set"), "{}", text(&output));
    }
}

// ─── `config list` ─────────────────────────────────────────────────────

#[test]
fn list_reports_every_setting_with_value_source_and_default() {
    let workspace = Workspace::new();
    home_rc(
        &workspace,
        r#"{"lsp":{"prewarm":"off"},"classification":{"api":"rc-secret-key"}}"#,
    );
    let output = config(&workspace, &["list", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", errors(&output));
    let listed = json_of(&output);
    let rows = listed["settings"].as_array().expect("settings");
    assert_eq!(
        rows.len(),
        CONFIG_FIELDS.len() + KNOWN_ENV.len(),
        "{listed}"
    );
    for field in CONFIG_FIELDS {
        let row = rows
            .iter()
            .find(|row| row["setting"] == field.path)
            .unwrap_or_else(|| panic!("{} is missing", field.path));
        assert_eq!(row["key"].as_str(), field.env_name(), "{row}");
        assert!(row.get("default").is_some(), "{row}");
        assert!(row["source"].is_string(), "{row}");
        assert!(row["workspace"].is_string(), "{row}");
    }
    let row = |key: &str| {
        rows.iter()
            .find(|row| row["setting"] == key)
            .unwrap()
            .clone()
    };
    assert_eq!(row("lsp.prewarm")["value"], "off");
    assert_eq!(row("lsp.prewarm")["source"], "home");
    assert_eq!(row("lsp.prewarm")["default"], "targeted");
    assert_eq!(row("local.beta")["source"], "default");
    assert_eq!(row("local.beta")["value"], false);
    assert_eq!(row(HOST_PATH)["workspace"], "no");
    assert!(
        row(HOST_PATH)["default"]
            .as_str()
            .is_some_and(|default| default.contains("https://api.typesafe.ai")),
        "{}",
        row(HOST_PATH)
    );
    assert_eq!(row("classification.api")["credential"], true);
    assert_eq!(row("classification.api")["set"], true);
    assert!(!listed.to_string().contains("rc-secret-key"), "{listed}");
}

#[test]
fn list_prints_one_line_per_setting_with_its_default() {
    let workspace = Workspace::new();
    home_rc(
        &workspace,
        r#"{"lsp":{"prewarm":"off"},"classification":{"api":"rc-secret-key"}}"#,
    );
    let output = config(&workspace, &["list"]);
    assert_eq!(output.status.code(), Some(0), "{}", errors(&output));
    let body = text(&output);
    assert!(!body.contains("rc-secret-key"), "{body}");
    for field in CONFIG_FIELDS {
        assert!(
            body.lines().any(|line| line.starts_with(field.path)),
            "{} missing from:\n{body}",
            field.path
        );
    }
    let line = body
        .lines()
        .find(|line| line.starts_with("lsp.prewarm"))
        .unwrap();
    assert!(line.contains("off"), "{line}");
    assert!(line.contains("targeted"), "{line}");
    assert!(line.contains("home"), "{line}");
}

// ─── `.octocoderc` format and layering ─────────────────────────────────

#[test]
fn an_octocoderc_accepts_comments_trailing_commas_and_a_schema_key() {
    let workspace = Workspace::new();
    home_rc(
        &workspace,
        "// settings\n{\n  \"$schema\": \"https://octocode.ai/schemas/config-v1.json\",\n  /* block */\n  \"lsp\": { \"prewarm\": \"all\", },\n}\n",
    );
    let output = config(&workspace, &["get", "lsp.prewarm"]);
    assert_eq!(text(&output), "all");
    assert_eq!(errors(&output), "", "a clean file warns about nothing");
}

#[test]
fn a_workspace_octocoderc_wins_per_field_and_the_home_file_fills_the_rest() {
    let workspace = Workspace::new();
    home_rc(
        &workspace,
        r#"{"lsp":{"prewarm":"all"},"classification":{"model":"home-model"}}"#,
    );
    std::fs::write(
        workspace_dir(&workspace).join(".octocoderc"),
        r#"{"lsp":{"prewarm":"off"}}"#,
    )
    .unwrap();
    let prewarm = json_of(&config(&workspace, &["get", "lsp.prewarm", "--json"]));
    assert_eq!(prewarm["value"], "off", "{prewarm}");
    assert_eq!(prewarm["source"], "workspace", "{prewarm}");
    assert_eq!(
        text(&config(&workspace, &["get", "classification.model"])),
        "home-model"
    );
}

#[test]
fn an_environment_value_beats_both_octocoderc_files() {
    let workspace = Workspace::new();
    home_rc(&workspace, r#"{"lsp":{"prewarm":"all"}}"#);
    std::fs::write(workspace.home.join(".env"), "OCTOCODE_LSP_PREWARM=off\n").unwrap();
    let dotenv = json_of(&config(&workspace, &["get", "lsp.prewarm", "--json"]));
    assert_eq!(dotenv["value"], "off", "{dotenv}");
    assert_eq!(
        dotenv["path"],
        workspace.home.join(".env").to_str().unwrap()
    );
    let process = workspace
        .cli()
        .env("OCTOCODE_LSP_PREWARM", "targeted")
        .args(["config", "get", "lsp.prewarm", "--json"])
        .output()
        .unwrap();
    let process = json_of(&process);
    assert_eq!(process["value"], "targeted", "{process}");
    assert_eq!(process["source"], "env", "{process}");
}

#[test]
fn a_bad_octocoderc_warns_and_falls_through_to_the_default() {
    // An unknown key stays off startup stderr, so a file written for a newer
    // version does not warn on every call: `config` reports it.
    for (rc, code, on_stderr) in [
        (r#"{"lsp":{"prewarm":"sometimes"}}"#, "invalid_config", true),
        (
            r#"{"lsp":{"prewarmm":"off"}}"#,
            "unknown_or_future_config",
            false,
        ),
        ("{not json", "config_load_error", true),
    ] {
        let workspace = Workspace::new();
        home_rc(&workspace, rc);
        let output = config(&workspace, &["get", "lsp.prewarm"]);
        assert_eq!(text(&output), "targeted", "{rc}");
        assert_eq!(
            errors(&output).contains(code),
            on_stderr,
            "{rc}: {}",
            errors(&output)
        );
        let shown = config(&workspace, &["--json"]);
        assert!(
            json_of(&shown)["diagnostics"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["code"] == code)),
            "{rc}"
        );
    }
}

// ─── `config set` on the classification host ───────────────────────────

#[test]
fn set_and_unset_move_the_host_between_a_value_and_its_default() {
    let workspace = Workspace::new();
    let set = config(&workspace, &["set", HOST_PATH, "https://proxy.example"]);
    assert_eq!(set.status.code(), Some(0), "{}", errors(&set));
    assert_eq!(
        text(&config(&workspace, &["get", HOST_PATH])),
        "https://proxy.example"
    );
    let listed = json_of(&config(&workspace, &["get", HOST_ENV, "--json"]));
    assert_eq!(listed["source"], "home", "{listed}");
    assert_eq!(
        listed["path"],
        workspace.home.join(".env").to_str().unwrap()
    );
    let unset = config(&workspace, &["unset", HOST_PATH]);
    assert_eq!(unset.status.code(), Some(0), "{}", errors(&unset));
    assert_eq!(
        config(&workspace, &["get", HOST_PATH]).status.code(),
        Some(1)
    );
}

#[test]
fn set_refuses_a_host_that_could_never_work_without_echoing_it() {
    let workspace = Workspace::new();
    for value in [
        "not-a-url",
        "https://user:typed-secret@proxy.example",
        "https://proxy.example/?token=typed-secret",
    ] {
        let output = config(&workspace, &["set", HOST_ENV, value]);
        assert_eq!(output.status.code(), Some(2), "{value}");
        let all = format!("{}{}", text(&output), errors(&output));
        assert!(!all.contains("typed-secret"), "{all}");
    }
    assert!(!workspace.home.join(".env").exists(), "nothing was stored");
}
