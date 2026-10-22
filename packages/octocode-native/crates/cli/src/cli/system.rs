//! System commands: authentication and cache maintenance.
//!
//! These wrap [`ToolRuntime`] to surface credential state and login/logout —
//! the environment-management side of the CLI, distinct from tool dispatch in
//! the parent module.
use super::write_json;
use octocode_contract_types::ErrorCode;
use octocode_native::response::exit::ExitClass;
use octocode_native::runtime::{FailureKind, ToolRuntime};
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

/// Use the same credential hostname for status, login, and logout.
fn configured_github_host(runtime: &ToolRuntime) -> String {
    octocode_native::providers::github::configured_credential_host(
        &runtime.config().resolved.github.api_url,
    )
    .unwrap_or_else(|| "github.com".into())
}

async fn resolve_auth(
    runtime: &ToolRuntime,
    host: &str,
) -> Option<octocode_native::providers::github::AuthSelection> {
    use octocode_io::budget::RequestBudget;
    use octocode_native::providers::github::{AuthMode, Authentication};
    let auth = Authentication::new(std::sync::Arc::new(runtime.config().clone()));
    let budget = RequestBudget::with_timeout(std::time::Duration::from_secs(5), 0);
    auth.resolve(host, AuthMode::Inspect, &budget)
        .await
        .ok()
        .flatten()
}

/// Where the active GitHub token comes from, without verifying it.
pub(super) async fn active_token_source(runtime: &ToolRuntime) -> Option<String> {
    let host = configured_github_host(runtime);
    resolve_auth(runtime, &host)
        .await
        .map(|selection| selection.source_label().to_owned())
}

pub async fn auth_status(runtime: &ToolRuntime, json_out: bool) -> u8 {
    use octocode_native::providers::github::login::{TokenCheck, verify_token};
    let api_base = &runtime.config().resolved.github.api_url;
    let hostname = configured_github_host(runtime);
    let selection = resolve_auth(runtime, &hostname).await;
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
    // Not authenticated is the documented auth exit (4), whatever the mode.
    let unauthenticated = super::exit_code(ExitClass::Failed(FailureKind::Authentication));
    if json_out {
        let written = write_json(&json!({
            "authenticated": authenticated,
            "verification": verification,
            "username": username,
            "hostname": hostname,
            "tokenSource": source,
        }));
        return match written {
            0 if authenticated => 0,
            0 => unauthenticated,
            failed => failed,
        };
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
            unauthenticated
        }
        _ => {
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
    // login; otherwise fall back to the host derived from `github.apiUrl`.
    let host = hostname
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| configured_github_host(runtime));
    let client_id_env = octocode_native::config::environment::OCTOCODE_GITHUB_CLIENT_ID;
    let client_id = octocode_native::providers::github::login::client_id_for_host(
        &host,
        runtime.config().env_value(client_id_env),
    );
    if client_id.is_empty() {
        let message =
            format!("{client_id_env} is required for GitHub Enterprise device login and refresh.");
        return auth_failure(ErrorCode::MissingConfiguration, &message, None, json_out);
    }

    let credential_store =
        octocode_native::providers::github::CredentialStore::new(&runtime.config().home);

    // Refresh path: exchange the stored refresh token; no device flow, no TTY.
    if refresh {
        return refresh_login(&host, client_id, &credential_store, json_out).await;
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
        return already_authenticated(existing, &host, json_out);
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
        return auth_failure(ErrorCode::InvalidInput, message, None, json_out);
    }

    device_login(&host, client_id, &credential_store, json_out).await
}

/// `auth login --refresh`: exchange the stored refresh token.
async fn refresh_login(
    host: &str,
    client_id: &str,
    store: &octocode_native::providers::github::CredentialStore,
    json_out: bool,
) -> u8 {
    let result = octocode_native::providers::github::login::refresh_auth_token_result_in_store(
        host, client_id, store,
    )
    .await;
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
fn already_authenticated(
    existing: &octocode_native::providers::github::StoredCredentials,
    host: &str,
    json_out: bool,
) -> u8 {
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
async fn device_login(
    host: &str,
    client_id: &str,
    store: &octocode_native::providers::github::CredentialStore,
    json_out: bool,
) -> u8 {
    let endpoints = octocode_native::providers::github::login::LoginEndpoints::from_host(host);
    match octocode_native::providers::github::login::login_device_flow_in_store(
        &endpoints,
        client_id,
        &tokio_util::sync::CancellationToken::new(),
        store,
    )
    .await
    {
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
    let store = octocode_native::providers::github::CredentialStore::new(&runtime.config().home);
    logout_with(runtime, &|host| store.delete(host), json_out)
}

fn logout_with(
    runtime: &ToolRuntime,
    delete: &dyn Fn(&str) -> Result<(), octocode_github::ProviderError>,
    json_out: bool,
) -> u8 {
    let view = runtime.inspect_config();
    // Same host derivation as auth status / login so all three target the
    // identical credential-store host (api.github.com → github.com).
    let host = configured_github_host(runtime);
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
mod auth_tests {
    use super::*;
    use octocode_github::{ProviderError, ProviderErrorKind};
    use octocode_native::runtime::HostOptions;
    use std::{collections::BTreeMap, sync::Mutex};

    fn runtime_with(env: &[(&str, &str)]) -> (tempfile::TempDir, ToolRuntime) {
        let dir = tempfile::tempdir().expect("fixture home");
        let mut vars = BTreeMap::from([
            (
                "OCTOCODE_HOME".to_owned(),
                dir.path().to_string_lossy().into_owned(),
            ),
            ("OCTOCODE_ENABLE_STATS".to_owned(), "false".to_owned()),
        ]);
        vars.extend(env.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
        let runtime = ToolRuntime::from_host(HostOptions {
            cwd: Some(dir.path().into()),
            env: Some(vars),
            ..HostOptions::default()
        })
        .expect("fixture runtime");
        (dir, runtime)
    }

    #[test]
    fn an_auth_failure_exits_by_its_code_class_in_either_output_mode() {
        for json_out in [false, true] {
            assert_eq!(
                auth_failure(ErrorCode::Authentication, "m", Some("next"), json_out),
                4
            );
            assert_eq!(
                auth_failure(ErrorCode::InvalidInput, "m", None, json_out),
                2
            );
            assert_eq!(
                auth_failure(ErrorCode::MissingConfiguration, "m", None, json_out),
                5
            );
            assert_eq!(
                auth_failure(ErrorCode::CredentialStoreUnavailable, "m", None, json_out),
                5
            );
        }
    }

    #[test]
    fn the_credential_host_is_derived_from_the_api_url_with_a_public_default() {
        for (api, host) in [
            (None, "github.com"),
            (Some("https://api.github.com"), "github.com"),
            (Some("https://ghe.example/api/v3"), "ghe.example"),
        ] {
            let env = api.map(|api| [("GITHUB_API_URL", api)]);
            let (_dir, runtime) = runtime_with(env.as_ref().map_or(&[], |env| env.as_slice()));
            assert_eq!(configured_github_host(&runtime), host, "{api:?}");
        }
    }

    #[test]
    fn logout_in_json_mode_deletes_once_and_maps_a_failure_to_its_exit_class() {
        let (_dir, runtime) = runtime_with(&[("GITHUB_API_URL", "https://ghe.example/api/v3")]);
        let calls = Mutex::new(Vec::new());
        let delete = |host: &str| {
            calls.lock().expect("calls").push(host.to_owned());
            Ok(())
        };
        assert_eq!(logout_with(&runtime, &delete, true), 0);
        assert_eq!(*calls.lock().expect("calls"), ["ghe.example"]);
        for (kind, exit) in [
            (ProviderErrorKind::Authentication, 4),
            (ProviderErrorKind::Validation, 2),
            (ProviderErrorKind::CredentialStoreUnavailable, 5),
        ] {
            let failure = |_: &str| Err(ProviderError::new(kind, "fixture delete failed"));
            assert_eq!(logout_with(&runtime, &failure, true), exit, "{kind:?}");
        }
    }

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
            assert_eq!(logout_with(&runtime, &delete, false), 0);
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
            // A store that cannot delete is the declared execution failure.
            assert_eq!(logout_with(&runtime, &failure, false), 5);
        }
    }
}
