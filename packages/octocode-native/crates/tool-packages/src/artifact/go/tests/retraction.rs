use super::*;
use octocode_test_support::json_fixture;

/// A pinned version the module's latest `go.mod` retracts is stated with
/// the module's reason; a version it does not retract, and the latest
/// lookup, state nothing.
#[tokio::test]
async fn a_version_the_latest_go_mod_retracts_is_disclosed() {
    let module = |version: &str| {
        (
            format!("/v1/module/github.com/o/r?version={version}"),
            json!({"path":"github.com/o/r","version":version,
                "repoUrl":"https://github.com/o/r"}),
        )
    };
    let mut http = RouteHttp::new();
    for version in ["v1.4.0", "v1.5.0", "v1.6.0"] {
        let (target, body) = module(version);
        http = http.json(&target, body);
    }
    let http = http
    .json("/github.com/o/r/@latest", json_fixture(r#"{"Version": "v1.6.0"}"#))
    .bytes(
        "/github.com/o/r/@v/v1.6.0.mod",
        "module github.com/o/r\n\nretract (\n    v1.4.0 // Contains a backwards incompatible change.\n    // Published by mistake.\n    [v1.5.0, v1.5.9]\n)\n",
    );
    let b = test_budget();
    let client = RegistryClient::new(&http, &b);
    let resolve = |version: &str| {
        artifact_query(
            json!({"operation":"resolve","ecosystem": ArtifactType::Go,
                "packageName": "github.com/o/r", "version": version}),
            None,
        )
    };
    let state = ArtifactProviderState::default();
    let retracted = go(&resolve("v1.4.0"), &state, &client)
        .await
        .expect("v1.4.0");
    assert_eq!(
        retracted.artifacts[0]
            .retraction
            .as_ref()
            .and_then(|r| r.reason.as_deref()),
        Some("Contains a backwards incompatible change.")
    );
    let ranged = go(&resolve("v1.5.0"), &state, &client)
        .await
        .expect("v1.5.0");
    assert_eq!(
        ranged.artifacts[0]
            .retraction
            .as_ref()
            .and_then(|r| r.reason.as_deref()),
        Some("Published by mistake.")
    );
    let latest = go(&resolve("v1.6.0"), &state, &client)
        .await
        .expect("v1.6.0");
    assert_eq!(latest.artifacts[0].retraction, None);
}

/// Retraction operands: a version, a closed range, a block, comments
/// above or after the directive; another version holds none.
#[test]
fn retract_directives_name_the_versions_they_withdraw() {
    let go_mod = "module m\n\n// Rationale above.\nretract v1.0.0\nretract [v1.1.0, v1.1.5] // range\nretract (\n v2.0.0-rc.1 // pre\n)\n";
    let reason = |version: &str| retract::retraction(go_mod, version).map(|r| r.reason);
    assert_eq!(reason("v1.0.0"), Some(Some("Rationale above.".to_owned())));
    assert_eq!(reason("v1.1.3"), Some(Some("range".to_owned())));
    assert_eq!(reason("v1.1.5"), Some(Some("range".to_owned())));
    assert_eq!(reason("v2.0.0-rc.1"), Some(Some("pre".to_owned())));
    assert_eq!(reason("v1.1.6"), None);
    assert_eq!(reason("v1.0.1"), None);
    assert_eq!(reason("nonsense"), None);
}

/// A proxy that cannot say whether a version is retracted is a stated
/// "unchecked", never a version that reads as clean.
#[tokio::test]
async fn a_failed_retraction_check_is_disclosed_as_unchecked() {
    let (target, body) = module_row("github.com/o/r", "v1.4.0", "https://github.com/o/r");
    let resolve = |http: RouteHttp| async move {
        let b = test_budget();
        let client = RegistryClient::new(&http, &b);
        let query = artifact_query(
            json!({"operation":"resolve","ecosystem": ArtifactType::Go,
                "packageName": "github.com/o/r", "version": "v1.4.0"}),
            None,
        );
        go(&query, &ArtifactProviderState::default(), &client)
            .await
            .expect("lookup")
            .artifacts
            .remove(0)
    };
    // `@latest` unreadable.
    let item = resolve(RouteHttp::new().json(&target, body.clone()).status(
        "/github.com/o/r/@latest",
        503,
        json!({}),
    ))
    .await;
    assert_eq!(item.retraction, None);
    assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
    assert!(item.disclosures[0].contains("retraction unchecked"));
    // The latest version's go.mod unreadable.
    let item = resolve(
        RouteHttp::new()
            .json(&target, body.clone())
            .json(
                "/github.com/o/r/@latest",
                json_fixture(r#"{"Version": "v1.6.0"}"#),
            )
            .status("/github.com/o/r/@v/v1.6.0.mod", 500, json!({})),
    )
    .await;
    assert_eq!(item.disclosures.len(), 1, "{:?}", item.disclosures);
    assert!(item.disclosures[0].contains("v1.6.0"));
    // A go.mod that says nothing of the version: checked, no disclosure.
    let item = resolve(
        RouteHttp::new()
            .json(&target, body)
            .json(
                "/github.com/o/r/@latest",
                json_fixture(r#"{"Version": "v1.6.0"}"#),
            )
            .bytes("/github.com/o/r/@v/v1.6.0.mod", "module github.com/o/r\n"),
    )
    .await;
    assert!(item.disclosures.is_empty(), "{:?}", item.disclosures);
    assert_eq!(item.retraction, None);
}

/// A cancelled call fails the lookup: a cancelled proxy request, for the latest
/// version or for its go.mod, is not a retraction check to disclose as unchecked.
#[tokio::test]
async fn a_cancelled_retraction_check_fails_the_lookup() {
    let (target, body) = module_row("github.com/o/r", "v1.4.0", "https://github.com/o/r");
    let query = artifact_query(
        json!({"operation":"resolve","ecosystem": ArtifactType::Go,
            "packageName": "github.com/o/r", "version": "v1.4.0"}),
        None,
    );
    for http in [
        RouteHttp::new()
            .json(&target, body.clone())
            .fails("/github.com/o/r/@latest", ErrorCode::Cancelled),
        RouteHttp::new()
            .json(&target, body.clone())
            .json(
                "/github.com/o/r/@latest",
                json_fixture(r#"{"Version": "v1.6.0"}"#),
            )
            .fails("/github.com/o/r/@v/v1.6.0.mod", ErrorCode::Cancelled),
    ] {
        let b = test_budget();
        let client = RegistryClient::new(&http, &b);
        let error = go(&query, &ArtifactProviderState::default(), &client)
            .await
            .expect_err("cancelled");
        assert_eq!(error.code, ErrorCode::Cancelled);
    }
}
