//! Legacy setting names through `octocode config`: every declared legacy
//! variable sets its current name from the process and from a `.env`, every
//! legacy `.octocoderc` path sets its current path, and `octocode config`
//! names each one with its file, while tool calls stay quiet.
use super::settings::{config, errors, home_rc, json_of, text};
use super::*;
use octocode_runtime_base::config::{
    CONFIG_FIELDS, ConfigFieldKind, LEGACY_CONFIG_PATHS, LEGACY_ENV_NAMES, LEGACY_SETTING_CODE,
};

/// A value that differs from the default of the setting `name` binds; any
/// text for a variable no setting binds (a token).
fn sample(name: &str) -> String {
    match CONFIG_FIELDS
        .iter()
        .find(|field| field.env_name() == Some(name))
    {
        Some(field) if field.kind == ConfigFieldKind::Boolean => {
            (field.default_json != "true").to_string()
        }
        Some(field) => panic!("add a sample for {}", field.path),
        None => format!("legacy-sample-{name}"),
    }
}

/// `octocode config <args>` with `env` in the process and without the
/// fixture's current local switch, so a legacy one can apply.
fn config_with(workspace: &Workspace, env: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    workspace
        .cli()
        .env_remove("OCTOCODE_ENABLE_LOCAL")
        .envs(env.iter().copied())
        .arg("config")
        .args(args)
        .output()
        .expect("config")
}

#[test]
fn every_legacy_name_sets_its_current_name_from_the_process_and_a_dotenv() {
    assert!(!LEGACY_ENV_NAMES.is_empty());
    for (legacy, current) in LEGACY_ENV_NAMES {
        let value = sample(current);
        for in_file in [false, true] {
            let workspace = Workspace::new();
            let env: &[(&str, &str)] = if in_file {
                std::fs::write(workspace.home.join(".env"), format!("{legacy}={value}\n")).unwrap();
                &[]
            } else {
                &[(legacy, value.as_str())]
            };
            let check = config_with(&workspace, env, &["check", current]);
            assert_eq!(
                check.status.code(),
                Some(0),
                "{legacy} (file: {in_file}): {}",
                errors(&check)
            );
            let field = CONFIG_FIELDS
                .iter()
                .find(|field| field.env_name() == Some(current));
            if let Some(field) = field {
                let got = config_with(&workspace, env, &["get", field.path]);
                assert_eq!(text(&got), value, "{legacy} (file: {in_file})");
            }
            let shown = text(&config_with(&workspace, env, &["list", "--json"]));
            assert!(
                !shown.contains("legacy-sample-"),
                "{legacy} leaked: {shown}"
            );
        }
    }
}

#[test]
fn every_legacy_path_sets_its_current_path() {
    assert!(!LEGACY_CONFIG_PATHS.is_empty());
    for (legacy, current) in LEGACY_CONFIG_PATHS {
        let workspace = Workspace::new();
        let (section, key) = legacy.rsplit_once('.').expect("dotted");
        home_rc(&workspace, &format!("{{\"{section}\":{{\"{key}\":true}}}}"));
        assert_eq!(
            text(&config(&workspace, &["get", current])),
            "true",
            "{legacy}"
        );
    }
}

#[test]
fn config_names_each_legacy_spelling_with_its_file_and_tool_calls_stay_quiet() {
    let workspace = Workspace::new();
    let dotenv: String = LEGACY_ENV_NAMES
        .iter()
        .map(|(legacy, current)| format!("{legacy}={}\n", sample(current)))
        .collect();
    std::fs::write(workspace.home.join(".env"), dotenv).unwrap();
    let rc: Vec<String> = LEGACY_CONFIG_PATHS
        .iter()
        .map(|(legacy, _)| {
            let (section, key) = legacy.rsplit_once('.').expect("dotted");
            format!("\"{section}\":{{\"{key}\":true}}")
        })
        .collect();
    home_rc(&workspace, &format!("{{{}}}", rc.join(",")));

    let view = json_of(&config_with(&workspace, &[], &["--json"]));
    let reports: Vec<&serde_json::Value> = view["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|diagnostic| diagnostic["code"] == LEGACY_SETTING_CODE)
        .collect();
    let named = |legacy: &str, current: &str, file: &str| {
        reports
            .iter()
            .filter(|report| {
                let message = report["message"].as_str().unwrap_or_default();
                message.starts_with(&format!("{legacy} "))
                    && message.contains(current)
                    && report["source_path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with(file))
            })
            .count()
    };
    for (legacy, current) in LEGACY_ENV_NAMES {
        assert_eq!(named(legacy, current, ".env"), 1, "{legacy}: {view}");
    }
    for (legacy, current) in LEGACY_CONFIG_PATHS {
        assert_eq!(named(legacy, current, ".octocoderc"), 1, "{legacy}: {view}");
    }
    assert_eq!(
        reports.len(),
        LEGACY_ENV_NAMES.len() + LEGACY_CONFIG_PATHS.len(),
        "{view}"
    );
    assert!(!view.to_string().contains("legacy-sample-"), "{view}");

    let call = workspace
        .cli()
        .env_remove("OCTOCODE_ENABLE_LOCAL")
        .args([
            "localSearch",
            r#"{"queries":[{"path":".","matchString":["x"]}]}"#,
        ])
        .output()
        .expect("tool call");
    assert!(
        !errors(&call).contains(LEGACY_SETTING_CODE),
        "{}",
        errors(&call)
    );
}
