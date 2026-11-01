//! System commands: authentication and cache maintenance.
//!
//! These wrap [`ToolRuntime`] to surface credential state and login/logout —
//! the environment-management side of the CLI, distinct from tool dispatch in
//! the parent module.
use super::write_json;
use octocode_contract_types::ErrorCode;
use octocode_native::providers::github::StoredCredentials;
use octocode_native::response::exit::ExitClass;
use octocode_native::runtime::{
    AuthStatus, FailureKind, LoginPlan, LoginTarget, ToolRuntime, Verification,
};
use serde_json::json;

/// A failed auth command: the one `{error, errorCode}` envelope (JSON) or
/// the message and its next step (text), exiting by the code's class.
fn auth_failure(code: ErrorCode, message: &str, next_step: Option<&str>, json_out: bool) -> u8 {
    super::emit_error(code, message, json_out);
    if let Some(next_step) = next_step.filter(|_| !json_out) {
        eprintln!("{next_step}");
    }
    super::exit_code(ExitClass::of(code))
}

const LOGIN_NEXT_STEP: &str = "Run `octocode auth login`, or set GITHUB_TOKEN / GH_TOKEN.";

pub async fn auth_status(runtime: &ToolRuntime, json_out: bool) -> u8 {
    let status = runtime.auth_status().await;
    let AuthStatus {
        host,
        source,
        verification,
        username,
    } = &status;
    // Not authenticated is the documented auth exit (4), whatever the mode.
    let unauthenticated = super::exit_code(ExitClass::Failed(FailureKind::Authentication));
    if json_out {
        let written = write_json(&json!({
            "authenticated": verification.authenticated(),
            "verification": verification.as_str(),
            "username": username,
            "hostname": host,
            "tokenSource": source,
        }));
        return match written {
            0 if verification.authenticated() => 0,
            0 => unauthenticated,
            failed => failed,
        };
    }
    match verification {
        Verification::Verified | Verification::Unverified => {
            let note = if *verification == Verification::Unverified {
                "; unverified: GitHub could not be reached"
            } else {
                ""
            };
            match username {
                Some(user) => println!("authenticated as {user} (source: {source}{note})"),
                None => println!("authenticated (source: {source}{note})"),
            }
            0
        }
        Verification::Invalid => {
            eprintln!("invalid token (source: {source}): GitHub rejected it (HTTP 401)");
            eprintln!(
                "Run `octocode auth login`, or fix or unset the token; an invalid env token overrides stored login."
            );
            unauthenticated
        }
        Verification::None => {
            eprintln!("unauthenticated");
            eprintln!("{LOGIN_NEXT_STEP}");
            unauthenticated
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
    // login; otherwise the host derived from `github.apiUrl`.
    let target = match runtime.login_target(hostname) {
        Ok(target) => target,
        Err((code, message)) => return auth_failure(code, &message, None, json_out),
    };
    match runtime.login_plan(&target, force, refresh) {
        LoginPlan::Refresh => refresh_login(runtime, &target, json_out).await,
        LoginPlan::AlreadyAuthenticated(existing) => {
            already_authenticated(&existing, &target.host, json_out)
        }
        LoginPlan::DeviceFlow { env_override } => {
            // Non-fatal: the environment token keeps winning until it is unset.
            if let Some(var) = env_override.filter(|_| !json_out) {
                eprintln!(
                    "⚠ {var} is set and takes priority over stored credentials until you unset it."
                );
            }
            use std::io::IsTerminal;
            if !std::io::stdin().is_terminal() {
                let message =
                    "login requires an interactive terminal, or set GITHUB_TOKEN / GH_TOKEN.";
                return auth_failure(ErrorCode::InvalidInput, message, None, json_out);
            }
            device_login(runtime, &target, json_out).await
        }
    }
}

/// `auth login --refresh`: exchange the stored refresh token.
async fn refresh_login(runtime: &ToolRuntime, target: &LoginTarget, json_out: bool) -> u8 {
    let result = runtime.refresh_login(target).await;
    if result.success {
        let host_label = result.hostname.as_deref().unwrap_or("github.com");
        if json_out {
            return write_json(&json!({
                "success": true,
                "action": "refresh",
                "hostname": host_label,
                "username": result.username,
            }));
        }
        eprintln!("Refreshed credentials for {host_label}");
        return 0;
    }
    let error = result
        .error
        .as_deref()
        .unwrap_or("The stored credential could not be refreshed.");
    auth_failure(
        ErrorCode::Authentication,
        error,
        Some(LOGIN_NEXT_STEP),
        json_out,
    )
}

/// A live stored OAuth credential for `host`: left in place.
fn already_authenticated(existing: &StoredCredentials, host: &str, json_out: bool) -> u8 {
    if json_out {
        return write_json(&json!({
            "success": true,
            "action": "none",
            "alreadyAuthenticated": true,
            "hostname": host,
            "username": existing.username,
        }));
    }
    let user = if existing.username.is_empty() {
        host
    } else {
        existing.username.as_str()
    };
    println!("Already authenticated as {user} on {host}. Use `--force` to switch accounts.");
    0
}

/// The device flow; the old login stays until the replacement has been
/// authenticated and saved.
async fn device_login(runtime: &ToolRuntime, target: &LoginTarget, json_out: bool) -> u8 {
    match runtime.device_login(target).await {
        Ok(stored) => {
            if json_out {
                return write_json(&json!({
                    "success": true,
                    "action": "login",
                    "hostname": stored.hostname,
                    "username": stored.username,
                }));
            }
            eprintln!(
                "Authenticated as {} on {}",
                stored.username, stored.hostname
            );
            0
        }
        Err(error) => auth_failure(
            error.kind.code(),
            &error.message,
            Some(LOGIN_NEXT_STEP),
            json_out,
        ),
    }
}

pub fn logout(runtime: &ToolRuntime, json_out: bool) -> u8 {
    logout_with(
        runtime,
        &|host| runtime.delete_github_credential(host),
        json_out,
    )
}

fn logout_with(
    runtime: &ToolRuntime,
    delete: &dyn Fn(&str) -> Result<(), octocode_github::ProviderError>,
    json_out: bool,
) -> u8 {
    let view = runtime.inspect_config();
    // Status, login and logout target the one credential host.
    let host = runtime.github_credential_host();
    match delete(&host) {
        Ok(()) if json_out => {
            write_json(&json!({"loggedOut": true, "hostname": host, "home": view.home}))
        }
        Ok(()) => {
            eprintln!(
                "Removed Octocode credentials for {host} from {} and the OS store. Environment and gh credentials are unchanged.",
                view.home.display()
            );
            0
        }
        Err(error) => auth_failure(error.kind.code(), &error.message, None, json_out),
    }
}

#[cfg(test)]
mod auth_tests;
