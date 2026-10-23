use super::super::loader::load_config;
use super::super::types::*;

pub(crate) fn inspector_data(input: &ConfigInput, output: &ConfigOutput) -> ConfigInspectorData {
    let home = output.home.clone();
    let top_level_keys = |file: &FileInput| -> Vec<String> {
        let mut keys: Vec<String> = load_config(file)
            .config
            .and_then(|v| v.as_object().map(|o| o.keys().cloned().collect()))
            .unwrap_or_default();
        keys.sort();
        keys
    };
    let skip_source = |key: &String| EnvSkip {
        key: key.clone(),
        source_path: match output.dotenv.sources.get(key).map(String::as_str) {
            Some("project") => input.project_env.path().clone(),
            _ => input.global_env.path().clone(),
        },
    };
    ConfigInspectorData {
        home,
        global_env_path: input.global_env.path().clone(),
        project_env_path: input.project_env.path().clone(),
        project_env_exists: !matches!(input.project_env, FileInput::Missing { .. }),
        loaded_keys: output.dotenv.keys.clone(),
        skipped_protected: output
            .dotenv
            .skipped_protected
            .iter()
            .map(skip_source)
            .chain(
                output
                    .dotenv
                    .skipped_workspace_protected
                    .iter()
                    .map(|key| EnvSkip {
                        key: key.clone(),
                        source_path: input.project_env.path().clone(),
                    }),
            )
            .collect(),
        skipped_existing: output
            .dotenv
            .skipped_existing
            .iter()
            .map(skip_source)
            .collect(),
        global_key_count: output
            .dotenv
            .sources
            .values()
            .filter(|s| s.as_str() == "global")
            .count(),
        project_key_count: output
            .dotenv
            .sources
            .values()
            .filter(|s| s.as_str() == "project")
            .count(),
        storage_mode: output.resolved.storage.mode.as_str().to_owned(),
        config_keys: top_level_keys(&input.config_file),
        source: output.source.clone(),
        config_path: output.config_path.clone(),
        project_config_file: input.project_config_file.path().clone(),
        project_config_path: output.project_config_path.clone(),
        project_config_keys: top_level_keys(&input.project_config_file),
        diagnostics: output.diagnostics.clone(),
    }
}
