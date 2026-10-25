//! npm exact lookups: not-found, release leads and npmrc credential scoping.
use super::*;
use crate::providers::artifact::test_http::KnownTags;

/// Loopback npm registry recording each request's Authorization header.
async fn registry() -> (u16, wiremock::MockServer) {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(json!({"name": "audit-package", "version": "1.0.0"})),
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

/// An exact lookup offers one source lead: the commit the version was
/// published from (`gitHead`), at the package directory. The default
/// branch is that lead without `branch`, so no second lead restates it.
#[tokio::test]
async fn exact_lookup_offers_one_release_lead_at_the_package_directory() {
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
        let related = data["related"].as_array().expect("related source call");
        assert_eq!(related.len(), 1, "{data}");
        // An exact lookup has one row: no pagination, no request echo.
        assert!(data.get("pagination").is_none(), "{data}");
        assert!(data.get("type").is_none(), "{data}");
        let release =
            crate::tools::test_support::checks::related_call(&data, "ghStructure", |_| true);
        assert_eq!(release["tool"], "ghStructure", "{data}");
        assert_eq!(
            release["input"]["queries"][0],
            json!({"operation":"tree", "owner": "o", "repo": "r", "path": "packages/x", "ref": sha}),
            "{data}"
        );
        // An unchecked registry ref is labeled as the registry's
        // claim on the row (the one source); leads do not repeat it.
        assert!(release.get("source").is_none(), "{data}");
        assert!(release.get("verification").is_none(), "{data}");
        let row = data["artifacts"][0].as_object().expect("row");
        assert_eq!(row["ref"], sha, "{data}");
        assert!(!row.contains_key("sourceRef"), "{data}");
        assert_eq!(row["verification"], "registryRef", "{data}");
        assert!(row.keys().all(|key| key != "gitHead"), "{data}");
        assert_eq!(
            row.keys().next().map(String::as_str),
            Some("packageName"),
            "{data}"
        );
    }
}

/// An npm `gitHead` GitHub does not have (an unpushed or rewritten
/// commit) is no release source: the row offers the default branch
/// (`viewRepo`, `verification:"defaultBranch"`) and its warning names the
/// registry's dead ref, instead of a `viewReleaseSource` that fails. A
/// ref GitHub has, or a check that could not run, keeps the registry's
/// claim (`registryRef`).
#[tokio::test]
async fn an_npm_git_head_missing_upstream_is_no_release_lead() {
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
    let fallback = crate::tools::test_support::checks::related_call(&data, "ghStructure", |_| true);
    assert!(
        fallback["input"]["queries"][0].get("ref").is_none(),
        "{data}"
    );
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

    for exists in [Some(true), None] {
        let refs = KnownTags::always(exists);
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
        assert_eq!(
            crate::tools::test_support::checks::related_call(&data, "ghStructure", |_| true)["input"]
                ["queries"][0]["ref"],
            sha,
            "{data}"
        );
        assert!(data.get("warnings").is_none(), "{data}");
    }
}

#[tokio::test]
async fn user_npmrc_token_reaches_only_its_scoped_registry() {
    let (port, seen) = registry().await;
    let dir = crate::tools::test_support::Fixture::new();
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
