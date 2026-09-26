//! Origin-scoped npm registry credentials from the user npmrc.
//!
//! Only the user config (`NPM_CONFIG_USERCONFIG`, else `~/.npmrc`) is read.
//! A project `.npmrc` is never consulted: it is repository-controlled and
//! must not be able to steer the user's token anywhere. The registry always
//! comes from the query, and a token is attached only when its
//! `//host[:port]/path/:_authToken` key is scoped to that registry's origin
//! (npm "nerf-dart" matching, longest path prefix wins).
use secrecy::SecretString;
use std::path::{Path, PathBuf};
use url::Url;

/// Path of the user npmrc, following npm's `userconfig` resolution.
pub(crate) fn user_npmrc_path() -> Option<PathBuf> {
    ["NPM_CONFIG_USERCONFIG", "npm_config_userconfig"]
        .iter()
        .find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".npmrc")))
}

/// Authorization header value for `registry` from the npmrc at `path`.
pub(crate) fn authorization_from_file(registry: &Url, path: &Path) -> Option<SecretString> {
    // npmrc files are tiny; refuse anything implausibly large.
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return None;
    }
    let contents = std::fs::read_to_string(path).ok()?;
    authorization_for(registry, &contents, |name| std::env::var(name).ok())
}

/// `//host[:port]/path/` for a registry URL, as npm keys credentials.
fn nerf_dart(registry: &Url) -> Option<String> {
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
) -> Option<SecretString> {
    let target = nerf_dart(registry)?;
    // (scope length, header) of the most specific matching credential.
    let mut best: Option<(usize, String)> = None;
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
        if best.as_ref().is_none_or(|(len, _)| scope.len() >= *len) {
            best = Some((scope.len(), format!("{scheme} {secret}")));
        }
    }
    best.map(|(_, header)| SecretString::from(header))
}

#[cfg(test)]
mod tests {
    use super::authorization_for;
    use secrecy::ExposeSecret;
    use url::Url;

    fn auth(registry: &str, npmrc: &str) -> Option<String> {
        let env = |name: &str| (name == "NPM_TOKEN").then(|| "from-env".to_owned());
        authorization_for(&Url::parse(registry).unwrap(), npmrc, env)
            .map(|secret| secret.expose_secret().to_owned())
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
