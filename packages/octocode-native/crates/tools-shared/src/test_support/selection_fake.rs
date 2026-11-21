//! One selection runtime for tests: any [`SelectionJudge`] over the default
//! selection limits, so a change to `Runtime` or `ExecutionContext` edits
//! one place.

use crate::selection::{
    Decision, JudgeCall, JudgeFailure, Judgments, ProviderError, Runtime, SelectionJudge,
};
use octocode_contract_types::ErrorCode;
use std::time::{Duration, Instant};

/// A selection runtime judging with `judge` on `handle`, within the request
/// deadline `timeout`.
pub fn runtime(
    judge: impl SelectionJudge + 'static,
    handle: tokio::runtime::Handle,
    timeout: Duration,
) -> Runtime {
    Runtime {
        limits: octocode_runtime_base::execution::RuntimeLimits::DEFAULT.selection,
        available: true,
        judge: std::sync::Arc::new(judge),
        saved: std::sync::Arc::default(),
        handle,
        context: octocode_runtime_base::execution::ExecutionContext {
            cancellation: octocode_io::cancellation::Cancellation::new(),
            deadline: Some(Instant::now() + timeout),
            response_window: None,
            github_credential: None,
        },
    }
}

/// Selects every admitted candidate.
pub struct SelectAll;

impl SelectionJudge for SelectAll {
    fn identity(&self) -> serde_json::Value {
        serde_json::json!({"test":"select-all"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| (ordinal, Decision::Selected))
            .collect())
    }
}

/// A provider code [`WordJudge`] fails with: a refused credential (HTTP 401).
pub const REFUSED: Option<ErrorCode> = Some(ErrorCode::ClassificationProviderError);
/// A provider code [`WordJudge`] fails with: a provider that is down (HTTP 503).
pub const DOWN: Option<ErrorCode> =
    Some(octocode_runtime_base::providers::classification::CIRCUIT_OPEN);

/// Selects a candidate whose state says `keep`, excludes one that says
/// `drop`, else answers unknown; `fail` fails every call with that provider
/// code instead ([`REFUSED`] or [`DOWN`]).
pub struct WordJudge {
    pub fail: Option<ErrorCode>,
}

impl SelectionJudge for WordJudge {
    fn identity(&self) -> serde_json::Value {
        serde_json::json!({"test":"words"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        if let Some(code) = self.fail {
            let status = if Some(code) == REFUSED {
                "401 Unauthorized"
            } else {
                "503 Service Unavailable"
            };
            return Err(JudgeFailure::Provider(ProviderError {
                code,
                message: format!("Classification provider returned HTTP {status}"),
                hints: vec!["Check OCTOCODE_CLASSIFICATION_API and account access.".into()],
            }));
        }
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| {
                // A hit is judged by its own line, whatever its window holds.
                let text = call.candidates[ordinal]
                    .state
                    .as_ref()
                    .map(|state| state.get("target").unwrap_or(state).to_string())
                    .unwrap_or_default();
                let decision = if text.contains("keep") {
                    Decision::Selected
                } else if text.contains("drop") {
                    Decision::Excluded
                } else {
                    Decision::Unknown
                };
                (ordinal, decision)
            })
            .collect())
    }
}

/// A judge that prices one request per admitted candidate, as the evaluator
/// does: a plan over the page's `maxRequests` sends nothing and fails with
/// its price; within it, [`WordJudge`] decides, and `calls` counts the
/// requests it stands for.
#[derive(Default)]
pub struct CappedJudge {
    pub calls: std::sync::atomic::AtomicU64,
}

impl SelectionJudge for CappedJudge {
    fn identity(&self) -> serde_json::Value {
        serde_json::json!({"test":"capped"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        let planned = u64::try_from(call.admitted.len()).unwrap_or(u64::MAX);
        call.usage.requests_planned = Some(planned);
        call.usage.requests_ceiling = Some(planned);
        if planned > call.max_requests {
            return Err(JudgeFailure::Provider(ProviderError {
                code: ErrorCode::ClassificationOverBudget,
                message: format!("needs {planned} requests"),
                hints: Vec::new(),
            }));
        }
        self.calls
            .fetch_add(planned, std::sync::atomic::Ordering::Relaxed);
        WordJudge { fail: None }.select(call)
    }
}
