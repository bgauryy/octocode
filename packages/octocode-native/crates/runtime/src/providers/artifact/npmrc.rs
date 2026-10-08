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
pub(crate) fn npm_authorization(
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

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name == "localhost" || name.ends_with(".localhost")
        }
        None => false,
    }
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
    if registry.scheme() != "https" && !is_loopback(registry) {
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
mod tests {
    use super::authorization_for;
    use secrecy::ExposeSecret;
    use url::Url;

    fn auth(registry: &str, npmrc: &str) -> Option<String> {
        let env = |name: &str| (name == "NPM_TOKEN").then(|| "from-env".to_owned());
        authorization_for(&Url::parse(registry).unwrap(), npmrc, env)
            .map(|secret| secret.header.expose_secret().to_owned())
    }

    #[test]
    fn token_is_sent_only_to_its_scoped_origin() {
        let npmrc = "//npm.example.com/:_authToken=abc\n";
        assert_eq!(
            auth("https://npm.example.com/", npmrc).as_deref(),
            Some("Bearer abc")
        );
        assert_eq!(
            auth("https://npm.example.com/sub/path", npmrc).as_deref(),
            Some("Bearer abc")
        );
        // Different host, suffix host, or port never receives the token.
        for other in [
            "https://evil.example/",
            "https://npm.example.com.evil.example/",
            "https://npm.example.com:8443/",
            "https://registry.npmjs.org/",
        ] {
            assert_eq!(auth(other, npmrc), None, "{other}");
        }
    }

    /// npmrc keys carry no scheme, so a query naming `http://` for the same
    /// host must not receive the token in clear text. Only loopback (a local
    /// Verdaccio) may use plain HTTP.
    #[test]
    fn token_is_never_sent_over_plain_http_off_loopback() {
        let npmrc = "//npm.corp.example/:_authToken=abc\n//localhost:4873/:_authToken=local\n//[::1]:4873/:_authToken=v6\n";
        assert_eq!(auth("http://npm.corp.example/", npmrc), None);
        assert_eq!(
            auth("https://npm.corp.example/", npmrc).as_deref(),
            Some("Bearer abc")
        );
        assert_eq!(
            auth("http://localhost:4873/", npmrc).as_deref(),
            Some("Bearer local")
        );
        assert_eq!(
            auth("http://[::1]:4873/", npmrc).as_deref(),
            Some("Bearer v6")
        );
    }

    #[test]
    fn path_scoped_tokens_prefer_the_longest_match() {
        let npmrc = "//h.test/:_authToken=root\n//h.test/api/npm/:_authToken=deep\n";
        assert_eq!(
            auth("https://h.test/api/npm/", npmrc).as_deref(),
            Some("Bearer deep")
        );
        assert_eq!(
            auth("https://h.test/other/", npmrc).as_deref(),
            Some("Bearer root")
        );
        let only_deep = "//h.test/api/npm/:_authToken=deep\n";
        assert_eq!(auth("https://h.test/", only_deep), None);
        assert_eq!(auth("https://h.test/api/npmx/", only_deep), None);
    }

    #[test]
    fn resolved_credentials_retain_the_winning_normalized_scope() {
        let base = Url::parse("https://h.test/api/npm/").unwrap();
        let authorization = authorization_for(
            &base,
            "//h.test/:_authToken=root\n//H.TEST/api/npm:_authToken=old\n//h.test/api/npm/:_auth=ZmFrZQ==\n",
            |_| None,
        ).expect("scoped credential");
        let registry = super::super::types::ResolvedNpmRegistry {
            base,
            authorization: Some(authorization),
            cache_identity: "fake-test".into(),
        };
        assert_eq!(
            registry.authorization.as_ref().unwrap().scope,
            "//h.test/api/npm/"
        );
        for target in [
            "https://h.test/api/npm/pkg",
            "https://h.test:443/api/npm/pkg",
        ] {
            assert_eq!(
                registry
                    .authorization_for(&Url::parse(target).unwrap())
                    .unwrap()
                    .expose_secret(),
                "Basic ZmFrZQ=="
            );
        }
        for target in [
            "https://h.test/other/pkg",
            "https://h.test/api/npm-extra/pkg",
            "https://h.test:444/api/npm/pkg",
            "http://h.test/api/npm/pkg",
            "https://fake:secret@h.test/api/npm/pkg",
        ] {
            assert!(
                registry
                    .authorization_for(&Url::parse(target).unwrap())
                    .is_none(),
                "{target}"
            );
        }
        assert!(!format!("{registry:?}").contains("ZmFrZQ=="));
    }

    #[test]
    fn env_expansion_basic_auth_ports_and_comments() {
        let npmrc = "# c\n; c\nregistry=https://evil.example/\n\
                     //127.0.0.1:4873/:_authToken=${NPM_TOKEN}\n\
                     //basic.test/:_auth=\"dXNlcjpwYXNz\"\n\
                     //missing.test/:_authToken=${UNSET_VAR}\n";
        assert_eq!(
            auth("http://127.0.0.1:4873", npmrc).as_deref(),
            Some("Bearer from-env")
        );
        assert_eq!(
            auth("https://basic.test/", npmrc).as_deref(),
            Some("Basic dXNlcjpwYXNz")
        );
        assert_eq!(auth("https://missing.test/", npmrc), None);
        // A bare `registry=` line never attaches credentials anywhere.
        assert_eq!(auth("https://evil.example/", npmrc), None);
    }
}
