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
        // Release tags are checked through the GitHub transport; without
        // usable GitHub services a lookup reads no tags.
        let services = self.github_services().ok();
        let tags = services.map(|services| services.release_tags(context, &self.github_access));
        let env = self.config.credential_env();
        let call = crate::tools::artifact_search::ArtifactCall {
            deadline: context.deadline,
            cancellation: context.cancellation.clone(),
            allow_private_registry: self.config.resolved.network.allow_private_registry,
            timeout: self.limits.network_timeout,
            max_retries: self.limits.max_retries,
            answers: Some(&*self.provider_answers),
            tags: tags
                .as_ref()
                .map(|tags| tags as &dyn crate::providers::artifact::ReleaseTags),
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
        let call = crate::tools::security_check::SecurityCall {
            deadline: context.deadline,
            cancellation: context.cancellation.clone(),
            timeout: self.limits.network_timeout,
            max_retries: self.limits.max_retries,
            answers: Some(&*self.provider_answers),
            config: &self.config.resolved,
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
        Ok(data) => dispatch::data_result(data),
        Err(error) => {
            let calls = error.calls;
            let mut row = dispatch::provider_failure(
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
