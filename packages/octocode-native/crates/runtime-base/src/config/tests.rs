use super::dotenv::{apply_env, merged_env};
use super::resolver::resolve_env_token;
use super::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
mod credentials;
mod parsing;
/// A valid value of `key` labeled `label`: an http(s) URL for a URL
/// setting's variable, `label` itself otherwise.
fn valid_value(key: &str, label: &str) -> String {
    let url = CONFIG_FIELDS
        .iter()
        .any(|field| field.kind == ConfigFieldKind::Url && field.env_name() == Some(key));
    if url {
        format!("https://{label}.example")
    } else {
        label.to_owned()
    }
}
fn input(env: BTreeMap<String, String>, file: Option<&str>) -> ConfigInput {
    let home = std::path::PathBuf::from("/synthetic/home");
    ConfigInput {
        env,
        cwd: "/synthetic/cwd".into(),
        os_home: "/synthetic".into(),
        global_env: FileInput::Missing {
            path: home.join(".env"),
        },
        project_env: FileInput::Missing {
            path: "/synthetic/cwd/.octocode/.env".into(),
        },
        config_file: match file {
            Some(text) => FileInput::Read {
                path: home.join(".octocoderc"),
                text: text.into(),
            },
            None => FileInput::Missing {
                path: home.join(".octocoderc"),
            },
        },
        project_config_file: FileInput::Missing {
            path: "/synthetic/cwd/.octocode/.octocoderc".into(),
        },
        runtime_surface: RuntimeSurface::Mcp,
    }
}
