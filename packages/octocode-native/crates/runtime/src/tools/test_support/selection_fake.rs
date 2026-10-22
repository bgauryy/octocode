//! One selection runtime for tests: any [`SelectionJudge`] over the default
//! selection limits, so a change to `Runtime` or `ExecutionContext` edits
//! one place.

use crate::tools::selection::{
    Decision, JudgeCall, JudgeFailure, Judgments, ProviderError, Runtime, SelectionJudge,
};
use octocode_contract_types::ErrorCode;
use std::time::{Duration, Instant};

/// A selection runtime judging with `judge` on `handle`, within `timeout`
/// (the judge's timeout and the request's deadline).
pub(crate) fn runtime(
    judge: impl SelectionJudge + 'static,
    handle: tokio::runtime::Handle,
    timeout: Duration,
) -> Runtime {
    Runtime {
        limits: crate::execution::SelectionLimits {
            timeout,
            ..crate::execution::RuntimeLimits::DEFAULT.selection
        },
        available: true,
        judge: std::sync::Arc::new(judge),
        handle,
        context: crate::execution::ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: Instant::now() + timeout,
            walk_threads: None,
            response_window: None,
            github_credential: None,
        },
    }
}

/// Selects every admitted candidate.
pub(crate) struct SelectAll;

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
pub(crate) const REFUSED: Option<ErrorCode> = Some(ErrorCode::ClassificationProviderError);
/// A provider code [`WordJudge`] fails with: a provider that is down (HTTP 503).
pub(crate) const DOWN: Option<ErrorCode> = Some(crate::providers::classification::CIRCUIT_OPEN);

/// Selects a candidate whose state says `keep`, excludes one that says
/// `drop`, else answers unknown; `fail` fails every call with that provider
/// code instead ([`REFUSED`] or [`DOWN`]).
pub(crate) struct WordJudge {
    pub(crate) fail: Option<ErrorCode>,
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
                let text = call.candidates[ordinal]
                    .state
                    .as_ref()
                    .map(serde_json::Value::to_string)
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
