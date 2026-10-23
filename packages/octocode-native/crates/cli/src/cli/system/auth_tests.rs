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
