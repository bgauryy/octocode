//! Legacy setting names that Octocode still reads. The config contract
//! declares them beside the setting they set (`legacyNames`, `legacyPaths`),
//! and the build renders [`LEGACY_ENV_NAMES`] and [`LEGACY_CONFIG_PATHS`].
//! Each source (the process environment, each `.env` file, each
//! `.octocoderc`) is read with its legacy names moved to their current names
//! before anything else sees it, so precedence, workspace trust, token order
//! and reports apply to one name. The current name wins where one source sets
//! both. `octocode config` reports each legacy name it read or ignored.

use super::types::{
    CONFIG_FIELDS, ConfigDiagnostic, LEGACY_CONFIG_PATHS, LEGACY_ENV_NAMES, LEGACY_SETTING_CODE,
    Severity,
};
use super::validation::get_path;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A legacy name one source sets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LegacyUse {
    pub(crate) legacy: &'static str,
    pub(crate) current: &'static str,
    /// Whether the value was read: the source does not set the current name.
    pub(crate) read: bool,
}

/// Move each legacy variable in `env` to its current name. A blank value is
/// no value, as everywhere else in configuration.
pub(crate) fn read_legacy_env(env: &mut BTreeMap<String, String>) -> Vec<LegacyUse> {
    let set = |env: &BTreeMap<String, String>, name: &str| {
        env.get(name).is_some_and(|value| !value.trim().is_empty())
    };
    let mut uses = Vec::new();
    for &(legacy, current) in LEGACY_ENV_NAMES {
        if !set(env, legacy) {
            continue;
        }
        let read = !set(env, current);
        if let Some(value) = env.remove(legacy)
            && read
        {
            env.insert(current.to_owned(), value);
        }
        uses.push(LegacyUse {
            legacy,
            current,
            read,
        });
    }
    uses
}

/// Move each legacy path in a parsed `.octocoderc` to its current path.
pub(super) fn read_legacy_paths(file: &mut Value) -> Vec<LegacyUse> {
    let mut uses = Vec::new();
    for &(legacy, current) in LEGACY_CONFIG_PATHS {
        let Some(value) = take_path(file, legacy) else {
            continue;
        };
        let read = get_path(file, current).is_none();
        if read {
            insert_path(file, current, value);
        }
        uses.push(LegacyUse {
            legacy,
            current,
            read,
        });
    }
    uses
}

fn take_path(root: &mut Value, path: &str) -> Option<Value> {
    let (parents, key) = path.rsplit_once('.').unwrap_or(("", path));
    let mut current = root;
    for part in parents.split('.').filter(|part| !part.is_empty()) {
        current = current.as_object_mut()?.get_mut(part)?;
    }
    current.as_object_mut()?.remove(key)
}

fn insert_path(root: &mut Value, path: &str, value: Value) {
    let (parents, key) = path.rsplit_once('.').unwrap_or(("", path));
    let mut current = root;
    for part in parents.split('.').filter(|part| !part.is_empty()) {
        let Some(object) = current.as_object_mut() else {
            return;
        };
        current = object
            .entry(part)
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if let Some(object) = current.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
}

/// The report of one legacy name: where it was set, what Octocode did with it
/// and what to change. `source_path` is `None` for the process environment.
pub(crate) fn legacy_diagnostic(
    found: &LegacyUse,
    path: bool,
    source_path: Option<PathBuf>,
) -> ConfigDiagnostic {
    let (noun, verb) = if path {
        ("path", "move")
    } else {
        ("name", "rename")
    };
    let process = if source_path.is_none() {
        " (process environment)"
    } else {
        ""
    };
    let LegacyUse {
        legacy,
        current,
        read,
    } = found;
    let message = if *read {
        format!(
            "{legacy}{process} is a legacy {noun} of {current} and is read as {current}; {verb} it to {current}"
        )
    } else {
        format!(
            "{legacy}{process} is a legacy {noun} of {current}, which the same source also sets; {legacy} is ignored, remove it"
        )
    };
    let field_path = if path {
        Some((*current).to_owned())
    } else {
        CONFIG_FIELDS
            .iter()
            .find(|field| field.env_name() == Some(*current))
            .map(|field| field.path.to_owned())
    };
    ConfigDiagnostic {
        severity: Severity::Warning,
        code: LEGACY_SETTING_CODE.into(),
        field_path,
        message,
        source_path,
    }
}

#[cfg(test)]
mod tests;
