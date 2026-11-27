//! `tools.enableSecurity`: off by default. A dependency list leaves the
//! machine when it is on, so a repository can never turn it on, and may turn it off.
use super::*;

const PATH: &str = "tools.enableSecurity";
const VAR: &str = "OCTOCODE_ENABLE_SECURITY";

fn on(out: &ConfigOutput) -> bool {
    value_of(out, PATH) == serde_json::json!(true)
}

#[test]
fn the_security_check_is_off_by_default() {
    assert!(!on(&resolve(NONE)));
}

#[test]
fn a_repository_cannot_turn_the_security_check_on() {
    let project_env = format!("{VAR}=true");
    let by_env = resolve(Layers {
        project_env: Some(&project_env),
        ..NONE
    });
    assert!(!on(&by_env));
    assert!(
        by_env.dotenv.skipped_protected.iter().any(|key| key == VAR),
        "{:?}",
        by_env.dotenv
    );
    let by_file = resolve(Layers {
        project_rc: Some(r#"{"tools":{"enableSecurity":true}}"#),
        ..NONE
    });
    assert!(!on(&by_file));
    assert!(has_diagnostic(&by_file, "workspace_config_protected", PATH));
    // Against the user's explicit off, too.
    let home_env = format!("{VAR}=false");
    let against_off = resolve(Layers {
        global_env: Some(&home_env),
        project_env: Some(&project_env),
        project_rc: Some(r#"{"tools":{"enableSecurity":true}}"#),
        ..NONE
    });
    assert!(!on(&against_off));
}

#[test]
fn the_user_turns_it_on_and_a_repository_may_turn_it_off() {
    let home_on = format!("{VAR}=true");
    let by_home_env = resolve(Layers {
        global_env: Some(&home_on),
        ..NONE
    });
    assert!(on(&by_home_env));
    assert_eq!(by_home_env.setting_source(PATH), SettingSource::HomeEnv);
    let by_home_file = resolve(Layers {
        global_rc: Some(r#"{"tools":{"enableSecurity":true}}"#),
        ..NONE
    });
    assert!(on(&by_home_file));
    assert!(on(&resolve(Layers {
        env: &[(VAR, "true")],
        ..NONE
    })));
    let project_off = format!("{VAR}=false");
    let env_over_env = resolve(Layers {
        global_env: Some(&home_on),
        project_env: Some(&project_off),
        ..NONE
    });
    assert!(!on(&env_over_env));
    let file_over_file = resolve(Layers {
        global_rc: Some(r#"{"tools":{"enableSecurity":true}}"#),
        project_rc: Some(r#"{"tools":{"enableSecurity":false}}"#),
        ..NONE
    });
    assert!(!on(&file_over_file));
    assert!(
        file_over_file.diagnostics.is_empty(),
        "{:?}",
        file_over_file.diagnostics
    );
    // The ordinary source order: any `.env` beats any file, so the user's own
    // `.env` on outranks a repository's file off.
    let home_env_over_project_file = resolve(Layers {
        global_env: Some(&home_on),
        project_rc: Some(r#"{"tools":{"enableSecurity":false}}"#),
        ..NONE
    });
    assert!(on(&home_env_over_project_file));
}
