use super::*;

const SETTING: &str = "storage.cloneCacheLimit";

#[test]
fn the_setting_has_a_default_and_parses_from_file_and_env() {
    let default = value_of(&resolve(NONE), SETTING);
    assert!(default.as_str().is_some_and(|text| !text.is_empty()));

    let from_file = resolve(Layers {
        global_rc: Some(r#"{"storage":{"cloneCacheLimit":"512MiB"}}"#),
        ..NONE
    });
    assert_eq!(value_of(&from_file, SETTING), serde_json::json!("512MiB"));

    let from_env = resolve(Layers {
        env: &[("OCTOCODE_CLONE_CACHE_LIMIT", " 0 ")],
        global_rc: Some(r#"{"storage":{"cloneCacheLimit":"512MiB"}}"#),
        ..NONE
    });
    assert_eq!(value_of(&from_env, SETTING), serde_json::json!("0"));
}

/// A repository cannot choose how much of the user's checkouts are discarded.
#[test]
fn a_workspace_env_file_cannot_set_the_limit() {
    let out = resolve(Layers {
        global_env: Some("OCTOCODE_CLONE_CACHE_LIMIT=2GiB"),
        project_env: Some("OCTOCODE_CLONE_CACHE_LIMIT=1"),
        ..NONE
    });
    assert_eq!(value_of(&out, SETTING), serde_json::json!("2GiB"));
}
