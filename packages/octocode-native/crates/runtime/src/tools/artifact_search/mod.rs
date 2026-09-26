//! Typed artifact registry discovery and exact metadata lookup.
use crate::providers::RequestBudget;
use crate::providers::artifact::{
    ArtifactError, ArtifactProviderContext, ArtifactSearchQuery, SystemArtifactHttp,
    execute_artifact,
};
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub use crate::providers::artifact::{ArtifactItem, ArtifactSearchQueryType, ResolvedNpmRegistry};

/// Signature scope for a query's cursor: a digest of the normalized query
/// with the cursor itself removed, so a token lifted onto a different query
/// fails verification while every page of one query shares a scope.
fn cursor_scope(query: &ArtifactSearchQuery) -> Result<String, ArtifactError> {
    let mut bare = query.clone();
    bare.cursor = None;
    let mut value = serde_json::to_value(&bare)
        .map_err(|_| ArtifactError::new("provider_error", "Failed to derive cursor scope."))?;
    if let Some(object) = value.as_object_mut() {
        // Caller intent and diagnostics do not change which results a page
        // holds; a replayed page may restate them freely.
        for meta in ["goal", "reasoning", "debug"] {
            object.remove(meta);
        }
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
    let mut query: ArtifactSearchQuery = serde_json::from_value(query.clone())
        .map_err(|error| ArtifactError::new("invalid_query", error.to_string()))?;
    let signing_key = crate::runtime::cursor::user_signing_key(octocode_home);
    // Signed cursors (issued by us) must verify against this query's scope.
    // A legacy raw-JSON state is still accepted for one release (dual-accept
    // window) and remains range-clamped by the provider.
    if let Some(cursor) = query.cursor.clone()
        && cursor.starts_with(crate::runtime::cursor::SIGNED_STATE_PREFIX)
    {
        let scope = cursor_scope(&query)?;
        let payload = crate::runtime::cursor::verify_state(signing_key, &scope, &cursor)
            .map_err(|_| unrecognized_cursor())?;
        let state = String::from_utf8(payload).map_err(|_| unrecognized_cursor())?;
        query.cursor = Some(state.parse().map_err(|_| unrecognized_cursor())?);
    }
    let http = SystemArtifactHttp::new()?;
    let budget = RequestBudget {
        deadline,
        cancellation,
        max_body_bytes: 16 * 1024 * 1024,
    };
    let requested_registry = match query.registry.as_deref() {
        Some(raw) => {
            let base = url::Url::parse(raw).map_err(|_| {
                ArtifactError::new(
                    "invalid_query",
                    "Invalid npm registry URL: use HTTP(S) without credentials, query or fragment.",
                )
            })?;
            Some(ResolvedNpmRegistry {
                base: base.clone(),
                authorization: None,
                cache_identity: base.host_str().unwrap_or("npm").to_owned(),
            })
        }
        None => None,
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
    let mut data = json!({
        "artifacts": page.artifacts,
        "pagination": {
            "perPage": query.page_size().unwrap_or(page.artifacts.len()),
            "returned": page.artifacts.len(),
            "hasMore": has_more,
            "totalFound": page.total,
        },
        "type": query.type_,
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
        next.cursor = Some(cursor.parse().map_err(|_| {
            ArtifactError::new("provider_error", "Failed to encode pagination cursor.")
        })?);
        if let Ok(next_query) = serde_json::to_value(next) {
            data["next"] = json!({
                "nextPage": {
                    "tool": "artifactSearch",
                    "query": next_query,
                    "confidence": "exact"
                }
            });
        }
    }
    // An exact lookup whose source lives on GitHub continues straight to its
    // tree (package subdirectory when the registry names one). Registry
    // metadata can point at a fork or stale repo, so this is a lead, not proof.
    if query.package_name.is_some()
        && let Some(artifact) = page.artifacts.first()
        && let Some((owner, repo)) = artifact.repository.as_deref().and_then(github_repo)
    {
        data["next"]["viewRepo"] = json!({
            "tool": "ghSearch",
            "confidence": "high",
            "query": {
                "operation": "tree",
                "owner": owner,
                "repo": repo,
                "path": artifact
                    .repository_directory
                    .clone()
                    .or_else(|| artifact.repository.as_deref().and_then(github_repo_dir))
                    .unwrap_or_default(),
                "maxDepth": 1,
                "reasoning": "Inspect the package's upstream source tree.",
            },
        });
    }
    if let Some(limit) = page.terminal_limit {
        data["isPartial"] = json!(true);
        data["terminalLimit"] = json!(true);
        data["partialReasons"] = json!([limit]);
    }
    if page.artifacts.is_empty() {
        data["status"] = json!("empty");
        data["hints"] = json!([if query.package_name.is_some() {
            "Check the package name and ecosystem coordinate."
        } else {
            "Try fewer or broader keywords."
        }]);
    }
    Ok(data)
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
/// repository URL. The ref is dropped: refs may contain slashes, and registry
/// refs are often stale, so the viewRepo lead reads the default branch.
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
    use super::{github_repo, github_repo_dir};

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
    use crate::providers::artifact::ArtifactSearchQueryType;
    use crate::runtime::cursor;
    use std::time::Duration;

    fn keyword_query(keywords: &[&str]) -> ArtifactSearchQuery {
        crate::providers::artifact::artifact_query(
            serde_json::json!({"type": ArtifactSearchQueryType::Npm, "keywords": keywords, "pageSize": 10}),
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
                "reasoning": "test",
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
        assert_ne!(legacy.code, "invalid_query");
        // A genuinely issued token for this query verifies and reaches the
        // provider.
        let key = cursor::user_signing_key(None);
        let typed = crate::providers::artifact::artifact_query(
            json!({"pageSize": null}),
            Some(&keyword_query(&["http"])),
        );
        let scope = cursor_scope(&typed).expect("scope");
        let token = cursor::sign_state(key, &scope, br#"{"offset":30,"page":2}"#).expect("sign");
        let query =
            json!({"type": "npm", "keywords": ["http"], "cursor": token, "reasoning": "test"});
        let issued = execute(&query, dead, CancellationToken::new(), false, None, 0, true)
            .await
            .expect_err("dead budget");
        assert_ne!(issued.code, "invalid_query");
    }
}
