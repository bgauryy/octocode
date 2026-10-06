//! System commands: authentication and cache maintenance.
//!
//! These wrap [`ToolRuntime`] to surface credential state and login/logout —
//! the environment-management side of the CLI, distinct from tool dispatch in
//! the parent module.
use super::write_json;
use octocode_native::runtime::ToolRuntime;
use serde_json::json;

/// Use the same credential hostname for status, login, and logout.
fn configured_github_host(runtime: &ToolRuntime) -> String {
    runtime
        .config()
        .resolved
        .github
        .api_url
        .parse::<url::Url>()
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|host| octocode_native::providers::github::credential_host(host).to_owned())
        })
        .unwrap_or_else(|| "github.com".into())
}

async fn resolve_auth(
    runtime: &ToolRuntime,
    host: &str,
) -> Option<octocode_native::providers::github::AuthSelection> {
    use octocode_native::providers::github::{AuthMode, Authentication, RequestContext};
    let auth = Authentication::new(std::sync::Arc::new(runtime.config().clone()));
    let budget = RequestContext::with_timeout(std::time::Duration::from_secs(5), 0);
    auth.resolve(host, AuthMode::Inspect, &budget)
        .await
        .ok()
        .flatten()
}

pub async fn auth_status(runtime: &ToolRuntime, json_out: bool) -> u8 {
    use octocode_native::providers::github::login::{TokenCheck, verify_token};
    let api_base = &runtime.config().resolved.github.api_url;
    let hostname = configured_github_host(runtime);
    let selection = resolve_auth(runtime, &hostname).await;
    let token_present = selection.is_some();
    let source = selection.as_ref().map_or("none", |s| s.source_label());
    // A present token proves nothing: GitHub must accept it. A rejected token
    // is not authenticated; one GitHub could not be asked about is unverified.
    let check = match selection.as_ref() {
        Some(selected) => Some(
            verify_token(
                api_base,
                selected.token(),
                std::time::Duration::from_secs(5),
            )
            .await,
        ),
        None => None,
    };
    let (authenticated, verification, verified_login) = match check {
        None => (false, "none", None),
        Some(TokenCheck::Valid(login)) => (true, "verified", login),
        Some(TokenCheck::Unverified) => (true, "unverified", None),
        Some(TokenCheck::Rejected) => (false, "invalid", None),
    };
    let username = verified_login.or_else(|| {
        authenticated
            .then(|| selection.as_ref().and_then(|s| s.username.clone()))
            .flatten()
    });
    if json_out {
        return write_json(
            &json!({
                "success": true,
                "authenticated": authenticated,
                "verification": verification,
                "username": username,
                "hostname": hostname,
                "tokenPresent": token_present,
                "tokenConfigured": token_present,
                "tokenSource": source,
                "publicGitHubAccess": if authenticated { "authenticated" } else { "unauthenticated" }
            }),
            true,
        );
    }
    match verification {
        "verified" | "unverified" => {
            let note = if verification == "unverified" {
                "; unverified: GitHub could not be reached"
            } else {
                ""
            };
            match &username {
                Some(user) => println!("authenticated as {user} (source: {source}{note})"),
                None => println!("authenticated (source: {source}{note})"),
            }
            0
        }
        "invalid" => {
            eprintln!("invalid token (source: {source}): GitHub rejected it (HTTP 401)");
            eprintln!(
                "Run `octocode auth login`, or fix or unset the token; an invalid env token overrides stored login."
            );
            1
        }
        _ => {
            eprintln!("unauthenticated");
            eprintln!("Run `octocode auth login` or set GITHUB_TOKEN / GH_TOKEN.");
            1
        }
    }
}

pub async fn login(
    runtime: &ToolRuntime,
    hostname: Option<&str>,
    force: bool,
    refresh: bool,
    json_out: bool,
) -> u8 {
    // `--hostname` overrides the configured GitHub host for enterprise device
    // login; otherwise fall back to the host derived from `github.apiUrl`.
    let host = hostname
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| configured_github_host(runtime));
    let client_id = octocode_native::providers::github::login::client_id_for_host(
        &host,
        runtime.config().env_value("OCTOCODE_GITHUB_CLIENT_ID"),
    );
    if client_id.is_empty() {
        let message =
            "OCTOCODE_GITHUB_CLIENT_ID is required for GitHub Enterprise device login and refresh.";
        if json_out {
            return write_json(&json!({ "success": false, "error": message }), true).max(1);
        }
        eprintln!("{message}");
        return 1;
    }

    let credential_store =
        octocode_native::providers::github::CredentialStore::new(&runtime.config().home);

    // Refresh path: exchange the stored refresh token; no device flow, no TTY.
    if refresh {
        let result = octocode_native::providers::github::login::refresh_auth_token_result_in_store(
            &host,
            client_id,
            &credential_store,
        )
        .await;
        if result.success {
            let host_label = result.hostname.as_deref().unwrap_or("github.com");
            if json_out {
                return write_json(
                    &json!({
                        "success": true,
                        "action": "refresh",
                        "hostname": host_label,
                        "username": result.username,
                    }),
                    true,
                );
            }
            eprintln!("Refreshed credentials for {host_label}");
            return 0;
        }
        let error = result
            .error
            .as_deref()
            .unwrap_or("credential.refreshFailed");
        if json_out {
            return write_json(&json!({ "success": false, "error": error }), true).max(1);
        }
        eprintln!("{error}");
        eprintln!("Run `octocode auth login`, or set GITHUB_TOKEN / GH_TOKEN.");
        return 1;
    }

    // Already-authenticated short-circuit: a stored OAuth credential for this
    // host is left in place unless `--force` re-authenticates.
    let stored = credential_store
        .load(&host)
        .ok()
        .flatten()
        .map(|(stored, _)| stored);
    if let Some(existing) = stored.as_ref().filter(|stored| {
        !force && !octocode_native::providers::github::login::is_token_expired(stored)
    }) {
        let user = if existing.username.is_empty() {
            host.clone()
        } else {
            existing.username.clone()
        };
        if json_out {
            return write_json(
                &json!({
                    "success": true,
                    "action": "none",
                    "alreadyAuthenticated": true,
                    "hostname": host,
                    "username": existing.username,
                }),
                true,
            );
        }
        println!("Already authenticated as {user} on {host}. Use `--force` to switch accounts.");
        return 0;
    }

    // Warn when an environment token is set: it takes priority over the stored
    // OAuth credential this flow writes, so the new login won't be used until
    // the variable is unset. Non-fatal. The variable name comes from the same
    // resolved selection the request path uses (`config.token`, source
    // `env:<VAR>`) so it names the token that would actually win.
    let env_token_var = runtime
        .config()
        .token
        .as_ref()
        .map(octocode_native::config::PrivateTokenSelection::source)
        .and_then(|source| source.strip_prefix("env:"));
    if let Some(var) = env_token_var
        && !json_out
    {
        eprintln!("⚠ {var} is set and takes priority over stored credentials until you unset it.");
    }

    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        let message = "login requires an interactive terminal, or set GITHUB_TOKEN / GH_TOKEN.";
        if json_out {
            return write_json(&json!({ "success": false, "error": message }), true).max(1);
        }
        eprintln!("{message}");
        return 1;
    }

    // Preserve the old login until the replacement has been authenticated and saved.
    let endpoints = octocode_native::providers::github::login::LoginEndpoints::from_host(&host);
    match octocode_native::providers::github::login::login_device_flow_in_store(
        &endpoints,
        client_id,
        &tokio_util::sync::CancellationToken::new(),
        &credential_store,
    )
    .await
    {
        Ok(stored) => {
            if json_out {
                return write_json(
                    &json!({
                        "success": true,
                        "action": "login",
                        "hostname": stored.hostname,
                        "username": stored.username,
                    }),
                    true,
                );
            }
            eprintln!(
                "Authenticated as {} on {}",
                stored.username, stored.hostname
            );
            0
        }
        Err(error) => {
            if json_out {
                return write_json(&json!({ "success": false, "error": error.message }), true)
                    .max(1);
            }
            eprintln!("{}", error.message);
            eprintln!("Run `octocode auth login`, or set GITHUB_TOKEN / GH_TOKEN.");
            1
        }
    }
}

pub fn logout(runtime: &ToolRuntime) -> u8 {
    let store = octocode_native::providers::github::CredentialStore::new(&runtime.config().home);
    logout_with(runtime, &|host| store.delete(host))
}

fn logout_with(
    runtime: &ToolRuntime,
    delete: &dyn Fn(&str) -> Result<(), octocode_native::providers::github::ProviderError>,
) -> u8 {
    let view = runtime.inspect_config();
    // Same host derivation as auth status / login so all three target the
    // identical credential-store host (api.github.com → github.com).
    let host = configured_github_host(runtime);
    match delete(&host) {
        Ok(()) => {
            eprintln!(
                "Removed Octocode credentials for {host} from {} and the OS store. Environment and gh credentials are unchanged.",
                view.home.display()
            );
            0
        }
        Err(error) => {
            eprintln!("{}", error.message);
            1
        }
    }
}

pub fn cache(runtime: &ToolRuntime, action: &str) -> u8 {
    match action {
        "status" => {
            let view = runtime.inspect_config();
            println!("cache home: {}", view.home.join("tmp").display());
            let (recent, log_bytes) =
                octocode_native::cache::evictions::recent_evictions(&view.home, 5);
            if recent.is_empty() {
                println!("recent evictions: none recorded");
            } else {
                println!(
                    "recent evictions (last {}, log {} bytes):",
                    recent.len(),
                    log_bytes
                );
                for line in recent {
                    println!("  {line}");
                }
            }
            0
        }
        "clear" => {
            runtime.clear_github_cache();
            println!("cleared GitHub content cache");
            0
        }
        other => {
            eprintln!("Usage: octocode cache <status|clear>");
            let _ = other;
            2
        }
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;
    use octocode_native::providers::github::{ProviderError, ProviderErrorKind};
    use octocode_native::runtime::HostOptions;
    use std::{collections::BTreeMap, sync::Mutex};

    #[test]
    fn logout_targets_configured_host_preserves_environment_and_reports_delete_failure() {
        for (api, expected_host) in [
            ("https://api.github.com", "github.com"),
            ("https://enterprise.example/api/v3", "enterprise.example"),
        ] {
            let dir = tempfile::tempdir().expect("fixture home");
            let runtime = ToolRuntime::from_host(HostOptions {
                cwd: Some(dir.path().into()),
                env: Some(BTreeMap::from([
                    (
                        "OCTOCODE_HOME".into(),
                        dir.path().to_string_lossy().into_owned(),
                    ),
                    ("GITHUB_API_URL".into(), api.into()),
                    ("GH_TOKEN".into(), "synthetic-env-token".into()),
                    ("OCTOCODE_ENABLE_STATS".into(), "false".into()),
                ])),
                ..HostOptions::default()
            })
            .expect("fixture runtime");
            let calls = Mutex::new(Vec::new());
            let delete = |host: &str| {
                calls.lock().expect("calls").push(host.to_owned());
                Ok(())
            };
            assert_eq!(logout_with(&runtime, &delete), 0);
            assert_eq!(*calls.lock().expect("calls"), [expected_host]);
            assert_eq!(
                runtime.config().env_value("GH_TOKEN"),
                Some("synthetic-env-token")
            );
            let failure = |_: &str| {
                Err(ProviderError::new(
                    ProviderErrorKind::CredentialStoreUnavailable,
                    "fixture delete failed",
                ))
            };
            assert_eq!(logout_with(&runtime, &failure), 1);
        }
    }
}
