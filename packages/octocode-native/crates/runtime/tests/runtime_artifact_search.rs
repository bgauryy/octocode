//! artifactSearch through the runtime: registry lookups and the
//! credentials the runtime env (never a workspace env) supplies.

use crate::support;

use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn artifact_search_lookup_goes_through_execute() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/left-pad/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "left-pad",
            "version": "1.3.0",
            "description": "pad strings",
            "repository": {"type": "git", "url": "https://github.com/stevemao/left-pad"}
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    // The mock registry binds to loopback; opt into the SSRF escape hatch so the
    // happy-path wiring is exercised (default-block is covered by npm.rs unit tests).
    let runtime = workspace.runtime(&[("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_string())]);
    let outcome = call(
        &runtime,
        "artifactSearch",
        json!({
            "operation":"resolve", "ecosystem": "npm",
            "packageName": "left-pad",
            "registryUrl": server.uri()
        }),
    )
    .await
    .expect("artifactSearch");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    let artifact = &data["artifacts"][0];
    assert_eq!(
        (&artifact["packageName"], &artifact["version"]),
        (&json!("left-pad"), &json!("1.3.0")),
        "{data}"
    );
    assert_eq!(
        (&artifact["owner"], &artifact["repo"]),
        (&json!("stevemao"), &json!("left-pad")),
        "{data}"
    );
    runtime.close().await;
}

/// npm credentials come from the runtime's resolved environment, never the
/// process's: each runtime's own userconfig (or its `HOME/.npmrc`) and its
/// own `${VAR}` values decide the Authorization header a registry receives.
#[tokio::test]
async fn artifact_search_npm_credentials_follow_the_runtime_env() {
    let workspace = Workspace::new();
    let empty = workspace.home.join("empty.npmrc");
    std::fs::write(&empty, "").expect("empty npmrc");
    for (case, expected) in [
        ("userconfig", Some("Bearer from-runtime-env")),
        ("home", Some("Bearer from-runtime-home")),
        ("empty", None),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/runtime-env-auth/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "runtime-env-auth",
                "version": "1.0.0"
            })))
            .mount(&server)
            .await;
        let port = server.address().port();
        let injected = workspace.home.join("injected.npmrc");
        std::fs::write(
            &injected,
            format!("//127.0.0.1:{port}/:_authToken=${{RUNTIME_NPM_TOKEN}}\n"),
        )
        .expect("injected npmrc");
        std::fs::write(
            workspace.home.join(".npmrc"),
            format!("//127.0.0.1:{port}/:_authToken=from-runtime-home\n"),
        )
        .expect("home npmrc");
        let mut settings = vec![
            ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_owned()),
            ("OCTOCODE_STORAGE_MODE", "memory".to_owned()),
            ("RUNTIME_NPM_TOKEN", "from-runtime-env".to_owned()),
            ("HOME", workspace.home.to_string_lossy().into_owned()),
        ];
        match case {
            "userconfig" => settings.push((
                "NPM_CONFIG_USERCONFIG",
                injected.to_string_lossy().into_owned(),
            )),
            "empty" => settings.push((
                "npm_config_userconfig",
                empty.to_string_lossy().into_owned(),
            )),
            _ => {}
        }
        let runtime = workspace.runtime(&settings);
        let outcome = call(
            &runtime,
            "artifactSearch",
            json!({"operation":"resolve", "ecosystem": "npm", "packageName": "runtime-env-auth", "registryUrl": server.uri()}),
        )
        .await
        .expect("artifactSearch");
        assert_eq!(
            row_status(&outcome),
            "success",
            "{case}: {}",
            outcome.structured_content
        );
        runtime.close().await;
        let seen: Vec<Option<String>> = server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .map(|request| {
                request
                    .headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            })
            .collect();
        assert!(!seen.is_empty(), "{case}");
        assert!(
            seen.iter().all(|header| header.as_deref() == expected),
            "{case}: {seen:?}"
        );
    }
}

/// A workspace `.octocode/.env` is repository-controlled: it never chooses
/// the npm userconfig, so a repository cannot point credential discovery at
/// its own npmrc and route a user's token to a registry it names.
#[tokio::test]
async fn artifact_search_npm_userconfig_never_comes_from_the_workspace_env() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/runtime-env-auth/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "runtime-env-auth",
            "version": "1.0.0"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let port = server.address().port();
    let planted = workspace.write(
        "planted.npmrc",
        format!("//127.0.0.1:{port}/:_authToken=${{GITHUB_TOKEN}}\n"),
    );
    let mut input = workspace.config(&[
        ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_owned()),
        ("OCTOCODE_STORAGE_MODE", "memory".to_owned()),
        ("HOME", workspace.home.to_string_lossy().into_owned()),
    ]);
    input.project_env = octocode_native::config::FileInput::Read {
        path: workspace.workspace.join(".octocode/.env"),
        text: format!("NPM_CONFIG_USERCONFIG={}\n", planted.display()),
    };
    let runtime =
        octocode_native::runtime::ToolRuntime::with_limits(input, crate::support::FIXTURE_LIMITS)
            .expect("runtime");
    let outcome = call(
        &runtime,
        "artifactSearch",
        json!({"operation":"resolve", "ecosystem": "npm", "packageName": "runtime-env-auth", "registryUrl": server.uri()}),
    )
    .await
    .expect("artifactSearch");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request.headers.get("authorization").is_none()),
        "a workspace .env steered npm credentials"
    );
}

/// A registry failure states GitHub's code for the same HTTP status, and a
/// retryable one offers the same `next.retry` (the same row again) every
/// GitHub failure does; a refused one gets the access hint and no retry.
#[tokio::test]
async fn registry_failures_share_githubs_codes_and_retry_lead() {
    let server = MockServer::builder().start().await;
    for (package, status) in [("down", 503), ("denied", 403)] {
        Mock::given(method("GET"))
            .and(path(format!("/{package}/latest")))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({"error":"no"})))
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_string())]);
    let read = |package: &str| json!({"operation":"resolve", "ecosystem": "npm", "packageName": package, "registryUrl": server.uri()});
    let outcome = call(&runtime, "artifactSearch", read("down"))
        .await
        .expect("row-level error");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "server", "{data}");
    assert_eq!(data["retryable"], true, "{data}");
    let retry = &data["next"]["retry"];
    assert_eq!(retry["tool"], "artifactSearch", "{data}");
    assert!(retry["input"]["cursor"].is_string());
    let repeated = call(&runtime, "artifactSearch", retry["input"].clone())
        .await
        .expect("issued retry");
    assert_eq!(row_data(&repeated)["errorCode"], "server");

    let outcome = call(&runtime, "artifactSearch", read("denied"))
        .await
        .expect("row-level error");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "permission", "{data}");
    assert!(data["next"].get("retry").is_none(), "{data}");
    assert!(
        data["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("credentials")),
        "{data}"
    );
    runtime.close().await;
}
