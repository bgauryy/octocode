//! Typed artifact registry discovery and exact metadata lookup.
use crate::providers::RequestBudget;
use crate::providers::artifact::{
    ArtifactError, ArtifactProviderContext, ArtifactSearchQuery, SystemArtifactHttp,
    execute_artifact,
};
use crate::tools::id::ToolId;
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub use crate::providers::artifact::{ArtifactItem, ArtifactType, ResolvedNpmRegistry};

/// Signature scope for a query's cursor: a digest of the normalized query
/// with the cursor itself removed, so a token lifted onto a different query
/// fails verification while every page of one query shares a scope.
fn cursor_scope(query: &ArtifactSearchQuery) -> Result<String, ArtifactError> {
    let mut value = serde_json::to_value(query)
        .map_err(|_| ArtifactError::new("provider_error", "Failed to derive cursor scope."))?;
    if let Some(object) = value.as_object_mut() {
        // Caller intent and diagnostics do not change which results a page
        // holds; a replayed page may restate them freely.
        for meta in crate::runtime::cursor::INTENT_FIELDS
            .iter()
            .chain(&["cursor"])
        {
            object.remove(*meta);
        }
        // Removing optional fields can reorder a preserve_order JSON map.
        // Scope identity depends on values, not the presence of a cursor.
        object.sort_keys();
    }
    crate::runtime::cursor::scope_digest(&value)
        .map_err(|_| ArtifactError::new("provider_error", "Failed to derive cursor scope."))
}

fn unrecognized_cursor() -> ArtifactError {
    ArtifactError::new(
        "invalid_query",
        "Unrecognized cursor. Copy the complete next.nextPage query unchanged, or omit the cursor to restart.",
    )
}

pub async fn execute(
    query: &Value,
    deadline: Instant,
    cancellation: CancellationToken,
    allow_private_registry: bool,
    octocode_home: Option<&std::path::Path>,
    // Config revision forwarded to the in-process registry HTTP cache.
    cache_revision: u64,
    // When `false` the in-process registry HTTP cache is bypassed entirely
    // (both reads and writes).  Mirrors `storage.mode == "persistent"`.
    cache_enabled: bool,
) -> Result<Value, ArtifactError> {
    run(
        query,
        deadline,
        cancellation,
        allow_private_registry,
        octocode_home,
        cache_revision,
        cache_enabled,
        crate::providers::artifact::user_npmrc_path(),
    )
    .await
}

/// [`execute`] with the user npmrc location made explicit (tests inject it).
#[allow(clippy::too_many_arguments)]
async fn run(
    query: &Value,
    deadline: Instant,
    cancellation: CancellationToken,
    allow_private_registry: bool,
    octocode_home: Option<&std::path::Path>,
    cache_revision: u64,
    cache_enabled: bool,
    user_npmrc: Option<std::path::PathBuf>,
) -> Result<Value, ArtifactError> {
    let mut query: ArtifactSearchQuery = serde_json::from_value(query.clone())
        .map_err(|error| ArtifactError::new("invalid_query", error.to_string()))?;
    let signing_key = crate::runtime::cursor::user_signing_key(octocode_home);
    // Signed cursors (issued by us) must verify against this query's scope.
    // A legacy raw-JSON state is still accepted for one release (dual-accept
    // window) and remains range-clamped by the provider.
    if let Some(cursor) = query.cursor().map(str::to_owned)
        && cursor.starts_with(crate::runtime::cursor::SIGNED_STATE_PREFIX)
    {
        let scope = cursor_scope(&query)?;
        let payload = crate::runtime::cursor::verify_state(signing_key, &scope, &cursor)
            .map_err(|_| unrecognized_cursor())?;
        let state = String::from_utf8(payload).map_err(|_| unrecognized_cursor())?;
        query.set_cursor(&state)?;
    }
    let http = SystemArtifactHttp::new()?;
    let budget = RequestBudget {
        deadline,
        cancellation,
        max_body_bytes: 16 * 1024 * 1024,
    };
    // The registry comes only from the query (default npmjs); credentials
    // come only from the user npmrc and only when scoped to that origin.
    let requested_registry = if query.artifact_type() == ArtifactType::Npm {
        let (base, cache_identity) = match query.registry() {
            Some(raw) => {
                let base = url::Url::parse(raw).map_err(|_| {
                    ArtifactError::new(
                        "invalid_query",
                        "Invalid npm registry URL: use HTTP(S) without credentials, query or fragment.",
                    )
                })?;
                let identity = base.host_str().unwrap_or("npm").to_owned();
                (base, identity)
            }
            None => (
                url::Url::parse("https://registry.npmjs.org/").map_err(|_| {
                    ArtifactError::new("invalid_query", "Invalid default npm registry URL.")
                })?,
                "npmjs".to_owned(),
            ),
        };
        let authorization = user_npmrc
            .as_ref()
            .and_then(|path| crate::providers::artifact::npm_authorization(&base, path));
        Some(ResolvedNpmRegistry {
            base,
            authorization,
            cache_identity,
        })
    } else {
        None
    };
    let page = execute_artifact(
        &query,
        &ArtifactProviderContext {
            http: &http,
            budget: &budget,
            npm_registry: requested_registry.as_ref(),
            allow_private_registry,
            cache_revision,
            cache_enabled,
        },
    )
    .await?;
    let has_more = page.next_state.is_some();
    let mut rows = serde_json::to_value(&page.artifacts)
        .map_err(|_| ArtifactError::new("provider_error", "Failed to encode artifacts."))?;
    if !query.debug() {
        compact_rows(&mut rows);
        lean_rows(&mut rows, query.artifact_type());
    }
    let mut data = json!({
        "artifacts": rows,
        "pagination": {
            "perPage": query.page_size().unwrap_or(page.artifacts.len()),
            "returned": page.artifacts.len(),
            "hasMore": has_more,
            "totalFound": page.total,
        },
        "type": query.artifact_type(),
    });
    if let Some(state) = page.next_state {
        let state_json = serde_json::to_string(&state).map_err(|_| {
            ArtifactError::new("provider_error", "Failed to encode pagination cursor.")
        })?;
        let mut next = query.clone();
        let scope = cursor_scope(&next)?;
        let cursor = crate::runtime::cursor::sign_state(signing_key, &scope, state_json.as_bytes())
            .map_err(|_| {
                ArtifactError::new("provider_error", "Failed to encode pagination cursor.")
            })?;
        next.set_cursor(&cursor)?;
        if let Ok(next_query) = serde_json::to_value(next) {
            data["next"] = json!({
                "nextPage": {
                    "tool": ToolId::ArtifactSearch.as_str(),
                    "query": next_query
                }
            });
        }
    }
    // An exact lookup whose source lives on GitHub continues straight to its
    // tree (package subdirectory when the registry names one). Registry
    // metadata can point at a fork or stale repo, so this is a lead, not proof.
    // `source` states each lead's scope and verification (its confidence).
    if query.package_name().is_some()
        && let Some(artifact) = page.artifacts.first()
        && let Some((owner, repo)) = artifact.repository.as_deref().and_then(github_repo)
    {
        data["next"]["viewRepo"] = json!({
            "tool": ToolId::GhStructure.as_str(),
            "source": { "scope": "defaultBranch", "verification": "unverified" },
            "why": "Default-branch code; not release evidence.",
            // The root at depth one is ghStructure's default listing.
            "query": {
                "owner": owner,
                "repo": repo,
            },
        });
        if let Some(directory) = artifact
            .repository_directory
            .clone()
            .or_else(|| artifact.repository.as_deref().and_then(github_repo_dir))
            .filter(|directory| !directory.is_empty())
        {
            data["next"]["viewRepo"]["query"]["path"] = json!(directory);
        }
        // The published version's own commit. An npm provenance attestation
        // bound to this tarball and repository names it (`provenance`);
        // otherwise it is the registry's unchecked lead (npm `gitHead`, Go
        // pseudo-version), which may name a commit never pushed upstream.
        if let Some(reference) = artifact.source_ref.as_deref() {
            let attested = artifact.source_attested;
            let mut release = data["next"]["viewRepo"].clone();
            release["source"] = json!({
                "scope": "release",
                "verification": if attested { "provenance" } else { "unverified" },
            });
            release["query"]["branch"] = json!(reference);
            release["why"] = json!(if attested {
                "Release commit attested by npm provenance."
            } else {
                "Registry release-ref lead; if unpushed, omit branch."
            });
            data["next"]["viewReleaseSource"] = release;
            // A pinned version asks about its release; the default-branch
            // lead is the release lead without `branch`.
            if query.version().is_some()
                && let Some(next) = data["next"].as_object_mut()
            {
                next.remove("viewRepo");
            }
        }
    }
    if let Some(limit) = page.terminal_limit.as_deref() {
        data["isPartial"] = json!(true);
        data["terminalLimit"] = json!(true);
        data["partialReasons"] = json!([limit]);
    }
    if page.artifacts.is_empty() {
        // An exact coordinate the registry does not know is not-found, like a
        // GitHub 404; only a keyword discovery can be empty.
        if let Some(name) = query.package_name()
            && page.terminal_limit.is_none()
        {
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
        data["status"] = json!("empty");
        data["hints"] = json!(["Try fewer or broader keywords."]);
    }
    Ok(data)
}

/// Drop what a row restates: the registry URL (derivable from type and
/// name; kept under `debug`) and a homepage that is the repository page.
fn compact_rows(rows: &mut Value) {
    for row in rows.as_array_mut().into_iter().flatten() {
        let Some(row) = row.as_object_mut() else {
            continue;
        };
        row.remove("registryUrl");
        let duplicate = match (
            row.get("homepage").and_then(Value::as_str),
            row.get("repository").and_then(Value::as_str),
        ) {
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
            row.remove("homepage");
        }
    }
}

/// Drop what the request already states: each row's `type` (the queried
/// registry). Every other field is evidence: an exact lookup keeps the
/// description and a homepage that is not the repository page
/// ([`compact_rows`] drops only that duplicate).
fn lean_rows(rows: &mut Value, artifact_type: ArtifactType) {
    let requested = serde_json::to_value(artifact_type).ok();
    for row in rows.as_array_mut().into_iter().flatten() {
        let Some(row) = row.as_object_mut() else {
            continue;
        };
        if requested.is_some() && row.get("type") == requested.as_ref() {
            row.remove("type");
        }
    }
}

/// `owner/repo` from a GitHub repository URL in any common registry form
/// (`git+https://github.com/o/r.git`, `git@github.com:o/r`, `github.com/o/r/tree/…`).
fn github_repo(url: &str) -> Option<(String, String)> {
    let rest = url
        .split_once("github.com/")
        .or_else(|| url.split_once("github.com:"))?
        .1;
    let mut parts = rest.split(['/', '#', '?']);
    let owner = parts.next().filter(|part| !part.is_empty())?;
    let repo = parts.next()?.trim_end_matches(".git");
    (!repo.is_empty()).then(|| (owner.to_owned(), repo.to_owned()))
}

/// Monorepo subdirectory from a `/tree/<ref>/<dir>` or `/blob/<ref>/<file>`
/// repository URL. The ref is dropped: refs may contain slashes, and URL refs
/// are often stale; only viewReleaseSource pins the registry's source ref.
fn github_repo_dir(url: &str) -> Option<String> {
    let rest = url.split_once("github.com/")?.1;
    let rest = rest.split(['#', '?']).next()?;
    let mut parts = rest.split('/').filter(|part| !part.is_empty());
    let (_owner, _repo, kind, _ref) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let mut segments: Vec<&str> = parts.collect();
    match kind {
        "tree" => {}
        "blob" => {
            segments.pop();
        }
        _ => return None,
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

#[cfg(test)]
mod github_repo_tests {
    use super::{compact_rows, github_repo, github_repo_dir, lean_rows};

    #[test]
    fn rows_drop_the_registry_url_and_a_homepage_that_is_the_repository() {
        let mut rows = serde_json::json!([
            {"name":"zod","registryUrl":"https://registry.npmjs.org/zod",
             "homepage":"https://github.com/colinhacks/zod#readme",
             "repository":"https://github.com/colinhacks/zod"},
            {"name":"x","homepage":"https://zod.dev","repository":"https://github.com/o/x"}
        ]);
        compact_rows(&mut rows);
        assert_eq!(
            rows,
            serde_json::json!([
                {"name":"zod","repository":"https://github.com/colinhacks/zod"},
                {"name":"x","homepage":"https://zod.dev","repository":"https://github.com/o/x"}
            ])
        );
    }

    #[test]
    fn exact_rows_keep_the_description_and_a_distinct_homepage() {
        let row = serde_json::json!({"type":"npm","name":"express","version":"4.21.2",
            "description":"Fast web framework","homepage":"http://expressjs.com/",
            "license":"MIT","repository":"https://github.com/expressjs/express",
            "publishedAt":"2024-12-05","engines":">= 0.10.0","dependencies":31});
        let mut exact = serde_json::json!([row.clone()]);
        compact_rows(&mut exact);
        lean_rows(&mut exact, super::ArtifactType::Npm);
        let mut expected = row;
        expected.as_object_mut().map(|row| row.remove("type"));
        assert_eq!(exact, serde_json::json!([expected]));
        // Only a homepage that is the repository page is a duplicate.
        let mut same = serde_json::json!([{"name":"x","description":"d",
            "homepage":"https://github.com/o/x#readme","repository":"https://github.com/o/x"}]);
        compact_rows(&mut same);
        lean_rows(&mut same, super::ArtifactType::Npm);
        assert_eq!(
            same,
            serde_json::json!([{"name":"x","description":"d","repository":"https://github.com/o/x"}])
        );
    }

    #[test]
    fn parses_monorepo_subdirectories() {
        let dir = |url| github_repo_dir(url);
        assert_eq!(
            dir("https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore").as_deref(),
            Some("crates/ignore")
        );
        assert_eq!(
            dir("https://github.com/o/r/tree/main/packages/x/").as_deref(),
            Some("packages/x")
        );
        assert_eq!(
            dir("https://github.com/o/r/blob/main/crates/a/Cargo.toml").as_deref(),
            Some("crates/a")
        );
        assert_eq!(dir("https://github.com/o/r/tree/main"), None);
        assert_eq!(dir("https://github.com/o/r#readme"), None);
        assert_eq!(dir("git+https://github.com/o/r.git"), None);
    }

    #[test]
    fn parses_registry_repository_urls() {
        let expected = Some(("o".to_owned(), "r".to_owned()));
        for url in [
            "git+https://github.com/o/r.git",
            "https://github.com/o/r",
            "git@github.com:o/r.git",
            "https://github.com/o/r/tree/main/packages/x",
            "github.com/o/r#readme",
        ] {
            assert_eq!(github_repo(url), expected, "{url}");
        }
        assert_eq!(github_repo("https://gitlab.com/o/r"), None);
        assert_eq!(github_repo("https://github.com/o"), None);
    }
}

#[cfg(test)]
mod cursor_signing_tests {
    use super::*;
    use crate::providers::artifact::ArtifactType;
    use crate::runtime::cursor;
    use std::time::Duration;

    fn keyword_query(keywords: &[&str]) -> ArtifactSearchQuery {
        crate::providers::artifact::artifact_query(
            serde_json::json!({"type": ArtifactType::Npm, "keywords": keywords, "pageSize": 10}),
            None,
        )
    }

    #[test]
    fn issued_cursor_round_trips_and_is_scope_bound() {
        let key = cursor::user_signing_key(None);
        let scope = cursor_scope(&keyword_query(&["http"])).expect("scope");
        let state = r#"{"offset":30,"page":2}"#;
        let token = cursor::sign_state(key, &scope, state.as_bytes()).expect("sign");
        assert!(token.starts_with(cursor::SIGNED_STATE_PREFIX));
        assert_eq!(
            cursor::verify_state(key, &scope, &token).expect("verify"),
            state.as_bytes()
        );
        let continued = crate::providers::artifact::artifact_query(
            json!({"cursor": token, "mainGoal": "continue", "reasoning": "next page", "debug": true}),
            Some(&keyword_query(&["http"])),
        );
        let continued_scope = cursor_scope(&continued).expect("continuation scope");
        assert_eq!(continued_scope, scope);
        // A replay with a different brief keeps the same page scope.
        let rebriefed = crate::providers::artifact::artifact_query(
            json!({"cursor": continued.cursor(), "mainGoal": "other", "reasoning": "other"}),
            Some(&keyword_query(&["http"])),
        );
        assert_eq!(cursor_scope(&rebriefed).expect("rebriefed scope"), scope);
        assert_eq!(
            cursor::verify_state(key, &continued_scope, continued.cursor().expect("cursor"))
                .expect("issued continuation verifies"),
            state.as_bytes()
        );
        // Lifted onto a different query, the same token fails verification.
        let other_scope = cursor_scope(&keyword_query(&["json"])).expect("scope");
        assert!(cursor::verify_state(key, &other_scope, &token).is_err());
        // A tampered payload with the original tag fails verification.
        let forged = format!(
            "{}{}.{}",
            cursor::SIGNED_STATE_PREFIX,
            base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                r#"{"offset":9000,"page":900}"#
            ),
            token.rsplit('.').next().expect("tag")
        );
        assert!(cursor::verify_state(key, &scope, &forged).is_err());
    }

    /// An expired deadline separates cursor validation (runs first) from
    /// provider execution: a forged cursor must fail as `invalid_query`
    /// before any request budget is consulted, while accepted cursors reach
    /// the provider and fail on the dead budget instead.
    #[tokio::test]
    async fn forged_cursors_are_rejected_and_legacy_plus_signed_are_accepted() {
        let dead = Instant::now() - Duration::from_secs(1);
        let run = |cursor_value: String| {
            let query = json!({
                "type": "npm",
                "keywords": ["http"],
                "cursor": cursor_value,
                "mainGoal": "test", "reasoning": "test",
            });
            async move {
                execute(&query, dead, CancellationToken::new(), false, None, 0, true)
                    .await
                    .expect_err("dead budget or rejection")
            }
        };
        let forged = run("s1.eyJvZmZzZXQiOjMwLCJwYWdlIjoyfQ.deadbeef".into()).await;
        assert_eq!(forged.code, "invalid_query");
        // Legacy raw-JSON state (dual-accept window): passes cursor checks.
        let legacy = run(r#"{"offset":30,"page":2}"#.into()).await;
        assert_ne!(legacy.code, "invalid_query", "{legacy:?}");
        // A genuinely issued token for this query verifies and reaches the
        // provider.
        let key = cursor::user_signing_key(None);
        let typed = crate::providers::artifact::artifact_query(
            json!({"pageSize": null}),
            Some(&keyword_query(&["http"])),
        );
        let scope = cursor_scope(&typed).expect("scope");
        let token = cursor::sign_state(key, &scope, br#"{"offset":30,"page":2}"#).expect("sign");
        let query = json!({"type": "npm", "keywords": ["http"], "cursor": token, "mainGoal": "test", "reasoning": "test"});
        let issued = execute(&query, dead, CancellationToken::new(), false, None, 0, true)
            .await
            .expect_err("dead budget");
        assert_ne!(issued.code, "invalid_query", "{issued:?}");
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
            "type": "npm",
            "packageName": "audit-package",
            "registry": format!("http://127.0.0.1:{port}"),
            "mainGoal": "test", "reasoning": "test",
        });
        // Generous: building the system HTTP client (native root certs) can
        // be slow in sandboxed test environments.
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        run(
            &query,
            deadline,
            CancellationToken::new(),
            true,
            None,
            0,
            false,
            Some(npmrc.to_path_buf()),
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
            "type": "npm",
            "packageName": "no-such-package-zz",
            "registry": format!("http://127.0.0.1:{}", server.address().port()),
            "mainGoal": "test", "reasoning": "test",
        });
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        let error = run(
            &query,
            deadline,
            CancellationToken::new(),
            true,
            None,
            0,
            false,
            None,
        )
        .await
        .expect_err("missing package");
        assert_eq!(error.code, "notFound", "{error:?}");
        assert_eq!(error.status, Some(404));
        assert!(error.message.contains("no-such-package-zz"), "{error:?}");
    }

    /// D1/H1: the upstream-source lead reads the default branch (a published
    /// `gitHead` may never have been pushed upstream: GitHub 404/422), and a
    /// separate lower-confidence lead reads the commit the version was
    /// published from.
    #[tokio::test]
    async fn view_repo_reads_the_default_branch_and_the_git_head_separately() {
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
            "type": "npm",
            "packageName": "audit-package",
            "registry": format!("http://127.0.0.1:{}", server.address().port()),
            "mainGoal": "test", "reasoning": "test",
        });
        let deadline = Instant::now() + std::time::Duration::from_secs(300);
        let data = run(
            &query,
            deadline,
            CancellationToken::new(),
            true,
            None,
            0,
            false,
            None,
        )
        .await
        .expect("lookup succeeds");
        let view = &data["next"]["viewRepo"];
        assert!(view["query"].get("branch").is_none(), "{data}");
        assert_eq!(view["query"]["path"], "packages/x", "{data}");
        // Depth one is ghStructure's default listing.
        assert!(view["query"].get("maxDepth").is_none(), "{data}");
        // `source` states the lead's provenance; no separate confidence.
        assert!(view.get("confidence").is_none(), "{data}");
        assert_eq!(view["source"]["scope"], "defaultBranch", "{data}");
        assert_eq!(view["source"]["verification"], "unverified", "{data}");
        // The lead's reason is `why`; its query carries no invented brief.
        assert!(view["query"].get("reasoning").is_none(), "{data}");
        let why = view["why"].as_str().expect("why");
        assert!(why.contains("not release evidence"), "{data}");
        assert!(why.chars().count() <= 60, "{why}");
        let release = &data["next"]["viewReleaseSource"];
        assert_eq!(release["tool"], "ghStructure", "{data}");
        assert_eq!(release["query"]["branch"], sha, "{data}");
        assert_eq!(release["query"]["path"], "packages/x", "{data}");
        assert!(release.get("confidence").is_none(), "{data}");
        assert_eq!(release["source"]["scope"], "release", "{data}");
        assert_eq!(release["source"]["verification"], "unverified", "{data}");
        let row = data["artifacts"][0].as_object().expect("row");
        assert!(
            row.keys()
                .all(|key| key != "gitHead" && key != "sourceRef" && key != "registryUrl"),
            "{data}"
        ); // A pinned version's code is its release: the default-branch lead
        // would only repeat the release lead without its branch, which the
        // release lead's reasoning names as the recovery.
        let mut pinned = query.clone();
        pinned["version"] = json!("1.31.0");
        let data = run(
            &pinned,
            deadline,
            CancellationToken::new(),
            true,
            None,
            0,
            false,
            None,
        )
        .await
        .expect("pinned lookup succeeds");
        assert!(data["next"].get("viewRepo").is_none(), "{data}");
        let release = &data["next"]["viewReleaseSource"];
        assert_eq!(release["query"]["branch"], sha, "{data}");
        assert_eq!(release["source"]["verification"], "unverified", "{data}");
        let why = release["why"].as_str().expect("why");
        assert!(why.contains("omit branch"), "{data}");
        assert!(why.chars().count() <= 60, "{why}");
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
