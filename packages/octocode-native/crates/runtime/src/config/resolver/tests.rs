use crate::config::validation::is_http_url;

use super::resolve_sections;
use std::collections::BTreeMap;

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn blank_classification_key_in_env_is_an_opt_out_not_a_missing_value() {
    use super::{ConfigInput, FileInput, RuntimeSurface, resolve_config};
    let home = std::path::PathBuf::from("/synthetic/home");
    let input = |env: BTreeMap<String, String>| ConfigInput {
        env,
        cwd: "/synthetic/cwd".into(),
        os_home: "/synthetic".into(),
        global_env: FileInput::Read {
            path: home.join(".env"),
            text: "OCTOCODE_CLASSIFICATION_API=from-home-env".into(),
        },
        project_env: FileInput::Missing {
            path: "/synthetic/cwd/.octocode/.env".into(),
        },
        config_file: FileInput::Read {
            path: home.join(".octocoderc"),
            text: "{\"classification\":{\"api\":\"from-config-file\"}}".into(),
        },
        project_config_file: FileInput::Missing {
            path: "/synthetic/cwd/.octocode/.octocoderc".into(),
        },
        runtime_surface: RuntimeSurface::Mcp,
    };
    assert_eq!(
        resolve_config(&input(BTreeMap::new())).env_value("OCTOCODE_CLASSIFICATION_API"),
        Some("from-home-env")
    );
    for blank in ["", "  "] {
        let out = resolve_config(&input(env(&[("OCTOCODE_CLASSIFICATION_API", blank)])));
        assert_eq!(
            out.env_value("OCTOCODE_CLASSIFICATION_API").map(str::trim),
            Some(""),
            "blank {blank:?} must not be refilled from .env or .octocoderc"
        );
    }
}

#[test]
fn is_http_url_accepts_http_and_https_only() {
    assert!(is_http_url("https://ghe.internal/api/v3"));
    assert!(is_http_url("http://127.0.0.1:8080/api/v3"));
    assert!(!is_http_url("ftp://evil.example"));
    assert!(!is_http_url("file:///etc/passwd"));
    assert!(!is_http_url("not a url"));
}

#[test]
fn non_http_env_api_url_falls_back_to_default() {
    let resolved = resolve_sections(&[], &env(&[("GITHUB_API_URL", "ftp://evil.example")]));
    assert!(resolved.is_ok());
    assert_eq!(
        resolved.unwrap_or_default().github.api_url,
        "https://api.github.com"
    );
}

#[test]
fn https_env_api_url_is_honored() {
    let resolved = resolve_sections(
        &[],
        &env(&[("GITHUB_API_URL", "https://ghe.internal/api/v3")]),
    );
    assert!(resolved.is_ok());
    assert_eq!(
        resolved.unwrap_or_default().github.api_url,
        "https://ghe.internal/api/v3"
    );
}
