//! System commands: authentication, cache maintenance, and skill delegation.
//!
//! These wrap [`ToolRuntime`] to surface credential state and login/logout —
//! the environment-management side of the CLI, distinct from tool dispatch in
//! the parent module.
use super::write_json;
use octocode_native::runtime::ToolRuntime;
use serde_json::json;

/// Resolve authentication: checks environment variables, the OS credential
/// store, and then the GitHub CLI.
/// Returns `(authenticated, username, source, raw_token)`.
/// - `username` is populated natively only for the platform-keychain source.
/// - `raw_token` is the credential secret when we can expose it (env / file / gh-cli);
///   callers may use it for a live GH API call to resolve `username`.
fn configured_github_host(runtime: &ToolRuntime) -> String {
    runtime
        .config()
        .resolved
        .github
        .api_url
        .parse::<url::Url>()
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .map(|host| {
            if host == "api.github.com" {
                "github.com".to_owned()
            } else {
                host
            }
        })
        .unwrap_or_else(|| "github.com".into())
}

fn oauth_client_id<'a>(runtime: &'a ToolRuntime, host: &str) -> Option<&'a str> {
    runtime
        .config()
        .env_value("OCTOCODE_GITHUB_CLIENT_ID")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or((host == "github.com")
            .then_some(octocode_native::providers::github::login::GITHUB_APP_CLIENT_ID))
}

fn resolve_auth(
    runtime: &ToolRuntime,
    host: &str,
) -> (bool, Option<String>, &'static str, Option<String>) {
    use octocode_native::providers::github::{
        CredentialSourceProvider, GhCliCredentialSource, PlatformCredentialStore,
        load_stored_credentials,
    };
    use secrecy::ExposeSecret;
    // 1. Environment token — reuse the single selection the request path
    // resolves (`config.token` = `resolve_env_token` over the contract-generated
    // ENV_TOKEN_VARS, already trimmed and priority-ordered). Sharing it keeps
    // this diagnostic and the actual credential used on requests from drifting.
    if let Some(selection) = runtime.config().token.as_ref() {
        return (true, None, "env", Some(selection.token().to_owned()));
    }
    // 2. OS platform keychain — username comes from keychain metadata directly
    if matches!(PlatformCredentialStore.load_blocking(host), Ok(Some(_))) {
        let username = load_stored_credentials(host)
            .ok()
            .flatten()
            .map(|c| c.username)
            .filter(|u| !u.is_empty());
        return (true, username, "platform", None);
    }
    // 3. gh CLI token
    if let Ok(Some(secret)) = GhCliCredentialSource.load_blocking(host) {
        let token = secret.expose_secret().to_owned();
        return (true, None, "gh-cli", Some(token));
    }
    (false, None, "none", None)
}

/// Call `GET /user` on the GitHub API with the given token and return the
/// `login` field. Goes through the shared GitHub executor (throttling and
/// rate-limit state); times out after 5 s and returns `None` on any error.
async fn fetch_github_username(token: &str, api_base: &str) -> Option<String> {
    octocode_native::providers::github::login::fetch_authenticated_login(
        api_base,
        token,
        std::time::Duration::from_secs(5),
    )
    .await
}

pub async fn auth_status(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let api_base = &runtime.config().resolved.github.api_url;
    let hostname = configured_github_host(runtime);
    let (authenticated, username, source, token) = resolve_auth(runtime, &hostname);
    // Resolve username via GH API when the credential source doesn't carry it
    let username = if authenticated && username.is_none() {
        match &token {
            Some(tok) => fetch_github_username(tok, api_base).await,
            None => None,
        }
    } else {
        username
    };
    if json_out {
        return write_json(
            &json!({
                "success": true,
                "authenticated": authenticated,
                "username": username,
                "hostname": hostname,
                "tokenPresent": authenticated,
                "tokenConfigured": authenticated,
                "tokenSource": source,
                "publicGitHubAccess": if authenticated { "authenticated" } else { "unauthenticated" }
            }),
            true,
        );
    }
    if authenticated {
        if let Some(user) = &username {
            println!("authenticated as {user} (source: {source})");
        } else {
            println!("authenticated (source: {source})");
        }
        0
    } else {
        eprintln!("unauthenticated");
        eprintln!("Run `octocode auth login` or set GITHUB_TOKEN / GH_TOKEN.");
        1
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
    let client_id = oauth_client_id(runtime, &host);
    if host != "github.com" && client_id.is_none() {
        let message =
            "OCTOCODE_GITHUB_CLIENT_ID is required for GitHub Enterprise device login and refresh.";
        if json_out {
            return write_json(&json!({ "success": false, "error": message }), true);
        }
        eprintln!("{message}");
        return 1;
    }

    // Refresh path: exchange the stored refresh token; no device flow, no TTY.
    if refresh {
        let result = octocode_native::providers::github::login::refresh_auth_token_result(
            Some(&host),
            client_id,
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
            return write_json(&json!({ "success": false, "error": error }), true);
        }
        eprintln!("{error}");
        eprintln!("Run `octocode auth login`, or set GITHUB_TOKEN / GH_TOKEN.");
        return 1;
    }

    // Already-authenticated short-circuit: a stored OAuth credential for this
    // host is left in place unless `--force` re-authenticates.
    let stored = octocode_native::providers::github::load_stored_credentials(&host)
        .ok()
        .flatten();
    if let Some(existing) = stored.as_ref().filter(|_| !force) {
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
            return write_json(&json!({ "success": false, "error": message }), true);
        }
        eprintln!("{message}");
        return 1;
    }

    // `--force`: remove the stored credential before re-authenticating.
    if force
        && stored.is_some()
        && let Err(error) = octocode_native::providers::github::delete_platform_credential(&host)
    {
        if json_out {
            return write_json(&json!({ "success": false, "error": error.message }), true);
        }
        eprintln!("{}", error.message);
        return 1;
    }

    let endpoints = octocode_native::providers::github::login::LoginEndpoints::from_host(&host);
    // The guard above returns before this point unless a client ID was resolved.
    #[allow(clippy::expect_used)]
    let client_id = client_id.expect("public GitHub or validated enterprise client ID");
    match octocode_native::providers::github::login::login_device_flow_with_client_id(
        &endpoints, client_id,
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
                return write_json(&json!({ "success": false, "error": error.message }), true);
            }
            eprintln!("{}", error.message);
            eprintln!("Run `octocode auth login`, or set GITHUB_TOKEN / GH_TOKEN.");
            1
        }
    }
}

pub fn logout(runtime: &ToolRuntime) -> u8 {
    let view = runtime.inspect_config();
    // Same host derivation as auth status / login so all three target the
    // identical credential-store host (api.github.com → github.com).
    let host = configured_github_host(runtime);
    match octocode_native::providers::github::delete_platform_credential(&host) {
        Ok(()) => {
            eprintln!(
                "Removed native keychain credentials for {host}. Environment tokens are unchanged. Stored files under {} were not printed.",
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

pub fn skill(args: &[String]) -> u8 {
    // Skill materialization lives in the npm CLI. If the `octocode` on PATH is
    // this native binary instead of the npm launcher, spawning would recurse
    // forever — the guard variable breaks that loop with a clear error.
    if std::env::var_os("OCTOCODE_SKILL_DELEGATED").is_some() {
        eprintln!("octocode skill: the `octocode` on PATH is the native binary, not the npm CLI.");
        eprintln!("Install the npm CLI (npm i -g octocode) or run: npx -y octocode skill …");
        return 1;
    }
    let mut command = std::process::Command::new("octocode");
    command
        .arg("skill")
        .args(args)
        .env("OCTOCODE_SKILL_DELEGATED", "1");
    match command.status() {
        Ok(status) => status.code().unwrap_or(1) as u8,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let rest = if args.is_empty() {
                String::new()
            } else {
                format!(" {}", args.join(" "))
            };
            eprintln!("octocode skill requires the Node CLI (`octocode`) on PATH.");
            eprintln!("Install: npm i -g octocode");
            eprintln!("Then:    octocode skill{rest}");
            eprintln!("Or:      npx -y octocode skill{rest}");
            1
        }
        Err(error) => {
            eprintln!("failed to spawn octocode skill: {error}");
            1
        }
    }
}
