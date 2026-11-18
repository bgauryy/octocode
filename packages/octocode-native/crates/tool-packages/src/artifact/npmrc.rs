//! Origin- and path-scoped npm registry credentials from the user npmrc.
//!
//! Only the user config (`NPM_CONFIG_USERCONFIG`, else `~/.npmrc`) is read,
//! both resolved from the runtime's environment, never the process's.
//! A project `.npmrc` is never consulted: it is repository-controlled and
//! must not be able to steer the user's token anywhere. The registry always
//! comes from the query, and a token is attached only when its
//! `//host[:port]/path/:_authToken` key matches that request's origin and path
//! (npm "nerf-dart" matching, longest path prefix wins).
use super::types::NpmAuthorization;
use secrecy::SecretString;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use url::Url;

/// Path of the user npmrc in `env`, following npm's `userconfig`
/// resolution: the userconfig variable, else `.npmrc` in the home directory.
fn user_npmrc_path(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    let set = |name: &str| env.get(name).filter(|value| !value.is_empty());
    set("NPM_CONFIG_USERCONFIG")
        .or_else(|| set("npm_config_userconfig"))
        .map(PathBuf::from)
        .or_else(|| {
            set("HOME")
                .or_else(|| set("USERPROFILE"))
                .map(|home| Path::new(home).join(".npmrc"))
        })
}

/// Authorization header value for `registry` from the user npmrc `env`
/// names, expanding `${NAME}` from `env`.
pub fn npm_authorization(
    registry: &Url,
    env: &BTreeMap<String, String>,
) -> Option<NpmAuthorization> {
    let path = user_npmrc_path(env)?;
    // npmrc files are tiny; refuse anything implausibly large.
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return None;
    }
    let contents = std::fs::read_to_string(&path).ok()?;
    authorization_for(registry, &contents, |name| env.get(name).cloned())
}

/// `//host[:port]/path/` for a registry URL, as npm keys credentials.
pub(crate) fn nerf_dart(registry: &Url) -> Option<String> {
    let host = registry.host_str()?.to_ascii_lowercase();
    let port = registry
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    let path = registry.path().trim_end_matches('/');
    Some(format!("//{host}{port}{path}/"))
}

/// Normalize a credential key's scope: lowercase authority, trailing slash.
fn normalize_scope(scope: &str) -> Option<String> {
    let rest = scope.strip_prefix("//")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.is_empty() {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() {
        String::new()
    } else {
        format!("/{path}")
    };
    Some(format!("//{}{path}/", authority.to_ascii_lowercase()))
}

/// Expand `${NAME}` references; any unset variable voids the value.
fn expand_env(value: &str, env: &impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let end = tail.find('}')?;
        out.push_str(&env(&tail[..end])?);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

pub(crate) fn authorization_for(
    registry: &Url,
    contents: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Option<NpmAuthorization> {
    // A nerf-dart key carries no scheme: never send it in clear text, except
    // to a loopback registry (a local Verdaccio) that never leaves the host.
    if registry.scheme() != "https" && !octocode_github::is_loopback(registry) {
        return None;
    }
    let target = nerf_dart(registry)?;
    // Keep the winning scope so every later request can enforce it.
    let mut best: Option<(String, String)> = None;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value);
        let (scope, scheme) = if let Some(scope) = key.strip_suffix(":_authToken") {
            (scope, "Bearer")
        } else if let Some(scope) = key.strip_suffix(":_auth") {
            (scope, "Basic")
        } else {
            continue;
        };
        let Some(scope) = normalize_scope(scope) else {
            continue;
        };
        if !target.starts_with(&scope) {
            continue;
        }
        let Some(secret) = expand_env(value, &env).filter(|secret| !secret.is_empty()) else {
            continue;
        };
        // Later lines override earlier ones for the same scope, as in npm.
        if best
            .as_ref()
            .is_none_or(|(selected_scope, _)| scope.len() >= selected_scope.len())
        {
            best = Some((scope, format!("{scheme} {secret}")));
        }
    }
    best.map(|(scope, header)| NpmAuthorization {
        header: SecretString::from(header),
        scope,
    })
}

#[cfg(test)]
mod tests;
