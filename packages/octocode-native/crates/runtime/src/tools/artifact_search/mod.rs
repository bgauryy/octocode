//! Typed artifact registry discovery and exact metadata lookup.
mod leads;
mod output;

pub(crate) use output::Output;

use crate::providers::RequestBudget;
use crate::providers::artifact::{
    ArtifactCache, ArtifactError, ArtifactProviderContext, ArtifactProviderPage, ReleaseTags,
    SystemArtifactHttp, execute_artifact,
};
use crate::tools::id::ToolId;
use crate::tools::result::Continuation;
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub use crate::providers::artifact::{
    ArtifactItem, ArtifactSearchQuery, ArtifactType, ResolvedNpmRegistry,
};

/// What one call takes from the runtime besides its query.
pub struct ArtifactCall<'a> {
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    /// Opt-in escape hatch for the npm registry SSRF guard (`network`).
    pub allow_private_registry: bool,
    /// The runtime's registry cache; `None` (non-persistent storage)
    /// bypasses it.
    pub cache: Option<&'a ArtifactCache>,
    /// `network.timeout`: each call's request budget, within its deadline.
    pub timeout: std::time::Duration,
    /// `network.maxRetries`: retries of a 5xx or 429 registry answer.
    pub max_retries: u8,
    /// Upstream release-tag checks through the GitHub API.
    pub tags: Option<&'a dyn ReleaseTags>,
    /// The runtime's resolved environment: the npm userconfig and its
    /// `${NAME}` credential values come from it, never from the process.
    pub env: &'a std::collections::BTreeMap<String, String>,
}

pub async fn execute(
    query: ArtifactSearchQuery,
    call: ArtifactCall<'_>,
) -> Result<Value, ArtifactError> {
    let http = SystemArtifactHttp::shared()?.with_retries(call.max_retries);
    let budget = RequestBudget {
        deadline: call.deadline.min(Instant::now() + call.timeout),
        cancellation: call.cancellation,
        max_body_bytes: 16 * 1024 * 1024,
    };
    let npm_registry = npm_registry(&query, call.env)?;
    let page = execute_artifact(
        &query,
        &ArtifactProviderContext {
            http: &http,
            budget: &budget,
            npm_registry: npm_registry.as_ref(),
            allow_private_registry: call.allow_private_registry,
            cache: call.cache,
            tags: call.tags,
        },
    )
    .await?;
    respond(&query, page)
}

/// The npm registry a query reads: only the query names it (default
/// npmjs); credentials come only from the user npmrc `env` names and only
/// when scoped to that origin. Other ecosystems have none.
fn npm_registry(
    query: &ArtifactSearchQuery,
    env: &std::collections::BTreeMap<String, String>,
) -> Result<Option<ResolvedNpmRegistry>, ArtifactError> {
    if query.artifact_type() != ArtifactType::Npm {
        return Ok(None);
    }
    let (raw, cache_identity) = match query.registry_url() {
        Some(raw) => (raw, None),
        None => ("https://registry.npmjs.org/", Some("npmjs")),
    };
    let base = url::Url::parse(raw).map_err(|_| {
        ArtifactError::new(
            "invalidInput",
            "Invalid npm registry URL: use HTTP(S) without credentials, query or fragment.",
        )
    })?;
    let cache_identity = cache_identity
        .or_else(|| base.host_str())
        .unwrap_or("npm")
        .to_owned();
    let authorization = crate::providers::artifact::npm_authorization(&base, env);
    Ok(Some(ResolvedNpmRegistry {
        base,
        authorization,
        cache_identity,
    }))
}

/// The response for one provider page. Rows are typed until here: a
/// homepage that restates the repository page is cleared before rows are
/// encoded, so every row keeps its field order (`name` first). An exact
/// lookup has no pagination; a discovery page states whether more exist and
/// the registry's total, and a later page names itself.
fn respond(
    query: &ArtifactSearchQuery,
    page: ArtifactProviderPage,
) -> Result<Value, ArtifactError> {
    let ArtifactProviderPage {
        mut artifacts,
        next_state,
        total,
        terminal_limit,
        ..
    } = page;
    if artifacts.is_empty()
        && let Some(name) = query.package_name()
        && terminal_limit.is_none()
    {
        // An exact coordinate the registry does not know is not-found, like
        // a GitHub 404; only a keyword discovery can be empty.
        return Err(ArtifactError::new(
            "notFound",
            format!(
                "Package {name} not found in the {} registry",
                query.artifact_type()
            ),
        )
        .with_status(404)
        .with_hint("Check the package name and ecosystem, or discover it with keywords."));
    }
    if !query.debug() {
        artifacts.iter_mut().for_each(drop_repository_homepage);
    }
    let exact = query.package_name().is_some();
    for artifact in &mut artifacts {
        leads::label_release_source(artifact, exact);
        artifact.dedupe_dependency_count();
    }
    // An exact lookup whose source lives on GitHub continues to its tree
    // and manifest. Registry metadata can point at a fork or stale repo, so
    // these are leads, not proof.
    let leads = query
        .package_name()
        .and(artifacts.first())
        .map(leads::source_leads)
        .unwrap_or_default();
    let mut data = json!({
        "artifacts": serde_json::to_value(&artifacts)
            .map_err(|_| ArtifactError::new("providerError", "Failed to encode artifacts."))?,
    });
    if query.package_name().is_none() {
        let current = query.page();
        data["pagination"] = json!({"hasMore": next_state.is_some(), "totalItems": total});
        if current > 1 {
            data["pagination"]["currentPage"] = json!(current);
        }
        if next_state.is_some() {
            let mut next = query.clone();
            next.set_page(current + 1);
            if let Ok(next_query) = serde_json::to_value(next) {
                data["next"]["nextPage"] =
                    Continuation::new(ToolId::ArtifactSearch, next_query).build();
            }
        }
    }
    for (name, lead) in leads {
        data["next"][name] = lead;
    }
    let missing = artifacts
        .iter()
        .filter_map(|artifact| {
            let reference = artifact.missing_ref.as_deref()?;
            let repository = artifact.repository.as_deref().unwrap_or("its repository");
            let repository = repository
                .strip_prefix("https://github.com/")
                .unwrap_or(repository);
            Some(format!(
                "{} {}: the registry's release commit {reference} is not in {repository} on GitHub (unpushed or rewritten); the source lead reads the default branch, not this release.",
                artifact.name,
                artifact.version.as_deref().unwrap_or("")
            ))
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        data["warnings"] = json!(missing);
    }
    if let Some(limit) = terminal_limit {
        data["isPartial"] = json!(true);
        data["terminalLimit"] = json!(true);
        data["partialReasons"] = json!([limit]);
    }
    if artifacts.is_empty() {
        data["status"] = json!("empty");
        data["hints"] = json!(["Try fewer or broader keywords."]);
    }
    Ok(data)
}

/// Clears a homepage that is the repository page (it adds nothing). The
/// registry URL is verbose (core field class); every other field is
/// evidence.
fn drop_repository_homepage(artifact: &mut ArtifactItem) {
    let duplicate = match (artifact.homepage.as_deref(), artifact.repository.as_deref()) {
        (Some(homepage), Some(repository)) => {
            let page = homepage
                .split(['#', '?'])
                .next()
                .unwrap_or(homepage)
                .trim_end_matches('/')
                .trim_end_matches(".git");
            page.eq_ignore_ascii_case(repository.trim_end_matches('/'))
        }
        _ => false,
    };
    if duplicate {
        artifact.homepage = None;
    }
}

#[cfg(test)]
mod row_tests {
    use super::{ArtifactItem, ArtifactType, drop_repository_homepage};

    fn row(homepage: &str, repository: &str) -> ArtifactItem {
        let mut item = ArtifactItem::new(ArtifactType::Npm, "x".into(), "https://r/x".into());
        item.homepage = Some(homepage.into());
        item.repository = Some(repository.into());
        item.description = Some("d".into());
        item
    }

    #[test]
    fn only_a_homepage_that_is_the_repository_page_is_dropped() {
        let mut same = row("https://github.com/o/x#readme", "https://github.com/o/x");
        drop_repository_homepage(&mut same);
        assert_eq!(same.homepage, None);
        let mut distinct = row("https://zod.dev", "https://github.com/o/x");
        drop_repository_homepage(&mut distinct);
        assert_eq!(distinct.homepage.as_deref(), Some("https://zod.dev"));
        // The registry URL is verbose: the verbose stage drops it by default.
        assert!(
            crate::tools::id::ToolId::ArtifactSearch
                .verbose_paths()
                .contains(&"results[].data.artifacts[].registryUrl")
        );
    }

    #[test]
    fn rows_carry_no_type_and_start_with_the_name() {
        let encoded =
            serde_json::to_value(row("https://zod.dev", "https://github.com/o/x")).expect("row");
        let keys = encoded.as_object().expect("row").keys().collect::<Vec<_>>();
        assert_eq!(
            keys.first().map(|key| key.as_str()),
            Some("name"),
            "{encoded}"
        );
        assert!(!keys.iter().any(|key| *key == "type"), "{encoded}");
    }
}

/// The generated query for a test's JSON row.
#[cfg(test)]
fn typed(query: &Value) -> ArtifactSearchQuery {
    serde_json::from_value(query.clone()).expect("valid artifactSearch query")
}

/// A test call: no cache, no tag checks, the default network settings.
#[cfg(test)]
fn call(deadline: Instant, allow_private_registry: bool) -> ArtifactCall<'static> {
    ArtifactCall {
        deadline,
        cancellation: CancellationToken::new(),
        allow_private_registry,
        cache: None,
        timeout: std::time::Duration::from_secs(300),
        max_retries: 1,
        tags: None,
        env: &NO_ENV,
    }
}

/// A test runtime environment with no npm userconfig or home.
#[cfg(test)]
static NO_ENV: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();

#[cfg(test)]
mod page_tests {
    use super::*;
    use std::time::Duration;

    /// An expired deadline separates page validation (runs first) from
    /// provider execution: a page past the paging range fails as
    /// `invalid_query` before any request budget is consulted, while an
    /// in-range page reaches the provider and fails on the dead budget.
    #[tokio::test]
    async fn a_page_past_the_paging_range_is_rejected_before_provider_work() {
        let dead = Instant::now() - Duration::from_secs(1);
        let run = |page: u64| {
            let query = json!({
                "ecosystem": "npm",
                "keywords": ["http"],
                "page": page,
                "mainGoal": "test", "reasoning": "test",
            });
            async move {
                execute(typed(&query), call(dead, false))
                    .await
                    .expect_err("dead budget or rejection")
            }
        };
        assert_eq!(run(5_000).await.code, "invalidInput");
        assert_ne!(run(2).await.code, "invalidInput");
    }
}

#[cfg(test)]
mod npm_auth_tests {
    use super::*;

    /// Loopback npm registry recording each request's Authorization header.
    async fn registry() -> (u16, wiremock::MockServer) {
        let server = wiremock::MockServer::start().await;
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
        let query = json!({
            "ecosystem": "npm",
            "packageName": "audit-package",
            "registryUrl": format!("http://127.0.0.1:{port}"),
            "mainGoal": "test", "reasoning": "test",
        });
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

    /// D12: a missing exact package is notFound (like the GitHub tools), not
    /// an empty discovery.
    #[tokio::test]
    async fn missing_exact_package_is_not_found() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let query = json!({
            "ecosystem": "npm",
            "packageName": "no-such-package-zz",
            "registryUrl": format!("http://127.0.0.1:{}", server.address().port()),
            "mainGoal": "test", "reasoning": "test",
        });
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        let error = execute(typed(&query), call(deadline, true))
            .await
            .expect_err("missing package");
        assert_eq!(error.code, "notFound", "{error:?}");
        assert_eq!(error.status, Some(404));
        assert!(error.message.contains("no-such-package-zz"), "{error:?}");
    }

    /// An exact lookup offers one source lead: the commit the version was
    /// published from (`gitHead`), at the package directory. The default
    /// branch is that lead without `branch`, so no second lead restates it.
    #[tokio::test]
    async fn exact_lookup_offers_one_release_lead_at_the_package_directory() {
        let sha = "4b0051f400219f8d8855f9a5433c6df35f15a639";
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "name": "audit-package", "version": "1.31.0", "gitHead": sha,
                "repository": {"url": "git+https://github.com/o/r.git", "directory": "packages/x"}
            })))
            .mount(&server)
            .await;
        let query = json!({
            "ecosystem": "npm",
            "packageName": "audit-package",
            "registryUrl": format!("http://127.0.0.1:{}", server.address().port()),
        });
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        for version in [None, Some("1.31.0")] {
            let mut query = query.clone();
            if let Some(version) = version {
                query["version"] = json!(version);
            }
            let data = execute(typed(&query), call(deadline, true))
                .await
                .expect("lookup succeeds");
            let next = data["next"].as_object().expect("next");
            assert_eq!(
                next.keys().collect::<Vec<_>>(),
                vec!["viewReleaseSource", "readManifest"],
                "{data}"
            );
            assert_eq!(
                next["readManifest"]["query"]["queries"][0],
                json!({"owner": "o", "repo": "r", "path": "packages/x/package.json", "ref": sha}),
                "{data}"
            );
            // An exact lookup has one row: no pagination, no request echo.
            assert!(data.get("pagination").is_none(), "{data}");
            assert!(data.get("type").is_none(), "{data}");
            let release = &next["viewReleaseSource"];
            assert_eq!(release["tool"], "ghStructure", "{data}");
            assert_eq!(
                release["query"]["queries"][0],
                json!({"owner": "o", "repo": "r", "path": "packages/x", "ref": sha}),
                "{data}"
            );
            // AR2: an unchecked registry ref is labeled as the registry's
            // claim on the row (the one source); leads do not repeat it.
            assert!(release.get("source").is_none(), "{data}");
            assert!(release.get("verification").is_none(), "{data}");
            assert!(next["readManifest"].get("verification").is_none(), "{data}");
            let why = release["why"].as_str().expect("why");
            assert!(why.contains("omit ref"), "{data}");
            let row = data["artifacts"][0].as_object().expect("row");
            assert_eq!(row["sourceRef"], sha, "{data}");
            assert_eq!(row["verification"], "registryRef", "{data}");
            assert!(row.keys().all(|key| key != "gitHead"), "{data}");
            assert_eq!(
                row.keys().next().map(String::as_str),
                Some("name"),
                "{data}"
            );
        }
    }

    /// GitHub answers every ref check with `exists`.
    struct Refs {
        exists: Option<bool>,
    }

    impl crate::providers::artifact::ReleaseTags for Refs {
        fn exists<'a>(
            &'a self,
            _owner: &'a str,
            _repo: &'a str,
            _reference: &'a str,
        ) -> crate::providers::artifact::TagFuture<'a> {
            let exists = self.exists;
            Box::pin(async move { exists })
        }
    }

    /// P7: an npm `gitHead` GitHub does not have (an unpushed or rewritten
    /// commit) is no release source: the row offers the default branch
    /// (`viewRepo`, `verification:"defaultBranch"`) and its warning names the
    /// registry's dead ref, instead of a `viewReleaseSource` that fails. A
    /// ref GitHub has, or a check that could not run, keeps the registry's
    /// claim (`registryRef`).
    #[tokio::test]
    async fn an_npm_git_head_missing_upstream_is_no_release_lead() {
        let sha = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b";
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "name": "typescript", "version": "7.0.2", "gitHead": sha,
                "repository": {"url": "git+https://github.com/microsoft/TypeScript.git"}
            })))
            .mount(&server)
            .await;
        let query = typed(&json!({
            "ecosystem": "npm",
            "packageName": "typescript",
            "registryUrl": format!("http://127.0.0.1:{}", server.address().port()),
        }));
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        let missing = Refs {
            exists: Some(false),
        };
        let checked = ArtifactCall {
            tags: Some(&missing),
            ..call(deadline, true)
        };
        let data = execute(query.clone(), checked).await.expect("lookup");
        let next = data["next"].as_object().expect("next");
        assert_eq!(next.keys().collect::<Vec<_>>(), vec!["viewRepo"], "{data}");
        let row = &data["artifacts"][0];
        assert!(row.get("sourceRef").is_none(), "{data}");
        assert_eq!(row["verification"], "defaultBranch", "{data}");
        let warnings = data["warnings"].to_string();
        assert!(warnings.contains(sha), "{data}");
        assert!(warnings.contains("microsoft/TypeScript"), "{data}");

        for exists in [Some(true), None] {
            let refs = Refs { exists };
            let checked = ArtifactCall {
                tags: Some(&refs),
                ..call(deadline, true)
            };
            let data = execute(query.clone(), checked).await.expect("lookup");
            assert_eq!(data["artifacts"][0]["sourceRef"], sha, "{data}");
            assert_eq!(
                data["artifacts"][0]["verification"], "registryRef",
                "{data}"
            );
            assert!(data["next"]["viewReleaseSource"].is_object(), "{data}");
            assert!(data.get("warnings").is_none(), "{data}");
        }
    }

    /// A discovery page states only what the rows do not: whether more
    /// exist and the registry's total.
    #[tokio::test]
    async fn discovery_pagination_states_more_and_the_total_only() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "objects": [{"package": {"name": "a", "version": "1.0.0"}},
                            {"package": {"name": "b", "version": "1.0.0"}}],
                "total": 9
            })))
            .mount(&server)
            .await;
        let query = json!({
            "ecosystem": "npm",
            "keywords": ["x"],
            "pageSize": 2,
            "registryUrl": format!("http://127.0.0.1:{}", server.address().port()),
        });
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        let data = execute(typed(&query), call(deadline, true))
            .await
            .expect("discovery succeeds");
        assert_eq!(
            data["pagination"],
            json!({"hasMore": true, "totalItems": 9}),
            "{data}"
        );
        assert_eq!(
            data["next"]["nextPage"]["query"]["queries"][0]["pageSize"], 2,
            "{data}"
        );
    }

    #[tokio::test]
    async fn user_npmrc_token_reaches_only_its_scoped_registry() {
        let (port, seen) = registry().await;
        let dir = tempfile::tempdir().unwrap();
        let scoped = dir.path().join("scoped.npmrc");
        std::fs::write(
            &scoped,
            format!("//127.0.0.1:{port}/:_authToken=synthetic\n"),
        )
        .unwrap();
        lookup(port, &scoped).await;
        let other = dir.path().join("other.npmrc");
        std::fs::write(
            &other,
            format!(
                "//127.0.0.1:{}/:_authToken=elsewhere\n",
                port.wrapping_add(1)
            ),
        )
        .unwrap();
        lookup(port, &other).await;
        assert_eq!(
            authorizations(&seen).await,
            vec![Some("Bearer synthetic".to_owned()), None]
        );
    }
}
