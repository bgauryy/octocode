//! Typed artifact registry discovery and exact metadata lookup.
use crate::providers::RequestBudget;
use crate::providers::artifact::{
    ArtifactError, ArtifactProviderContext, ArtifactQuery, SystemArtifactHttp, execute_artifact,
};
use serde_json::{Value, json};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

pub use crate::providers::artifact::{ArtifactItem, ArtifactType, ResolvedNpmRegistry};

pub async fn execute(
    query: &Value,
    deadline: Instant,
    cancellation: CancellationToken,
) -> Result<Value, ArtifactError> {
    let mut query = query.clone();
    if let Some(object) = query.as_object_mut() {
        object.remove("goal");
        object.remove("reasoning");
    }
    let query: ArtifactQuery = serde_json::from_value(query)
        .map_err(|error| ArtifactError::new("invalid_query", error.to_string()))?;
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
        },
    )
    .await?;
    let has_more = page.next_state.is_some();
    let mut data = json!({
        "type": query.artifact_type,
        "artifacts": page.artifacts,
        "pagination": {
            "perPage": query.page_size.unwrap_or(page.artifacts.len()),
            "returned": page.artifacts.len(),
            "hasMore": has_more,
            "totalFound": page.total,
        }
    });
    if let Some(registry) = page.registry {
        data["registry"] = json!(registry);
    }
    if let Some(state) = page.next_state {
        let cursor = serde_json::to_string(&state).unwrap_or_default();
        let mut next = query.clone();
        next.cursor = Some(cursor);
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
