//! System commands: authentication, status reporting, and cache management.
//!
//! These wrap [`ToolRuntime`] to surface credential state, MCP-client
//! detection, cache sizing, and login/logout — the environment-management
//! side of the CLI, distinct from the tool-dispatch command wrappers in the
//! parent module. The public entry points (`status`, `auth_status`, `login`,
//! `logout`, `cache`) are re-exported by the parent so existing `human::*`
//! call sites keep resolving.
use super::super::{mcp_sync, write_json};
use octocode_native::runtime::ToolRuntime;
use serde_json::json;
use std::path::Path;

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
    // 1. Environment variables (fast, no I/O)
    for key in [
        "OCTOCODE_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
    ] {
        if let Some(v) = runtime.config().env_value(key).filter(|v| !v.is_empty()) {
            return (true, None, "env", Some(v.to_owned()));
        }
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
/// `login` field. Times out after 5 s and silently returns `None` on any error.
async fn fetch_github_username(token: &str, api_base: &str) -> Option<String> {
    let url = format!("{}/user", api_base.trim_end_matches('/'));
    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        reqwest::Client::new()
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header(
                "User-Agent",
                concat!("octocode-native/", env!("CARGO_PKG_VERSION")),
            )
            .header("Accept", "application/vnd.github.v3+json")
            .send(),
    )
    .await
    .ok()?
    .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    json.get("login")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// Returns total bytes in a directory tree (best-effort; skips unreadable entries).
fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let meta = e.metadata().ok();
            if meta.as_ref().is_some_and(|m| m.is_dir()) {
                dir_size_bytes(&e.path())
            } else {
                meta.map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

/// All known MCP client IDs and their config format.
const ALL_IDES: &[(&str, &str)] = &[
    ("cursor", "json"),
    ("claude-desktop", "json"),
    ("claude-code", "json"),
    ("windsurf", "json"),
    ("vscode-cline", "json"),
    ("vscode-roo", "json"),
    ("vscode-continue", "json"),
    ("zed", "json"),
    ("opencode", "json"),
    ("gemini-cli", "json"),
    ("kiro", "json"),
    ("trae", "json"),
    ("antigravity", "json"),
    ("codex", "toml"),
    ("goose", "yaml"),
];

/// Detect which IDEs have an Octocode MCP entry in their config.
/// Returns ALL known IDEs — configured:true only when the config file exists
/// and contains an octocode entry.
fn detect_mcp_clients(_home: &Path) -> Vec<(&'static str, bool)> {
    use super::super::mcp_install::config_path;
    ALL_IDES
        .iter()
        .map(|(ide, fmt)| {
            let configured = config_path(ide)
                .and_then(|path| std::fs::read_to_string(&path).ok())
                .map(|content| match *fmt {
                    "toml" => {
                        content.contains("[mcpServers.octocode]") || content.contains("octocode")
                    }
                    "yaml" => content.contains("octocode:") || content.contains("- octocode"),
                    _ => serde_json::from_str::<serde_json::Value>(&content)
                        .ok()
                        .and_then(|v| {
                            v["mcpServers"]
                                .as_object()
                                .map(|s| s.contains_key("octocode"))
                        })
                        .unwrap_or(false),
                })
                .unwrap_or(false);
            (*ide, configured)
        })
        .collect()
}

/// Format bytes as a human-readable size string.
fn human_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1_048_576 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

pub async fn status(
    runtime: &ToolRuntime,
    hostname: Option<&str>,
    json_out: bool,
    sync: bool,
) -> u8 {
    let view = runtime.inspect_config();
    let catalog = runtime.catalog().ok();
    let available = catalog
        .as_ref()
        .and_then(|value| value["tools"].as_array())
        .map(|tools| {
            tools
                .iter()
                .filter(|tool| tool["available"] == true)
                .filter_map(|tool| tool["name"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let hostname = hostname
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| configured_github_host(runtime));
    let (auth, username, source, _token) = resolve_auth(runtime, &hostname);
    // MCP client detection (all 15 IDEs)
    let mcp_clients = detect_mcp_clients(&view.home);
    let configured_count = mcp_clients.iter().filter(|(_, has)| *has).count();
    // Cache size — sub-directory breakdown
    let cache_root = view.home.join("tmp");
    let cache_dirs = ["tmp", "clone", "tree", "response"];
    let mut cache_totals: Vec<(_, u64)> = cache_dirs
        .iter()
        .map(|d| (*d, dir_size_bytes(&view.home.join(d))))
        .collect();
    // legacy: everything in tmp/ is counted under "tmp" already
    let total_cache: u64 = cache_totals.iter().map(|(_, b)| b).sum();
    // keep tmp as the raw dir_size (may overlap); give clone/tree/response their own paths
    cache_totals[0] = ("tmp", dir_size_bytes(&cache_root));
    let sync_analysis = if sync {
        let snapshots = mcp_sync::read_all_client_configs();
        Some(mcp_sync::analyze(&snapshots))
    } else {
        None
    };
    if json_out {
        let mut payload = json!({
            "home": view.home,
            "storage": view.storage_mode,
            "auth": {
                "authenticated": auth,
                "username": username,
                "hostname": hostname,
                "tokenPresent": auth,
                "tokenSource": source,
            },
            "config": {
                "source": "file",
                "storageMode": view.storage_mode,
            },
            "availableTools": available,
            "mcpClients": mcp_clients.iter().map(|(ide, has)| json!({
                "client": ide,
                "octocodeInstalled": has
            })).collect::<Vec<_>>(),
            "cache": {
                "totalBytes": total_cache,
                "details": {
                    "tmp": cache_totals[0].1,
                    "clone": cache_totals[1].1,
                    "tree": cache_totals[2].1,
                    "response": cache_totals[3].1,
                }
            }
        });
        if let Some(ref analysis) = sync_analysis {
            payload["sync"] = mcp_sync::sync_json(analysis);
        }
        return write_json(&payload, true);
    }
    // Human output
    let auth_line = match (auth, &username) {
        (true, Some(user)) => format!("authenticated as {user} (source: {source})"),
        (true, None) => format!("authenticated (source: {source})"),
        (false, _) => "unauthenticated".into(),
    };
    println!("home:    {}", view.home.display());
    println!("storage: {}", view.storage_mode);
    println!("auth:    {auth_line}");
    println!();
    println!(
        "MCP Clients  ({configured_count}/{} configured)",
        mcp_clients.len()
    );
    for (ide, has) in &mcp_clients {
        println!("  {} {ide}", if *has { "✓" } else { "○" });
    }
    println!();
    println!("Cache  {} total", human_bytes(total_cache));
    for (label, bytes) in &cache_totals {
        println!("  {label:<10} {}", human_bytes(*bytes));
    }
    println!();
    println!("Tools  ({} enabled):", available.len());
    println!("  {}", available.join(" "));
    if let Some(ref analysis) = sync_analysis {
        println!();
        println!(
            "Sync  ({} unique MCPs across {} clients)",
            analysis.summary.total_unique_mcps, analysis.summary.clients_with_config
        );
        if analysis.summary.consistent > 0 {
            println!("  ✓ {} fully synced", analysis.summary.consistent);
        }
        if analysis.summary.needs_sync > 0 {
            println!(
                "  ○ {} missing in some configs",
                analysis.summary.needs_sync
            );
        }
        if analysis.summary.conflicts > 0 {
            println!(
                "  ! {} conflicts across MCP configs",
                analysis.summary.conflicts
            );
        }
    }
    0
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
        eprintln!("Run `octocode login` or set GITHUB_TOKEN / GH_TOKEN.");
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
        eprintln!("Run `octocode login`, or set GITHUB_TOKEN / GH_TOKEN.");
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
    // the variable is unset. Non-fatal.
    let env_token_var = [
        "OCTOCODE_TOKEN",
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
    ]
    .into_iter()
    .find(|key| {
        runtime
            .config()
            .env_value(key)
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
    });
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
    match octocode_native::providers::github::login::login_device_flow_with_client_id(
        &endpoints,
        client_id.expect("public GitHub or validated enterprise client ID"),
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
            eprintln!("Authenticated as {} on {}", stored.username, stored.hostname);
            0
        }
        Err(error) => {
            if json_out {
                return write_json(&json!({ "success": false, "error": error.message }), true);
            }
            eprintln!("{}", error.message);
            eprintln!("Run `octocode login`, or set GITHUB_TOKEN / GH_TOKEN.");
            1
        }
    }
}

pub fn logout(runtime: &ToolRuntime) -> u8 {
    let view = runtime.inspect_config();
    let host = runtime
        .config()
        .resolved
        .github
        .api_url
        .parse::<url::Url>()
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| "github.com".into());
    let host = if host == "api.github.com" {
        "github.com".into()
    } else {
        host
    };
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
