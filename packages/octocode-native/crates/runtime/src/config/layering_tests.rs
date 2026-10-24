//! Global + workspace `.octocoderc` layering.
//!
//! Precedence, per field:
//! process env > workspace `.env` > global `.env` > workspace `.octocoderc`
//! > global `.octocoderc` > generated default.
use super::*;
use std::fs;
use std::path::PathBuf;
mod acquisition;
mod diagnostics;
mod hardening;
mod precedence;
mod workspace_trust;

const HOME: &str = "/synthetic/.octocode";
const WORKSPACE_RC: &str = "/synthetic/cwd/.octocode/.octocoderc";
const GLOBAL_RC: &str = "/synthetic/.octocode/.octocoderc";

/// Sets `field_path` in `root`, creating missing sections.
fn insert_path(root: &mut serde_json::Value, field_path: &str, value: serde_json::Value) {
    let mut current = root;
    let mut parts = field_path.split('.').peekable();
    while let Some(part) = parts.next() {
        let object = current.as_object_mut().expect("object path");
        if parts.peek().is_none() {
            object.insert(part.to_owned(), value);
            return;
        }
        current = object.entry(part).or_insert_with(|| serde_json::json!({}));
    }
}

fn file(path: &str, text: Option<&str>) -> FileInput {
    match text {
        Some(text) => FileInput::Read {
            path: path.into(),
            text: text.into(),
        },
        None => FileInput::Missing { path: path.into() },
    }
}

struct Layers<'a> {
    env: &'a [(&'a str, &'a str)],
    global_env: Option<&'a str>,
    project_env: Option<&'a str>,
    global_rc: Option<&'a str>,
    project_rc: Option<&'a str>,
}

const NONE: Layers<'static> = Layers {
    env: &[],
    global_env: None,
    project_env: None,
    global_rc: None,
    project_rc: None,
};

fn input(layers: Layers<'_>) -> ConfigInput {
    ConfigInput {
        env: layers
            .env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        cwd: "/synthetic/cwd".into(),
        os_home: "/synthetic".into(),
        global_env: file("/synthetic/.octocode/.env", layers.global_env),
        project_env: file("/synthetic/cwd/.octocode/.env", layers.project_env),
        config_file: file(GLOBAL_RC, layers.global_rc),
        project_config_file: file(WORKSPACE_RC, layers.project_rc),
        runtime_surface: RuntimeSurface::Cli,
    }
}

fn resolve(layers: Layers<'_>) -> ConfigOutput {
    resolve_config(&input(layers))
}

// ─── 2026-10-14 config hardening (R03/R09/R19–R30) ─────────

fn value_of(out: &ConfigOutput, path: &str) -> serde_json::Value {
    super::get_config_value(&out.resolved, path).unwrap_or(serde_json::Value::Null)
}

fn has_diagnostic(out: &ConfigOutput, code: &str, path: &str) -> bool {
    out.diagnostics
        .iter()
        .any(|d| d.code == code && d.field_path.as_deref() == Some(path))
}
