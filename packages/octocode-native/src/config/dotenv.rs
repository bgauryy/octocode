use super::types::{EnvApplyReport, PROTECTED_KEYS};
use std::collections::BTreeMap;

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
        let value = normalized[eq + 1..]
            .trim()
            .trim_start_matches(['"', '\''])
            .trim_end_matches(['"', '\'']);
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
    for (key, value) in map {
        if PROTECTED_KEYS.contains(&key.as_str()) {
            report.skipped_protected.push(key.clone());
        } else if target.get(key).is_some_and(|v| !v.is_empty()) {
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
    for (k, v) in parse_env(global) {
        sources.insert(k.clone(), "global".into());
        map.insert(k, v);
    }
    if trusted {
        for (k, v) in parse_env(project) {
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
