use super::super::github;
use super::{DomainDispatcher, local_capture_failure};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::{
    config::ConfigOutput,
    id::ToolId,
    policy::path::PathPolicy,
    response::outcome::DomainResult,
    security::{CheckedQuery, ContentSecurity},
    tools::source_capture::SourceReader,
};
use octocode_contract_types::ErrorCode;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn retained_access_error() -> crate::tools::judgment::ClassificationError {
    crate::tools::judgment::ClassificationError::new(
        ErrorCode::ClassificationContextUnavailable,
        "The saved source access identity changed or could not be verified. No saved evidence was returned.",
        "Check source access, then start a fresh request explicitly.",
    )
}

impl SourceReader for DomainDispatcher {
    fn paths(&self) -> &PathPolicy {
        &self.paths
    }

    fn security(&self) -> &ContentSecurity {
        &self.security
    }

    fn config(&self) -> &ConfigOutput {
        &self.config
    }

    fn limits(&self) -> &crate::execution::RuntimeLimits {
        &self.limits
    }

    fn home(&self) -> &Path {
        &self.home
    }

    fn handle(&self) -> &tokio::runtime::Handle {
        &self.handle
    }

    fn availability(&self) -> &crate::execution::availability::ToolAvailability {
        &self.availability
    }

    fn evaluator(&self) -> &crate::tools::judgment::Evaluator {
        &self.evaluator
    }

    fn execute(
        &self,
        tool: ToolId,
        query: &CheckedQuery,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        DomainDispatcher::execute(self, tool, query, context)
    }

    fn retained_github_identity(
        &self,
        context: &ExecutionContext,
    ) -> Result<Option<String>, crate::tools::judgment::ClassificationError> {
        let observed = self
            .github_access
            .identity()
            .map_err(|()| retained_access_error())?;
        self.validate_retained_github_identity(observed.as_deref(), context)?;
        Ok(observed)
    }

    fn validate_retained_github_identity(
        &self,
        expected: Option<&str>,
        context: &ExecutionContext,
    ) -> Result<(), crate::tools::judgment::ClassificationError> {
        let check = || {
            context.check().map_err(|error| {
                crate::tools::judgment::ClassificationError::new(
                    error.code(),
                    error.to_string(),
                    "Retry the request explicitly.",
                )
            })
        };
        check()?;
        let Some(expected) = expected else {
            return Ok(());
        };
        let identity = self.github_response_identity(context);
        check()?;
        if identity.as_ref().is_ok_and(|identity| identity == expected) {
            Ok(())
        } else {
            Err(retained_access_error())
        }
    }

    fn text_capture_limit(&self) -> usize {
        crate::tools::local_fetch::MAX_SOURCE_BYTES as usize
    }

    fn capture_local_text(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<(String, PathBuf, String), crate::tools::judgment::ClassificationError> {
        let parsed = serde_json::from_value(query.clone()).map_err(|_| {
            crate::tools::judgment::ClassificationError::new(
                ErrorCode::InvalidClassificationContext,
                "The local resource query could not be decoded.",
                "Inspect the localFetch schema.",
            )
        })?;
        let captured =
            crate::tools::local_fetch::capture_text(&parsed, self.paths.as_ref(), context);
        context.check().map_err(|error| {
            crate::tools::judgment::ClassificationError::new(
                error.code(),
                error.to_string(),
                "Retry the request explicitly.",
            )
        })?;
        let captured = captured.map_err(local_capture_failure)?;
        Ok((captured.text, captured.canonical, captured.snapshot))
    }

    /// Acquire source bytes through the same GitHub credential, commit
    /// resolution, byte cache and request budget as a file read.
    fn capture_github_content(
        &self,
        query: &Value,
        context: &ExecutionContext,
        max_bytes: usize,
    ) -> Result<octocode_github::ContentResponse, crate::tools::judgment::ClassificationError> {
        let acquired = self
            .github_services()
            .map_err(Clone::clone)
            .and_then(|services| {
                services.capture_content(
                    query,
                    context,
                    &self.handle,
                    max_bytes,
                    &self.github_access,
                )
            });
        acquired.map_err(|error| {
            let row = github::provider_error(error);
            crate::tools::judgment::ClassificationError {
                failure: row.failure,
                ..crate::tools::judgment::ClassificationError::with_hints(
                    row.data["errorCode"]
                        .as_str()
                        .and_then(crate::id::error_codes::parse)
                        .unwrap_or(ErrorCode::ClassificationContextFailed),
                    row.data["error"]
                        .as_str()
                        .unwrap_or("The GitHub source could not be acquired."),
                    row.data["hints"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect(),
                )
            }
        })
    }
}
