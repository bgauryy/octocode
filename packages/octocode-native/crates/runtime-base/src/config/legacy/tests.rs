use super::super::types::*;
use super::super::{resolve_config, resolver::get_config_value};
use std::collections::BTreeMap;
use std::path::PathBuf;

const HOME: &str = "/synthetic/.octocode";
const WORKSPACE: &str = "/synthetic/cwd/.octocode";

#[derive(Default)]
struct Sources<'a> {
    process: &'a [(&'a str, &'a str)],
    home_env: Option<String>,
    workspace_env: Option<String>,
    home_rc: Option<String>,
    workspace_rc: Option<String>,
}

fn file(path: String, text: Option<String>) -> FileInput {
    let path = PathBuf::from(path);
    match text {
        Some(text) => FileInput::Read { path, text },
        None => FileInput::Missing { path },
    }
}

fn resolve(sources: Sources<'_>) -> ConfigOutput {
    let mut env: BTreeMap<String, String> = sources
        .process
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    env.insert("OCTOCODE_HOME".into(), HOME.into());
    resolve_config(&ConfigInput {
        env,
        cwd: "/synthetic/cwd".into(),
        os_home: "/synthetic".into(),
        global_env: file(format!("{HOME}/.env"), sources.home_env),
        project_env: file(format!("{WORKSPACE}/.env"), sources.workspace_env),
        config_file: file(format!("{HOME}/.octocoderc"), sources.home_rc),
        project_config_file: file(format!("{WORKSPACE}/.octocoderc"), sources.workspace_rc),
        runtime_surface: RuntimeSurface::Mcp,
    })
}

fn setting_of(name: &str) -> Option<&'static ConfigFieldSpec> {
    CONFIG_FIELDS
        .iter()
        .find(|field| field.env_name() == Some(name))
}

/// A value of the setting `name` binds that differs from its default; any
/// nonblank text for a variable no setting binds (a token).
fn sample(name: &str) -> String {
    match setting_of(name) {
        Some(field) if field.kind == ConfigFieldKind::Boolean => {
            (field.default_json != "true").to_string()
        }
        Some(field) => panic!("add a sample for {}", field.path),
        None => format!("sample-{name}"),
    }
}

/// What a current name decides: its setting's resolved value and source, or the
/// effective value and the selected token's value.
fn outcome(config: &ConfigOutput, name: &str) -> String {
    match setting_of(name) {
        Some(field) => format!(
            "{:?} {:?}",
            get_config_value(&config.resolved, field.path),
            config.setting_source(field.path)
        ),
        None => format!(
            "{:?} {:?}",
            config.env_value(name),
            config.token.as_ref().map(|token| token.token().to_owned())
        ),
    }
}

fn legacy_reports(config: &ConfigOutput) -> Vec<&ConfigDiagnostic> {
    config
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == LEGACY_SETTING_CODE)
        .collect()
}

#[test]
fn the_contract_declares_legacy_names_for_the_renamed_settings() {
    assert!(!LEGACY_ENV_NAMES.is_empty());
    assert!(!LEGACY_CONFIG_PATHS.is_empty());
    for (legacy, current) in LEGACY_ENV_NAMES {
        assert!(DECLARED_ENV_KEYS.contains(current), "{current}");
        assert!(DECLARED_ENV_KEYS.contains(legacy), "{legacy}");
        assert!(
            setting_of(current).is_some() || ENV_TOKEN_VARS.contains(current),
            "{legacy} must name a setting or token variable"
        );
    }
    for (_, current) in LEGACY_CONFIG_PATHS {
        assert!(CONFIG_FIELDS.iter().any(|field| field.path == *current));
    }
}

#[test]
fn a_legacy_name_carries_its_current_names_policy() {
    for (legacy, current) in LEGACY_ENV_NAMES {
        for (set, label) in [
            (PROTECTED_KEYS, "protected"),
            (HOME_TRUSTED_ENV_KEYS, "home-trusted"),
            (ENV_CREDENTIAL_VARS, "credential"),
        ] {
            assert_eq!(
                set.contains(legacy),
                set.contains(current),
                "{legacy} {label}"
            );
        }
        let narrow = |name: &str| {
            WORKSPACE_NARROW_ONLY
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
        };
        assert_eq!(narrow(legacy), narrow(current), "{legacy} narrowing value");
    }
}

#[test]
fn each_legacy_name_sets_what_its_current_name_sets_in_every_source() {
    for (legacy, current) in LEGACY_ENV_NAMES {
        let value = sample(current);
        let process = |name: &str| {
            outcome(
                &resolve(Sources {
                    process: &[(name, &value)],
                    ..Default::default()
                }),
                current,
            )
        };
        assert_eq!(process(legacy), process(current), "{legacy} in the process");
        for workspace in [false, true] {
            let in_file = |name: &str| {
                let text = Some(format!("{name}={value}\n"));
                let sources = if workspace {
                    Sources {
                        workspace_env: text,
                        ..Default::default()
                    }
                } else {
                    Sources {
                        home_env: text,
                        ..Default::default()
                    }
                };
                outcome(&resolve(sources), current)
            };
            assert_eq!(
                in_file(legacy),
                in_file(current),
                "{legacy}, workspace .env: {workspace}"
            );
        }
    }
}

#[test]
fn each_legacy_path_sets_what_its_current_path_sets_in_every_file() {
    for (legacy, current) in LEGACY_CONFIG_PATHS {
        let field = CONFIG_FIELDS
            .iter()
            .find(|field| field.path == *current)
            .expect("declared");
        let value = sample(field.env_name().expect("bound"));
        let json = |path: &str| {
            let (section, key) = path.rsplit_once('.').expect("dotted");
            format!("{{\"{section}\":{{\"{key}\":{value}}}}}")
        };
        for workspace in [false, true] {
            let in_file = |path: &str| {
                let sources = if workspace {
                    Sources {
                        workspace_rc: Some(json(path)),
                        ..Default::default()
                    }
                } else {
                    Sources {
                        home_rc: Some(json(path)),
                        ..Default::default()
                    }
                };
                let config = resolve(sources);
                let warnings: Vec<_> = config
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code != LEGACY_SETTING_CODE)
                    .map(|diagnostic| diagnostic.code.clone())
                    .collect();
                (outcome(&config, field.env_name().expect("bound")), warnings)
            };
            assert_eq!(
                in_file(legacy),
                in_file(current),
                "{legacy}, workspace: {workspace}"
            );
        }
    }
}

#[test]
fn the_current_name_wins_where_one_source_sets_both() {
    for (legacy, current) in LEGACY_ENV_NAMES {
        let value = sample(current);
        let other = match setting_of(current) {
            Some(_) => (value != "true").to_string(),
            None => format!("other-{current}"),
        };
        let both = resolve(Sources {
            process: &[(legacy, &other), (current, &value)],
            ..Default::default()
        });
        let alone = resolve(Sources {
            process: &[(current, &value)],
            ..Default::default()
        });
        assert_eq!(
            outcome(&both, current),
            outcome(&alone, current),
            "{legacy}"
        );
        let reports = legacy_reports(&both);
        assert_eq!(reports.len(), 1, "{legacy}");
        assert!(
            reports[0].message.contains("is ignored"),
            "{}",
            reports[0].message
        );
    }
}

#[test]
fn a_higher_source_legacy_name_beats_a_lower_source_current_name() {
    for (legacy, current) in LEGACY_ENV_NAMES {
        let value = sample(current);
        let lower = match setting_of(current) {
            Some(_) => (value != "true").to_string(),
            None => format!("lower-{current}"),
        };
        let config = resolve(Sources {
            process: &[(legacy, &value)],
            home_env: Some(format!("{current}={lower}\n")),
            ..Default::default()
        });
        let process_only = resolve(Sources {
            process: &[(current, &value)],
            ..Default::default()
        });
        assert_eq!(
            outcome(&config, current),
            outcome(&process_only, current),
            "{legacy}"
        );
    }
}

#[test]
fn a_legacy_token_ranks_right_after_the_current_name_it_is_read_as() {
    for (legacy, current) in LEGACY_ENV_NAMES
        .iter()
        .filter(|(_, current)| ENV_TOKEN_VARS.contains(current))
    {
        let position = ENV_TOKEN_VARS
            .iter()
            .position(|name| name == current)
            .expect("token");
        for later in &ENV_TOKEN_VARS[position + 1..] {
            let config = resolve(Sources {
                process: &[(legacy, "legacy-token"), (later, "later-token")],
                ..Default::default()
            });
            let token = config.token.as_ref().expect("selected");
            assert_eq!(token.token(), "legacy-token", "{legacy} before {later}");
        }
    }
}

#[test]
fn every_legacy_name_is_reported_once_with_its_source_and_never_its_value() {
    for (legacy, current) in LEGACY_ENV_NAMES {
        let value = sample(current);
        for (sources, path) in [
            (
                Sources {
                    process: &[(legacy, &value)],
                    ..Default::default()
                },
                None,
            ),
            (
                Sources {
                    home_env: Some(format!("{legacy}={value}\n")),
                    ..Default::default()
                },
                Some(format!("{HOME}/.env")),
            ),
            (
                Sources {
                    workspace_env: Some(format!("{legacy}={value}\n")),
                    ..Default::default()
                },
                Some(format!("{WORKSPACE}/.env")),
            ),
        ] {
            let config = resolve(sources);
            let reports = legacy_reports(&config);
            assert_eq!(reports.len(), 1, "{legacy}");
            let report = reports[0];
            assert_eq!(report.source_path, path.map(PathBuf::from), "{legacy}");
            assert_eq!(
                report.field_path.as_deref(),
                setting_of(current).map(|field| field.path)
            );
            assert!(
                report.message.contains(legacy) && report.message.contains(current),
                "{}",
                report.message
            );
            assert!(!report.message.contains("sample-"), "{}", report.message);
            assert!(
                config.env_value(legacy).is_none(),
                "{legacy} is read as {current}"
            );
        }
    }
    for (legacy, current) in LEGACY_CONFIG_PATHS {
        let (section, key) = legacy.rsplit_once('.').expect("dotted");
        let config = resolve(Sources {
            home_rc: Some(format!("{{\"{section}\":{{\"{key}\":true}}}}")),
            ..Default::default()
        });
        let reports = legacy_reports(&config);
        assert_eq!(reports.len(), 1, "{legacy}");
        assert_eq!(reports[0].field_path.as_deref(), Some(*current));
        assert!(
            config
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != UNKNOWN_CONFIG_CODE),
            "{legacy} is a known path"
        );
    }
}
