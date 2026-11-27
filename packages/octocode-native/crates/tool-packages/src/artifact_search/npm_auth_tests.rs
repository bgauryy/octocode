//! npm exact lookups: not-found, release leads and npmrc credential scoping.
use super::*;
use crate::artifact::test_http::KnownTags;
use octocode_test_support::json_fixture;

/// Loopback npm registry recording each request's Authorization header.
/// A registry whose manifest dates itself (its upload record), so a
/// lookup reads one manifest and nothing more (no packument `time`).
async fn registry() -> (u16, wiremock::MockServer) {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(json_fixture(
                r#"{
                    "name": "audit-package",
                    "version": "1.0.0",
                    "_npmOperationalInternal": {"tmp": "tmp/audit-package_1.0.0_1789341914484_0.59"}
                }"#,
            )),
        )
        .mount(&server)
        .await;
    (server.address().port(), server)
}

async fn authorizations(server: &wiremock::MockServer) -> Vec<Option<String>> {
    server
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
        .collect()
}

async fn lookup(port: u16, npmrc: &std::path::Path) {
    let query = json!({"operation":"resolve",
        "ecosystem": "npm",
        "packageName": "audit-package",
        "registryUrl": format!("http://127.0.0.1:{port}")});
    // Generous: building the system HTTP client (native root certs) can
    // be slow in sandboxed test environments.
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let env = std::collections::BTreeMap::from([(
        "NPM_CONFIG_USERCONFIG".to_owned(),
        npmrc.to_string_lossy().into_owned(),
    )]);
    execute(
        typed(&query),
        ArtifactCall {
            env: &env,
            ..call(deadline, true)
        },
    )
    .await
    .expect("lookup succeeds");
}

/// A missing exact package is notFound (like the GitHub tools), not
/// an empty discovery.
#[tokio::test]
async fn missing_exact_package_is_not_found() {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let query = json!({"operation":"resolve",
        "ecosystem": "npm",
        "packageName": "no-such-package-zz",
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())});
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let error = execute(typed(&query), call(deadline, true))
        .await
        .expect_err("missing package");
    assert_eq!(error.code, ErrorCode::NotFound, "{error:?}");
    assert_eq!(error.status, Some(404));
    assert!(error.message.contains("no-such-package-zz"), "{error:?}");
}

/// An exact lookup's row states the commit the version was published from
/// (`gitHead`) as its `ref`, beside its repository: the row carries no call.
#[tokio::test]
async fn exact_lookup_states_its_release_ref_on_the_row() {
    let sha = "4b0051f400219f8d8855f9a5433c6df35f15a639";
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "name": "audit-package", "version": "1.31.0", "gitHead": sha,
            "repository": {"url": "git+https://github.com/o/r.git", "directory": "packages/x"}
        })))
        .mount(&server)
        .await;
    let query = json!({"operation":"resolve",
        "ecosystem": "npm",
        "packageName": "audit-package",
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())});
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    for version in [None, Some("1.31.0")] {
        let mut query = query.clone();
        if let Some(version) = version {
            query["version"] = json!(version);
        }
        let data = execute(typed(&query), call(deadline, true))
            .await
            .expect("lookup succeeds");
        assert!(data.get("related").is_none(), "{data}");
        // An exact lookup has one row: no pagination, no request echo.
        assert!(data.get("pagination").is_none(), "{data}");
        assert!(data.get("type").is_none(), "{data}");
        // An unchecked registry ref is labeled as the registry's claim on
        // the row (the one source).
        let row = data["artifacts"][0].as_object().expect("row");
        assert_eq!(
            (&row["owner"], &row["repo"]),
            (&json!("o"), &json!("r")),
            "{data}"
        );
        assert_eq!(row["ref"], sha, "{data}");
        assert!(!row.contains_key("sourceRef"), "{data}");
        assert_eq!(row["verification"], "registryRef", "{data}");
        assert!(row.keys().all(|key| key != "gitHead"), "{data}");
        // The request names the package: the row does not restate it.
        assert!(!row.contains_key("packageName"), "{data}");
    }
}

/// An npm `gitHead` GitHub does not have (an unpushed or rewritten
/// commit) is no release source: the row states the default branch
/// (`verification:"defaultBranch"`, no `ref`) and its warning names the
/// registry's dead ref. A ref GitHub has, or a check that could not run,
/// keeps the registry's claim (`registryRef`).
#[tokio::test]
async fn an_npm_git_head_missing_upstream_is_no_release_ref() {
    let sha = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b";
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "name": "typescript", "version": "7.0.2", "gitHead": sha,
            "repository": {"url": "git+https://github.com/microsoft/TypeScript.git"}
        })))
        .mount(&server)
        .await;
    let query = typed(&json!({"operation":"resolve",
        "ecosystem": "npm",
        "packageName": "typescript",
        "registryUrl": format!("http://127.0.0.1:{}", server.address().port())}));
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let missing = KnownTags::always(Some(false));
    let checked = ArtifactCall {
        tags: Some(&missing),
        ..call(deadline, true)
    };
    let data = execute(query.clone(), checked).await.expect("lookup");
    let row = &data["artifacts"][0];
    assert!(row.get("ref").is_none(), "{data}");
    assert!(row.get("sourceRef").is_none(), "{data}");
    assert_eq!(row["verification"], "defaultBranch", "{data}");
    assert_eq!(
        data["warnings"],
        json!([format!(
            "typescript 7.0.2: release commit {sha} is not in microsoft/TypeScript (unpushed or rewritten); its source on the default branch may be another version."
        )]),
        "{data}"
    );

    let commit_only: &'static str =
        Box::leak(format!("microsoft/TypeScript@{sha}").into_boxed_str());
    for refs in [KnownTags::new(vec![commit_only]), KnownTags::always(None)] {
        let checked = ArtifactCall {
            tags: Some(&refs),
            ..call(deadline, true)
        };
        let data = execute(query.clone(), checked).await.expect("lookup");
        assert_eq!(data["artifacts"][0]["ref"], sha, "{data}");
        assert_eq!(
            data["artifacts"][0]["verification"], "registryRef",
            "{data}"
        );
        match refs.commit_check_failure() {
            // A check that failed is stated, with its cause, not silent: the
            // commit stays what the registry named.
            Some(failure) => {
                let warnings = data["warnings"].to_string();
                assert!(warnings.contains("could not be checked"), "{data}");
                assert!(warnings.contains(&failure.describe()), "{data}");
            }
            None => assert!(data.get("warnings").is_none(), "{data}"),
        }
    }

    // A release tag naming the recorded commit proves it; one naming another
    // commit is said on the row, and the ref stays what the registry named.
    let tied: &'static str =
        Box::leak(format!("microsoft/TypeScript@v7.0.2={sha}").into_boxed_str());
    let refs = KnownTags::new(vec![commit_only, tied]);
    let checked = ArtifactCall {
        tags: Some(&refs),
        ..call(deadline, true)
    };
    let data = execute(query.clone(), checked).await.expect("lookup");
    assert_eq!(data["artifacts"][0]["verification"], "tag", "{data}");
    assert_eq!(data["artifacts"][0]["releaseTag"], "v7.0.2", "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
    let refs = KnownTags::new(vec![commit_only, "microsoft/TypeScript@v7.0.2=f00d"]);
    let checked = ArtifactCall {
        tags: Some(&refs),
        ..call(deadline, true)
    };
    let data = execute(query.clone(), checked).await.expect("lookup");
    assert_eq!(data["artifacts"][0]["ref"], sha, "{data}");
    assert!(data["artifacts"][0].get("releaseTag").is_none(), "{data}");
    assert_eq!(
        data["artifacts"][0]["verification"], "registryRef",
        "{data}"
    );
    assert_eq!(
        data["warnings"],
        json!([format!(
            "typescript 7.0.2: the registry names commit {sha}, but release tag v7.0.2 is commit f00d."
        )]),
        "{data}"
    );
}

#[tokio::test]
async fn user_npmrc_token_reaches_only_its_scoped_registry() {
    let (port, seen) = registry().await;
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let scoped = dir.write(
        "scoped.npmrc",
        format!("//127.0.0.1:{port}/:_authToken=synthetic\n"),
    );
    lookup(port, &scoped).await;
    let other = dir.write(
        "other.npmrc",
        format!(
            "//127.0.0.1:{}/:_authToken=elsewhere\n",
            port.wrapping_add(1)
        ),
    );
    lookup(port, &other).await;
    assert_eq!(
        authorizations(&seen).await,
        vec![Some("Bearer synthetic".to_owned()), None]
    );
}

/// ALL-TOOLS-C D7 with credentials: the packument read that dates an
/// older version sends the registry's own token (a private registry
/// would refuse it otherwise) and the token never reaches the output.
#[tokio::test]
async fn packument_date_read_uses_the_registry_credential_and_never_prints_it() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let port = server.address().port();
    wiremock::Mock::given(method("GET"))
        .and(path("/audit-package/1.0.0"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(json_fixture(
                r#"{"name": "audit-package", "version": "1.0.0"}"#,
            )),
        )
        .mount(&server)
        .await;
    wiremock::Mock::given(method("GET"))
        .and(path("/audit-package"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(json_fixture(
                r#"{
                    "name": "audit-package",
                    "versions": {"1.0.0": {"name": "audit-package"}},
                    "time": {
                        "created": "2012-01-01T00:00:00.000Z",
                        "1.0.0": "2012-10-23T04:30:12.000Z"
                    }
                }"#,
            )),
        )
        .mount(&server)
        .await;
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let npmrc = dir.write(
        "scoped.npmrc",
        format!("//127.0.0.1:{port}/:_authToken=synthetic-secret\n"),
    );
    let query = json!({"operation":"resolve", "ecosystem": "npm",
        "packageName": "audit-package", "version": "1.0.0",
        "registryUrl": format!("http://127.0.0.1:{port}")});
    let env = std::collections::BTreeMap::from([(
        "NPM_CONFIG_USERCONFIG".to_owned(),
        npmrc.to_string_lossy().into_owned(),
    )]);
    let deadline = Instant::now() + std::time::Duration::from_secs(300);
    let data = execute(
        typed(&query),
        ArtifactCall {
            env: &env,
            ..call(deadline, true)
        },
    )
    .await
    .expect("lookup succeeds");
    assert_eq!(data["artifacts"][0]["publishedAt"], "2012-10-23", "{data}");
    assert!(!data.to_string().contains("synthetic-secret"), "{data}");
    let seen = authorizations(&server).await;
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(
        seen.iter()
            .all(|auth| auth.as_deref() == Some("Bearer synthetic-secret")),
        "{seen:?}"
    );
}
