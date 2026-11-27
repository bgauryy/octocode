use super::super::github;
use super::DomainDispatcher;
use crate::runtime::tool_output::ToolOutputOf;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::{
    config::ConfigOutput,
    id::ToolId,
    policy::path::PathPolicy,
    response::outcome::DomainResult,
    security::{CheckedQuery, ContentSecurity},
};
use octocode_tools_shared::source_capture::{SourceReader, withheld_github_source};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn retained_access_error() -> octocode_tools_shared::judgment::ClassificationError {
    octocode_tools_shared::judgment::ClassificationError::new(
        ErrorCode::ClassificationContextUnavailable,
        "The saved source access identity changed or could not be verified. No saved evidence was returned.",
        "Check source access, then start a fresh request explicitly.",
    )
}

impl SourceReader for DomainDispatcher {
    // What a source reader captures goes to a provider.
    fn paths(&self) -> &PathPolicy {
        self.egress_paths.as_ref()
    }

    fn read_paths(&self) -> &PathPolicy {
        self.paths.as_ref()
    }

    fn security(&self) -> &ContentSecurity {
        &self.security
    }

    fn config(&self) -> &ConfigOutput {
        &self.config
    }

    fn limits(&self) -> &octocode_runtime_base::execution::RuntimeLimits {
        &self.limits
    }

    fn home(&self) -> &Path {
        &self.home
    }

    fn handle(&self) -> &tokio::runtime::Handle {
        &self.handle
    }

    fn availability(&self) -> &octocode_runtime_base::execution::availability::ToolAvailability {
        &self.availability
    }

    fn evaluator(&self) -> &octocode_tools_shared::judgment::Evaluator {
        &self.evaluator
    }

    fn output(
        &self,
        tool: ToolId,
    ) -> &'static dyn octocode_runtime_base::response::output::ToolOutput {
        tool.output()
    }

    fn execute(
        &self,
        tool: ToolId,
        query: &CheckedQuery,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        // A source reader's row goes to a classification provider.
        self.execute_row_under(tool, query, context, true, None)
    }

    fn artifact_preview_destinations(
        &self,
        query: &octocode_contract_types::ArtifactSearchQuery,
    ) -> Result<Vec<Value>, octocode_tools_shared::judgment::ClassificationError> {
        octocode_tool_packages::artifact_search::preview_destinations(
            query,
            &self.config.resolved.github.api_url,
        )
        .map_err(|error| {
            octocode_tools_shared::judgment::ClassificationError::new(
                error.code,
                error.message,
                "Correct the registry selection.",
            )
        })
    }

    fn retained_github_identity(
        &self,
        context: &ExecutionContext,
    ) -> Result<Option<String>, octocode_tools_shared::judgment::ClassificationError> {
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
    ) -> Result<(), octocode_tools_shared::judgment::ClassificationError> {
        let check = || {
            context.check().map_err(|error| {
                octocode_tools_shared::judgment::ClassificationError::new(
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
        octocode_engine::num::saturating_usize(octocode_tool_local::local_fetch::MAX_SOURCE_BYTES)
    }

    fn capture_local_text(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<(String, PathBuf, String), octocode_tools_shared::judgment::ClassificationError>
    {
        let parsed = serde_json::from_value(query.clone()).map_err(|_| {
            octocode_tools_shared::judgment::ClassificationError::new(
                ErrorCode::InvalidClassificationContext,
                "The local resource query could not be decoded.",
                "Inspect the localFetch schema.",
            )
        })?;
        let captured = octocode_tool_local::local_fetch::capture_text(
            &parsed,
            self.egress_paths.as_ref(),
            context,
        );
        context.check().map_err(|error| {
            octocode_tools_shared::judgment::ClassificationError::new(
                error.code(),
                error.to_string(),
                "Retry the request explicitly.",
            )
        })?;
        let captured = captured.map_err(local_capture_failure)?;
        Ok((captured.text, captured.canonical, captured.snapshot))
    }

    /// Acquire source bytes through the same GitHub credential, commit
    /// resolution and request budget as a file read.
    fn capture_github_content(
        &self,
        query: &Value,
        context: &ExecutionContext,
        max_bytes: usize,
    ) -> Result<
        octocode_github::ContentResponse,
        octocode_tools_shared::judgment::ClassificationError,
    > {
        if let Some(withheld) = withheld_github_source(query, &self.paths) {
            return Err(withheld);
        }
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
                    // The bytes of a symlink are its target's: a target the
                    // egress withholds is never asked for.
                    &|target| {
                        withheld_github_source(&serde_json::json!({"path": target}), &self.paths)
                            .is_some()
                    },
                )
            });
        let read = acquired.map_err(|error| {
            let row = github::provider_error(error);
            octocode_tools_shared::judgment::ClassificationError {
                failure: row.failure,
                ..octocode_tools_shared::judgment::ClassificationError::with_hints(
                    row.data["errorCode"]
                        .as_str()
                        .and_then(octocode_runtime_base::id::error_codes::parse)
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
        })?;
        match read {
            Ok((read, _)) => Ok(read),
            // A withheld target is refused by its own name, as a capture of
            // that path is.
            Err(target) => Err(
                withheld_github_source(&serde_json::json!({"path": &target}), &self.paths)
                    .unwrap_or_else(|| {
                        octocode_tools_shared::judgment::ClassificationError::new(
                            ErrorCode::PathPolicyDenied,
                            format!(
                                "The symlink resolves to '{target}', which the security path policy withholds from classification: nothing was fetched or sent to the provider."
                            ),
                            "Classify another file.",
                        )
                    }),
            ),
        }
    }
}

/// The error row of a GitHub read whose text would reach a provider and whose
/// name the egress withholds; its route follows the user's policy `reads`.
/// A refusal with no route carries no `next`.
pub(super) fn withheld_github_read(query: &Value, reads: &PathPolicy) -> Option<DomainResult> {
    let withheld = withheld_github_source(query, reads)?;
    let next =
        (!withheld.leads.is_empty()).then(|| Value::Object(withheld.leads.into_iter().collect()));
    Some(DomainResult::failure(
        withheld.code,
        withheld.message,
        withheld.hints,
        next,
    ))
}

/// A failed local capture as a classification error, with the reader's own
/// code, message, hints and leads (`viewTree`, `findFile`, `read`).
pub(super) fn local_capture_failure(
    result: Box<octocode_tools_shared::source_view::LocalFetchResult>,
) -> octocode_tools_shared::judgment::ClassificationError {
    let leads = result
        .next
        .as_ref()
        .and_then(|next| serde_json::to_value(next).ok())
        .and_then(|next| next.as_object().cloned())
        .map(|next| next.into_iter().collect())
        .unwrap_or_default();
    octocode_tools_shared::judgment::ClassificationError {
        leads,
        ..octocode_tools_shared::judgment::ClassificationError::with_hints(
            result
                .error_code
                .unwrap_or(ErrorCode::ClassificationContextUnavailable),
            result
                .error
                .unwrap_or_else(|| "The local source could not be captured.".into()),
            result.hints,
        )
    }
}
