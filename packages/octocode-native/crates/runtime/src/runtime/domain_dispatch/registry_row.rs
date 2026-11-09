//! The rows that ask a public package registry or advisory database,
//! `artifactSearch` and `securityCheck`: a failed one is one row with its
//! provider calls, offering the same `next.retry` every GitHub failure does.
use super::*;
use crate::providers::artifact::ArtifactError;

impl DomainDispatcher {
    pub(super) fn execute_artifact(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        let row_query = query;
        let query = match dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        // Release tags are checked through the GitHub transport; when its
        // services cannot be built (a malformed API URL), every check says so
        // on its row instead of being skipped.
        let tags: Box<dyn crate::providers::artifact::ReleaseTags + '_> =
            match self.github_services() {
                Ok(services) => Box::new(services.release_tags(context, &self.github_access)),
                Err(error) => Box::new(crate::providers::artifact::FailedTags(
                    crate::providers::artifact::CheckFailure::Services(error.message.to_string()),
                )),
            };
        let env = self.config.credential_env();
        let call = crate::tools::artifact_search::ArtifactCall {
            deadline: crate::execution::cancel::deadline_of(context),
            cancellation: context.cancellation.clone(),
            allow_private_registry: self.config.resolved.network.allow_private_registry,
            timeout: self.limits.network_timeout,
            max_retries: self.limits.max_retries,
            answers: Some(&*self.provider_answers),
            tags: Some(&*tags),
            env: &env,
        };
        self.handle.block_on(async {
            Ok(registry_row(
                ToolId::ArtifactSearch,
                row_query,
                crate::tools::artifact_search::execute(query, call).await,
            ))
        })
    }

    pub(super) fn execute_security_check(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        let row_query = query;
        let query = match dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        // The repository operation reads GitHub with the row's own credential; when its
        // services cannot be built, the row says so instead of reading nothing.
        let reader: Box<dyn crate::tools::security_check::RepositoryRead + '_> =
            match self.github_services() {
                Ok(services) => Box::new(services.repository_reader(context, &self.github_access)),
                Err(error) => Box::new(crate::tools::security_check::FailedRepository(format!(
                    "GitHub could not be read: its transport could not be built ({}).",
                    error.message
                ))),
            };
        let call = crate::tools::security_check::SecurityCall {
            deadline: crate::execution::cancel::deadline_of(context),
            cancellation: context.cancellation.clone(),
            timeout: self.limits.network_timeout,
            max_retries: self.limits.max_retries,
            answers: Some(&*self.provider_answers),
            config: &self.config.resolved,
            github: Some(&*reader),
            osv_api: self.limits.osv_api,
        };
        self.handle.block_on(async {
            Ok(registry_row(
                ToolId::SecurityCheck,
                row_query,
                crate::tools::security_check::execute(query, call).await,
            ))
        })
    }
}

fn registry_row(tool: ToolId, query: &Value, answer: Result<Value, ArtifactError>) -> DomainResult {
    match answer {
        // A row a retry may complete (securityCheck's partial row when OSV did
        // not answer) offers the same row again, as a failed one does.
        Ok(data) => {
            let mut row = dispatch::data_result(data);
            dispatch::offer_retry(tool, query, &mut row.data);
            row
        }
        Err(error) => {
            let calls = error.calls;
            let mut row = dispatch::provider_failure(
                tool,
                error.message,
                error.code,
                error.hints,
                error.status,
                None,
            );
            if let Some(calls) = calls {
                crate::response::result::attach_calls(&mut row.data, *calls);
            }
            dispatch::offer_retry(tool, query, &mut row.data);
            row
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// B233 (RV-W3 F4): a securityCheck row that is Ok but partial and
    /// retryable (OSV did not answer) offers the same row again as
    /// `next.retry`; a complete row offers none.
    #[test]
    fn a_retryable_partial_security_row_offers_next_retry() {
        let query = json!({"operation":"package","ecosystem":"npm","packageName":"x"});
        let partial = registry_row(
            ToolId::SecurityCheck,
            &query,
            Ok(json!({"isPartial":true,"retryable":true})),
        );
        assert_eq!(partial.data["next"]["retry"]["tool"], "securityCheck");
        assert_eq!(
            partial.data["next"]["retry"]["input"]["queries"][0], query,
            "{}",
            partial.data
        );
        let complete = registry_row(ToolId::SecurityCheck, &query, Ok(json!({"status":"ok"})));
        assert!(complete.data.get("next").is_none(), "{}", complete.data);
    }
}
