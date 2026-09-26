use super::types::{
    CONFIG_FIELDS, ENV_TOKEN_VARS, EnvApplyReport, HOME_TRUSTED_ENV_KEYS, PROTECTED_KEYS,
};
use std::collections::{BTreeMap, BTreeSet};

/// Present-but-blank in the process env disables clasify and every
/// classification feature: no home `.env`, config-file, or vendor-key fallback
/// may refill it.
pub const CLASSIFICATION_KILL_SWITCH: &str = "OCTOCODE_CLASSIFICATION_API";

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
        // Strip at most ONE leading and ONE trailing quote, independently, to
        // match the JS resolver's `/^["']|["']$/g`. `trim_*_matches` stripped
        // all/mismatched quotes and corrupted values like `"'v'"` differently
        // from JS (Rust -> `v`, JS -> `'v'`).
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

pub fn apply_env(
    map: &BTreeMap<String, String>,
    sources: BTreeMap<String, String>,
    target: &mut BTreeMap<String, String>,
) -> EnvApplyReport {
    let mut report = EnvApplyReport {
        sources,
        keys: map.keys().cloned().collect(),
        ..Default::default()
    };
    // Capture source decisions before mutating target: same-source aliases
    // retain their declared priority; lower-source aliases cannot mask them.
    let groups = std::iter::once(ENV_TOKEN_VARS.to_vec()).chain(
        CONFIG_FIELDS
            .iter()
            .filter(|field| field.credential && field.env.len() > 1)
            .map(|field| field.env.iter().map(|binding| binding.name).collect()),
    );
    let mut shadowed = BTreeSet::new();
    for group in groups {
        let process_selected = group.iter().any(|key| {
            target
                .get(*key)
                .is_some_and(|value| !value.trim().is_empty())
                || (*key == CLASSIFICATION_KILL_SWITCH && target.contains_key(*key))
        });
        let workspace_selected = group.iter().any(|key| {
            report.sources.get(*key).map(String::as_str) == Some("project")
                && map.get(*key).is_some_and(|value| !value.trim().is_empty())
        });
        for key in group {
            if process_selected
                || (workspace_selected
                    && report.sources.get(key).map(String::as_str) != Some("project"))
            {
                shadowed.insert(key);
            }
        }
    }
    for (key, value) in map {
        let home_trusted = HOME_TRUSTED_ENV_KEYS.contains(&key.as_str())
            && report.sources.get(key).map(String::as_str) == Some("global");
        // Windows env vars are case-insensitive: match protected keys the same
        // way there so a `.env` `Gh_Token=…` cannot dodge the exact-case check
        // and fold into `GH_TOKEN`. POSIX keeps exact-case semantics.
        let protected_key = PROTECTED_KEYS.contains(&key.as_str())
            || (cfg!(windows)
                && PROTECTED_KEYS
                    .iter()
                    .any(|protected| protected.eq_ignore_ascii_case(key)));
        if protected_key && !home_trusted {
            report.skipped_protected.push(key.clone());
        } else if target.get(key).is_some_and(|v| !v.trim().is_empty())
            || shadowed.contains(key.as_str())
            || (key == CLASSIFICATION_KILL_SWITCH && target.contains_key(key))
        {
            report.skipped_existing.push(key.clone());
        } else {
            target.insert(key.clone(), value.clone());
            report.applied.push(key.clone());
        }
    }
    report
}

pub fn merged_env(
    global: Option<&str>,
    project: Option<&str>,
    trusted: bool,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut map = BTreeMap::new();
    let mut sources = BTreeMap::new();
    for (k, v) in parse_env(global)
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
    {
        sources.insert(k.clone(), "global".into());
        map.insert(k, v);
    }
    if trusted {
        for (k, v) in parse_env(project)
            .into_iter()
            .filter(|(_, v)| !v.trim().is_empty())
        {
            sources.insert(k.clone(), "project".into());
            map.insert(k, v);
        }
    }
    (map, sources)
}

pub fn parse_boolean_env(value: Option<&str>) -> Option<bool> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}
pub fn parse_int_env(value: Option<&str>) -> Option<i64> {
    let s = value?.trim();
    if s.is_empty() {
        return None;
    }
    let (sign, rest) = if let Some(x) = s.strip_prefix('-') {
        (-1, x)
    } else if let Some(x) = s.strip_prefix('+') {
        (1, x)
    } else {
        (1, s)
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<i64>().ok().map(|n| n * sign)
    }
}
pub fn parse_string_array_env(value: Option<&str>) -> Option<Vec<String>> {
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
