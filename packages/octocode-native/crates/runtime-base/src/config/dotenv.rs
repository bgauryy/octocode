use super::legacy::{LegacyUse, read_legacy_env};
use super::types::{
    ENV_TOKEN_VARS, EnvApplyReport, HOME_TRUSTED_ENV_KEYS, KNOWN_ENV, PROTECTED_KEYS,
    WORKSPACE_NARROW_ONLY,
};
use std::collections::{BTreeMap, BTreeSet};

/// The classification credential's variable: present-but-blank in the
/// process env, it disables clasify and every classification feature, and no
/// home `.env`, config-file, or vendor-key fallback may refill it.
pub(super) fn is_classification_kill_switch(key: &str) -> bool {
    super::field::CLASSIFICATION_API.env_name() == Some(key)
}

/// Home-trusted switches stay home-trusted (a checked-out repository must not
/// widen where octocode writes), but a workspace may set the narrowing value
/// the contract names, so a project that opts out of persistence keeps working.
pub(super) fn workspace_may_narrow(key: &str, value: &str) -> bool {
    WORKSPACE_NARROW_ONLY
        .iter()
        .any(|(name, safe)| *name == key && value.trim().eq_ignore_ascii_case(safe))
}

/// An API key (a contract entry of type `key`): process > home `.env` >
/// workspace `.env`, so a repository only fills a key the user has not set.
/// `@octocodeai/config` `getKey` reads every key in this same order.
pub(super) fn is_api_key(key: &str) -> bool {
    KNOWN_ENV
        .iter()
        .any(|known| known.kind == "key" && known.name == key)
}

pub fn parse_env(text: Option<&str>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(text) = text else { return out };
    for raw in text.split('\n') {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let normalized = trimmed
            .strip_prefix("export ")
            .map(str::trim)
            .unwrap_or(trimmed);
        let Some(eq) = normalized.find('=') else {
            continue;
        };
        let key = normalized[..eq].trim();
        if key.is_empty() {
            continue;
        }
        // Strip at most ONE leading and ONE trailing quote, independently:
        // `trim_*_matches` would strip all or mismatched quotes and corrupt a
        // value like `"'v'"` (it keeps `'v'`).
        let trimmed_value = normalized[eq + 1..].trim();
        let trimmed_value = trimmed_value
            .strip_prefix(['"', '\''])
            .unwrap_or(trimmed_value);
        let value = trimmed_value
            .strip_suffix(['"', '\''])
            .unwrap_or(trimmed_value);
        out.insert(key.to_owned(), value.to_owned());
    }
    out
}

/// The home and workspace `.env` files merged: the workspace wins per key,
/// except an API key the home file sets.
pub(crate) struct MergedEnv {
    pub map: BTreeMap<String, String>,
    /// Which file each key in `map` came from: `global` or `project`.
    pub sources: BTreeMap<String, String>,
    /// Workspace keys dropped because the key is protected and the home file
    /// sets it: the home value stands and the workspace value is reported.
    pub(crate) workspace_protected: Vec<String>,
    /// The legacy names each file set, read as their current names.
    pub(crate) legacy_home: Vec<LegacyUse>,
    pub(crate) legacy_workspace: Vec<LegacyUse>,
}

pub(crate) fn apply_env(
    merged: MergedEnv,
    target: &mut BTreeMap<String, String>,
) -> EnvApplyReport {
    let MergedEnv {
        map,
        sources,
        workspace_protected,
        ..
    } = merged;
    let mut report = EnvApplyReport {
        sources,
        keys: map.keys().cloned().collect(),
        skipped_workspace_protected: workspace_protected,
        ..Default::default()
    };
    // Capture source decisions before mutating target: the GitHub token
    // variables keep their declared priority within a source, and a lower
    // source's token cannot mask a higher source's. Tokens are API keys, so
    // the home file outranks the workspace one.
    let from_project = |key: &str| report.sources.get(key).map(String::as_str) == Some("project");
    let from_home = |key: &str| report.sources.get(key).map(String::as_str) == Some("global");
    let process_selected = ENV_TOKEN_VARS.iter().any(|key| {
        target
            .get(*key)
            .is_some_and(|value| !value.trim().is_empty())
    });
    let home_selected = ENV_TOKEN_VARS
        .iter()
        .any(|key| from_home(key) && map.get(*key).is_some_and(|value| !value.trim().is_empty()));
    let shadowed: BTreeSet<&str> = ENV_TOKEN_VARS
        .iter()
        .copied()
        .filter(|key| process_selected || (home_selected && from_project(key)))
        .collect();
    for (key, value) in &map {
        let workspace = report.sources.get(key).map(String::as_str) == Some("project");
        if !dotenv_may_set(key, value, workspace) {
            report.skipped_protected.push(key.clone());
        } else if target.get(key).is_some_and(|v| !v.trim().is_empty())
            || shadowed.contains(key.as_str())
            || (is_classification_kill_switch(key) && target.contains_key(key))
        {
            report.skipped_existing.push(key.clone());
        } else {
            target.insert(key.clone(), value.clone());
            report.applied.push(key.clone());
        }
    }
    report
}

/// Whether a `.env` in this scope may set `key=value`. A protected key loads
/// only from the home file when it is home-trusted, or from a workspace file
/// when the value narrows. Loading and editing share this one rule.
pub(super) fn dotenv_may_set(key: &str, value: &str, workspace: bool) -> bool {
    !is_protected_key(key)
        || if workspace {
            workspace_may_narrow(key, value)
        } else {
            HOME_TRUSTED_ENV_KEYS.contains(&key)
        }
}

// Windows env vars are case-insensitive: match protected keys the same way
// there so a `.env` `Gh_Token=…` cannot dodge the exact-case check and fold
// into `GH_TOKEN`. POSIX keeps exact-case semantics.
fn is_protected_key(key: &str) -> bool {
    PROTECTED_KEYS.contains(&key)
        || (cfg!(windows)
            && PROTECTED_KEYS
                .iter()
                .any(|protected| protected.eq_ignore_ascii_case(key)))
}

/// One `.env` file's nonblank entries, its legacy names read as their
/// current names.
fn file_env(text: Option<&str>) -> (BTreeMap<String, String>, Vec<LegacyUse>) {
    let mut env = parse_env(text);
    env.retain(|_, value| !value.trim().is_empty());
    let legacy = read_legacy_env(&mut env);
    (env, legacy)
}

pub(crate) fn merged_env(global: Option<&str>, project: Option<&str>) -> MergedEnv {
    let (global, legacy_home) = file_env(global);
    let (project, legacy_workspace) = file_env(project);
    let mut merged = MergedEnv {
        map: BTreeMap::new(),
        sources: BTreeMap::new(),
        workspace_protected: vec![],
        legacy_home,
        legacy_workspace,
    };
    for (k, v) in global {
        merged.sources.insert(k.clone(), "global".into());
        merged.map.insert(k, v);
    }
    for (k, v) in project {
        // A workspace value for a protected key must not evict the trusted
        // home value for that key; it is dropped and reported.
        if merged.map.contains_key(&k) && !dotenv_may_set(&k, &v, true) {
            merged.workspace_protected.push(k);
            continue;
        }
        // An API key the home file sets stays the home one.
        if merged.map.contains_key(&k) && is_api_key(&k) {
            continue;
        }
        merged.sources.insert(k.clone(), "project".into());
        merged.map.insert(k, v);
    }
    merged
}

pub(crate) fn parse_boolean_env(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}
pub(crate) fn parse_string_array_env(value: Option<&str>) -> Option<Vec<String>> {
    let s = value?.trim();
    if s.is_empty() {
        None
    } else {
        Some(
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests;
