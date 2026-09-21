//! Typed artifact registry discovery and exact metadata lookup.
use crate::providers::RequestBudget;
use crate::providers::artifact::{
    ArtifactError, ArtifactProviderContext, ArtifactQuery, SystemArtifactHttp, execute_artifact,
};
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub use crate::providers::artifact::{ArtifactItem, ArtifactType, ResolvedNpmRegistry};

/// Signature scope for a query's cursor: a digest of the normalized query
/// with the cursor itself removed, so a token lifted onto a different query
/// fails verification while every page of one query shares a scope.
fn cursor_scope(query: &ArtifactQuery) -> Result<String, ArtifactError> {
    let mut bare = query.clone();
    bare.cursor = None;
    let mut value = serde_json::to_value(&bare)
        .map_err(|_| ArtifactError::new("provider_error", "Failed to derive cursor scope."))?;
    if let Some(object) = value.as_object_mut() {
        object.retain(|_, v| !v.is_null());
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
    /// Config revision forwarded to the in-process registry HTTP cache.
    cache_revision: u64,
    /// When `false` the in-process registry HTTP cache is bypassed entirely
    /// (both reads and writes).  Mirrors `storage.mode == "persistent"`.
    cache_enabled: bool,
) -> Result<Value, ArtifactError> {
    let mut query = query.clone();
    if let Some(object) = query.as_object_mut() {
        object.remove("goal");
        object.remove("reasoning");
        object.remove("debug");
    }
    let mut query: ArtifactQuery = serde_json::from_value(query)
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
        query.cursor = Some(String::from_utf8(payload).map_err(|_| unrecognized_cursor())?);
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
            "perPage": query.page_size.unwrap_or(page.artifacts.len()),
            "returned": page.artifacts.len(),
            "hasMore": has_more,
            "totalFound": page.total,
        },
        "type": query.artifact_type,
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
        next.cursor = Some(cursor);
        if let Ok(mut next_query) = serde_json::to_value(next) {
            // Keyword-discovery queries leave packageName/registry as `None`,
            // which serialize to JSON `null` and fail the canonical
            // continuation contract (its exact-lookup branch expects strings).
            // Drop null fields so the query matches the keyword+cursor branch,
            // mirroring gh_search's `remove_nulls` before building `nextPage`.
            if let Some(object) = next_query.as_object_mut() {
                object.retain(|_, value| !value.is_null());
            }
            data["next"] = json!({
                "nextPage": {
                    "tool": "artifactSearch",
                    "query": next_query,
                    "confidence": "exact"
                }
            });
        }
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

#[cfg(test)]
mod cursor_signing_tests {
    use super::*;
    use crate::providers::artifact::ArtifactType;
    use crate::runtime::cursor;
    use std::time::Duration;

    fn keyword_query(keywords: &[&str]) -> ArtifactQuery {
        ArtifactQuery {
            artifact_type: ArtifactType::Npm,
            package_name: None,
            keywords: Some(keywords.iter().map(|s| (*s).to_owned()).collect()),
            page_size: Some(10),
            cursor: None,
            registry: None,
        }
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
            });
            async move {
                execute(&query, dead, CancellationToken::new(), false, None)
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
        let mut typed = keyword_query(&["http"]);
        typed.page_size = None;
        let scope = cursor_scope(&typed).expect("scope");
        let token = cursor::sign_state(key, &scope, br#"{"offset":30,"page":2}"#).expect("sign");
        let query = json!({"type": "npm", "keywords": ["http"], "cursor": token});
        let issued = execute(&query, dead, CancellationToken::new(), false, None)
            .await
            .expect_err("dead budget");
        assert_ne!(issued.code, "invalid_query");
    }
}
